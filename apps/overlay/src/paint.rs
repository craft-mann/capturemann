//! Software painting of one overlay window (DESIGN §5.1 Region picker): the frozen frame, the
//! scrim, selection chrome (the logo's crop brackets plus a thin `rec` outline), size chips,
//! magnifier and key hints.
//!
//! Everything here draws into a `0RGB` buffer the size of the display, so window pixel (x, y)
//! is display pixel (x, y) and the bright pixels inside a selection are exactly the frozen
//! frame (CAP-1.AC2). Tokens come from the design kit (DESIGN §2, `design/tokens/tokens.json`;
//! a test fails when they drift); values in logical px are multiplied by the display scale.

use cm_core::{PhysicalPoint, PhysicalRect};
use cm_i18n::tr;

use crate::canvas::{blend, lerp_color, Canvas, IRect};
use crate::magnifier::{cell, hex_color, loupe_layout, source_offset};
use crate::picker::{Mode, Picker};
use crate::selection::{handle_position, rect_bottom, rect_right, Handle};
use crate::text::TextPainter;

/// The kit's tokens (`design/tokens/tokens.json`) as `0x00RRGGBB`, and the picker's metrics.
pub mod tokens {
    /// `ink`: the window ground, and the chips' fill.
    pub const INK: u32 = 0x000A_0A0B;
    /// `surface`: floating panels.
    pub const SURFACE: u32 = 0x0011_1113;
    /// `raised`: keycaps.
    pub const RAISED: u32 = 0x001E_1E22;
    /// `line`: 1 px borders of chips, panels and keycaps.
    pub const LINE: u32 = 0x002E_2E34;
    pub const TEXT: u32 = 0x00ED_EDEF;
    pub const TEXT_2: u32 = 0x00C9_C9CF;
    pub const TEXT_3: u32 = 0x00A1_A1A9;
    /// `rec`: the selection outline (at [`OUTLINE_ALPHA`]).
    pub const REC: u32 = 0x00FF_4F3A;
    pub const WHITE: u32 = 0x00FF_FFFF;
    /// Panels are `surface` at 96% (the kit's `rgba(17,17,19,0.96)`).
    pub const PANEL_ALPHA: u32 = 245;
    /// The selection outline is `rec` at 70%.
    pub const OUTLINE_ALPHA: u32 = 179;
    /// A dark halo under the loupe so it reads over any content.
    pub const HALO_ALPHA: u32 = 153;
    /// `scrim` is rgba(0,0,0,0.55): the frame keeps 45% of its brightness.
    pub const SCRIM_KEEP_PERCENT: u32 = 45;
    /// Crop brackets (logical px): 4 px white strokes, 28 px arms, radius 6.
    pub const BRACKET_STROKE: f64 = 4.0;
    pub const BRACKET_ARM: f64 = 28.0;
    pub const BRACKET_RADIUS: f64 = 6.0;
    /// The `rec` outline along the selection edge (logical px).
    pub const OUTLINE_STROKE: f64 = 2.0;
}

/// Frames of at least this many pixels are converted on several threads.
const PARALLEL_MIN_PIXELS: usize = 1 << 20;

/// The frozen frame of one display, pre-computed bright and dimmed (`0RGB`).
pub struct Frame {
    pub width: u32,
    pub height: u32,
    bright: Vec<u32>,
    dimmed: Vec<u32>,
}

impl Frame {
    /// Builds a frame from tightly packed RGBA8 (alpha ignored). `None` if the length is wrong.
    ///
    /// A 4K display is 8.3 M pixels and this runs while the user waits, so the conversion is a
    /// table lookup per channel (exactly `c * 45 / 100`) split over a few threads.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Option<Frame> {
        let n = (width as usize).checked_mul(height as usize)?;
        if width == 0 || height == 0 || rgba.len() != n.checked_mul(4)? {
            return None;
        }
        let mut dim = [0u32; 256];
        for (c, d) in dim.iter_mut().enumerate() {
            *d = c as u32 * tokens::SCRIM_KEEP_PERCENT / 100;
        }
        // Zeroed allocations come lazily from the OS: no memset pass over 2 x 33 MB.
        let mut bright = vec![0u32; n];
        let mut dimmed = vec![0u32; n];
        let convert = |src: &[u8], bright: &mut [u32], dimmed: &mut [u32]| {
            for ((px, b), d) in src.as_chunks::<4>().0.iter().zip(bright).zip(dimmed) {
                let (r, g, bl) = (usize::from(px[0]), usize::from(px[1]), usize::from(px[2]));
                *b = ((r as u32) << 16) | ((g as u32) << 8) | bl as u32;
                *d = (dim[r] << 16) | (dim[g] << 8) | dim[bl];
            }
        };
        let threads = if n >= PARALLEL_MIN_PIXELS {
            std::thread::available_parallelism()
                .map_or(1, std::num::NonZeroUsize::get)
                .min(8)
        } else {
            1
        };
        if threads <= 1 {
            convert(rgba, &mut bright, &mut dimmed);
        } else {
            let per = n.div_ceil(threads);
            std::thread::scope(|scope| {
                for ((src, b), d) in rgba
                    .chunks(per * 4)
                    .zip(bright.chunks_mut(per))
                    .zip(dimmed.chunks_mut(per))
                {
                    scope.spawn(move || convert(src, b, d));
                }
            });
        }
        Some(Frame {
            width,
            height,
            bright,
            dimmed,
        })
    }

    /// The frame as it is first shown: the dimmed pixels (`0RGB`), row-major.
    pub fn dimmed(&self) -> &[u32] {
        &self.dimmed
    }

    /// The undimmed pixel at display (x, y).
    pub fn pixel(&self, x: i32, y: i32) -> Option<u32> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        self.bright
            .get((y as usize) * (self.width as usize) + x as usize)
            .copied()
    }

    /// RGBA8 bytes of `rect` cropped from the frozen frame (alpha 255). This is what a saved
    /// region must equal (CAP-1.AC2). `None` if the rect is not inside the frame.
    pub fn crop_rgba(&self, rect: PhysicalRect) -> Option<Vec<u8>> {
        let (x0, y0) = (rect.origin.x, rect.origin.y);
        let (x1, y1) = (rect_right(rect), rect_bottom(rect));
        if x0 < 0 || y0 < 0 || x1 > self.width as i32 || y1 > self.height as i32 {
            return None;
        }
        let mut out = Vec::with_capacity(rect.size.width as usize * rect.size.height as usize * 4);
        for y in y0..y1 {
            let row = (y as usize) * (self.width as usize);
            for &p in &self.bright[row + x0 as usize..row + x1 as usize] {
                out.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]);
            }
        }
        Some(out)
    }
}

/// Everything the painter needs besides the pixels.
pub struct Scene<'a> {
    pub picker: &'a Picker,
    pub scale: f64,
    pub text: &'a TextPainter,
    /// CAP-3 magnifier (Region mode only).
    pub show_loupe: bool,
    /// Key hints at the bottom center.
    pub show_hint: bool,
    /// Window mode: Tab leaves for the desktop app's window list (the hint says so).
    pub tab_to_list: bool,
}

fn dev(logical: f64, scale: f64) -> i32 {
    (logical * scale).round().max(1.0) as i32
}

fn irect(r: PhysicalRect) -> IRect {
    IRect::new(
        r.origin.x,
        r.origin.y,
        r.size.width as i32,
        r.size.height as i32,
    )
}

/// Paints the whole window. `canvas` must be the size of `frame`; returns false (and paints
/// nothing) otherwise.
pub fn paint(canvas: &mut Canvas<'_>, frame: &Frame, scene: &Scene<'_>) -> bool {
    if canvas.w != frame.width as i32 || canvas.h != frame.height as i32 {
        return false;
    }
    canvas.buf.copy_from_slice(&frame.dimmed);
    let scale = scene.scale;
    let picker = scene.picker;

    // 1. Crosshair until a selection starts (DESIGN §5.1: 1 px white at 50%).
    if picker.mode() == Mode::Region && !picker.has_selection() {
        if let Some(p) = picker.pixel() {
            canvas.hline(0, p.y, canvas.w, 0x00FF_FFFF, 128);
            canvas.vline(p.x, 0, canvas.h, 0x00FF_FFFF, 128);
        }
    }

    // 2. Hover highlight (window or display): bright inside, outline.
    if let Some(r) = picker.hover_rect() {
        reveal(canvas, frame, r);
        outline(canvas, r, scale);
    }

    // 3. Selection: bright inside, outline, handles, size readout.
    if let Some(sel) = picker.selection() {
        if sel.size.width > 0 && sel.size.height > 0 {
            reveal(canvas, frame, sel);
            outline(canvas, sel, scale);
            if !picker.is_drawing() {
                handles(canvas, sel, scale);
            }
            readout(canvas, sel, scene);
        }
    }

    // 4. Magnifier.
    if scene.show_loupe && picker.mode() == Mode::Region {
        if let Some(px) = picker.pixel() {
            loupe(canvas, frame, px, scene);
        }
    }

    // 5. Key hints.
    if scene.show_hint {
        hint(canvas, scene);
    }
    true
}

/// Copies the undimmed frame inside `r`.
fn reveal(canvas: &mut Canvas<'_>, frame: &Frame, r: PhysicalRect) {
    let x0 = r.origin.x.max(0);
    let x1 = rect_right(r).min(canvas.w);
    if x0 >= x1 {
        return;
    }
    let w = canvas.w as usize;
    for y in r.origin.y.max(0)..rect_bottom(r).min(canvas.h) {
        let row = (y as usize) * w;
        canvas.buf[row + x0 as usize..row + x1 as usize]
            .copy_from_slice(&frame.bright[row + x0 as usize..row + x1 as usize]);
    }
}

/// The selection or hover chrome (DESIGN §5.1): a 2 px `rec` outline at 70% along the edge, drawn
/// inside it (the capture is cut from the frozen frame, not from this buffer), and the four white
/// crop brackets just outside the corners.
fn outline(canvas: &mut Canvas<'_>, r: PhysicalRect, scale: f64) {
    let ir = irect(r);
    canvas.stroke_rect_inside(
        ir,
        dev(tokens::OUTLINE_STROKE, scale),
        tokens::REC,
        tokens::OUTLINE_ALPHA,
    );
    brackets(canvas, ir, scale);
}

/// The logo's crop brackets at the four corners of `r`: L-shaped, white, drawn just outside it.
fn brackets(canvas: &mut Canvas<'_>, r: IRect, scale: f64) {
    let stroke = dev(tokens::BRACKET_STROKE, scale);
    let arm = dev(tokens::BRACKET_ARM, scale);
    let radius = dev(tokens::BRACKET_RADIUS, scale) as f32;
    // A small selection must not be swallowed by its own brackets.
    let arm = arm.min((r.w.min(r.h) / 2 + stroke).max(stroke * 2));
    for (right, bottom) in [(false, false), (true, false), (false, true), (true, true)] {
        let x = if right {
            r.x + r.w + stroke - arm
        } else {
            r.x - stroke
        };
        let y = if bottom {
            r.y + r.h + stroke - arm
        } else {
            r.y - stroke
        };
        canvas.fill_bracket(
            IRect::new(x, y, arm, arm),
            (right, bottom),
            stroke,
            radius,
            tokens::WHITE,
        );
    }
}

/// White square handles on the edge midpoints (the corners are the brackets).
fn handles(canvas: &mut Canvas<'_>, r: PhysicalRect, scale: f64) {
    let s = dev(8.0, scale);
    let ring = dev(1.0, scale);
    for h in Handle::ALL {
        if !matches!(
            h,
            Handle::North | Handle::East | Handle::South | Handle::West
        ) {
            continue;
        }
        // Skip edge handles that would overlap the brackets on a small selection.
        let small = r.size.width < (6 * s) as u32 || r.size.height < (6 * s) as u32;
        if small {
            continue;
        }
        let p = handle_position(r, h);
        let hr = IRect::new(p.x - s / 2, p.y - s / 2, s, s);
        canvas.stroke_rect_outside(hr, ring, tokens::INK, 200);
        canvas.fill_rect(hr, tokens::WHITE, 255);
    }
}

/// A floating panel: `surface` at 96% with a 1 px `line` border outside it (DESIGN §4).
fn panel(canvas: &mut Canvas<'_>, r: IRect, radius: f32, scale: f64) {
    let t = dev(1.0, scale);
    canvas.fill_rrect(r.grow(t), radius + t as f32, tokens::LINE, 255);
    canvas.fill_rrect(r, radius, tokens::SURFACE, tokens::PANEL_ALPHA);
}

/// A size chip (DESIGN §4 Chips): Geist Mono 12, `ink` fill, 1 px `line`, radius 6, padding 4 8.
/// Returns the chip's right edge.
fn chip(
    canvas: &mut Canvas<'_>,
    x: i32,
    y: i32,
    label: &str,
    color: u32,
    scene: &Scene<'_>,
) -> i32 {
    let s = scene.scale;
    let px = 12.0 * s as f32;
    let tw = scene.text.measure_mono(label, px).ceil() as i32;
    let (padx, h) = (dev(8.0, s), dev(22.0, s));
    let r = IRect::new(x, y, tw + 2 * padx, h);
    let t = dev(1.0, s);
    canvas.fill_rrect(r.grow(t), dev(6.0, s) as f32 + t as f32, tokens::LINE, 255);
    canvas.fill_rrect(r, dev(6.0, s) as f32, tokens::INK, 255);
    let ty = y as f32 + (h as f32 - scene.text.line_height_mono(px)) / 2.0;
    scene
        .text
        .draw_mono(canvas, (x + padx) as f32, ty, label, px, color);
    r.x + r.w
}

/// The chips above the selection's top-left corner (DESIGN §5.1): "1334 × 972" and
/// "2× · native" (the scale factor; a picked area is always captured at physical resolution).
fn readout(canvas: &mut Canvas<'_>, sel: PhysicalRect, scene: &Scene<'_>) {
    let s = scene.scale;
    let size = format!("{} \u{d7} {}", sel.size.width, sel.size.height);
    let scale_label = scale_chip(s);
    let px = 12.0 * s as f32;
    let (padx, h, gap) = (dev(8.0, s), dev(22.0, s), dev(6.0, s));
    let w1 = scene.text.measure_mono(&size, px).ceil() as i32 + 2 * padx;
    let w2 = scene.text.measure_mono(&scale_label, px).ceil() as i32 + 2 * padx;
    let total = w1 + gap + w2;
    let mut x = sel.origin.x;
    let mut y = sel.origin.y - dev(tokens::BRACKET_STROKE, s) - gap - h;
    if y < 0 {
        // No room above: sit just inside the selection's top-left corner.
        y = sel.origin.y + gap;
        x += gap;
    }
    x = x.clamp(0, (canvas.w - total).max(0));
    let right = chip(canvas, x, y, &size, tokens::TEXT, scene);
    chip(canvas, right + gap, y, &scale_label, tokens::TEXT_3, scene);
}

/// "2× · native": the display scale as the kit writes it.
pub fn scale_chip(scale: f64) -> String {
    if (scale - scale.round()).abs() < 0.05 {
        format!("{}\u{d7} \u{b7} native", scale.round() as i64)
    } else {
        format!("{scale:.1}\u{d7} \u{b7} native")
    }
}

/// Anti-aliased coverage of a disc of radius `r` centered at (cx, cy) for pixel (x, y).
fn disc(x: i32, y: i32, cx: f32, cy: f32, r: f32) -> f32 {
    let d = (x as f32 + 0.5 - cx).hypot(y as f32 + 0.5 - cy);
    (r - d + 0.5).clamp(0.0, 1.0)
}

/// CAP-3 magnifier: 128 logical px circle at 8x with a pixel grid, outlined center pixel and a
/// `x, y  #RRGGBB` label.
fn loupe(canvas: &mut Canvas<'_>, frame: &Frame, px: PhysicalPoint, scene: &Scene<'_>) {
    let s = scene.scale;
    let layout = loupe_layout(px, scene.picker.size(), s);
    let d = layout.circle.size.width as i32;
    let (ox, oy) = (layout.circle.origin.x, layout.circle.origin.y);
    let (cxi, cyi) = (ox + d / 2, oy + d / 2);
    let (cx, cy) = (cxi as f32, cyi as f32);
    let ring = dev(2.0, s) as f32;
    let r_out = d as f32 / 2.0;
    let r_in = r_out - ring;
    let cell_px = cell(s);

    for y in oy - 1..oy + d + 1 {
        for x in ox - 1..ox + d + 1 {
            let outer = disc(x, y, cx, cy, r_out + 1.0);
            if outer <= 0.0 {
                continue;
            }
            // Shadow ring just outside, then the white ring, then the content.
            canvas.put(x, y, 0, (outer * tokens::HALO_ALPHA as f32) as u32);
            let face = disc(x, y, cx, cy, r_out);
            if face <= 0.0 {
                continue;
            }
            let inner = disc(x, y, cx, cy, r_in);
            let (dx, dy) = (x - cxi, y - cyi);
            let sx = px.x + source_offset(dx, cell_px);
            let sy = px.y + source_offset(dy, cell_px);
            let mut color = frame.pixel(sx, sy).unwrap_or(tokens::INK);
            // 1 px grid on cell borders.
            let gx = (dx + cell_px / 2).rem_euclid(cell_px) == 0;
            let gy = (dy + cell_px / 2).rem_euclid(cell_px) == 0;
            if gx || gy {
                color = blend(color, 0, 90);
            }
            let ringc = lerp_color(tokens::WHITE, color, (inner * 255.0) as u32);
            canvas.put(x, y, ringc, (face * 255.0) as u32);
        }
    }

    // Outline the center pixel: white inside, black outside.
    let c = IRect::new(cxi - cell_px / 2, cyi - cell_px / 2, cell_px, cell_px);
    let t = dev(1.0, s);
    canvas.stroke_rect_outside(c, t, 0, 220);
    canvas.stroke_rect_inside(c, t, tokens::WHITE, 255);

    // Label: swatch, "x, y", hex (Geist Mono: coordinates and values).
    let px_size = 12.0 * s as f32;
    let coords = format!("{}, {}", px.x, px.y);
    let center = frame.pixel(px.x, px.y).unwrap_or(0);
    let hex = hex_color(center);
    let gap = dev(8.0, s);
    let (tw1, tw2) = (
        scene.text.measure_mono(&coords, px_size).ceil() as i32,
        scene.text.measure_mono(&hex, px_size).ceil() as i32,
    );
    let sw = dev(12.0, s);
    let padx = dev(8.0, s);
    let w = padx + sw + gap + tw1 + gap + tw2 + padx;
    let slot = layout.label;
    let x = (slot.origin.x + (slot.size.width as i32 - w) / 2).clamp(0, (canvas.w - w).max(0));
    let r = IRect::new(x, slot.origin.y, w, slot.size.height as i32);
    panel(canvas, r, dev(6.0, s) as f32, s);
    let ty = r.y as f32 + (r.h as f32 - scene.text.line_height_mono(px_size)) / 2.0;
    let sy = r.y + (r.h - sw) / 2;
    canvas.fill_rrect(
        IRect::new(x + padx, sy, sw, sw),
        dev(3.0, s) as f32,
        center,
        255,
    );
    canvas.stroke_rect_inside(
        IRect::new(x + padx, sy, sw, sw),
        dev(1.0, s),
        0x00FF_FFFF,
        60,
    );
    let tx = (x + padx + sw + gap) as f32;
    scene
        .text
        .draw_mono(canvas, tx, ty, &coords, px_size, tokens::TEXT);
    scene.text.draw_mono(
        canvas,
        tx + (tw1 + gap) as f32,
        ty,
        &hex,
        px_size,
        tokens::TEXT_2,
    );
}

enum HintItem {
    /// A keycap: a keyboard glyph or key name, never translated.
    Key(&'static str),
    /// A label, translated when the hint is built.
    Label(String),
}

fn hint_items(scene: &Scene<'_>) -> Vec<HintItem> {
    use HintItem::{Key, Label};
    let p = scene.picker;
    match p.mode() {
        Mode::Region if p.is_drawing() => vec![Key("Esc"), Label(tr!("Cancel"))],
        Mode::Region if p.has_selection() => {
            vec![
                Key("Enter"),
                Label(tr!("Capture")),
                Key("Esc"),
                Label(tr!("Cancel")),
            ]
        }
        Mode::Region => vec![
            Label(tr!("Drag to select, click a window")),
            Key("Esc"),
            Label(tr!("Cancel")),
        ],
        Mode::Window if scene.tab_to_list => vec![
            Label(tr!("Click a window")),
            Key("Tab"),
            Label(tr!("Pick from list")),
            Key("Esc"),
            Label(tr!("Cancel")),
        ],
        Mode::Window => vec![
            Label(tr!("Click a window")),
            Key("Esc"),
            Label(tr!("Cancel")),
        ],
        Mode::Display => vec![
            Label(tr!("Click a display")),
            Key("Esc"),
            Label(tr!("Cancel")),
        ],
    }
}

/// Bottom-center pill with keycaps (DESIGN §4 Keycaps, §5.1): keycaps are Geist Mono 12 on
/// `raised`, radius 6, a 1 px `line` edge; labels are Geist.
fn hint(canvas: &mut Canvas<'_>, scene: &Scene<'_>) {
    let s = scene.scale;
    let items = hint_items(scene);
    let label_px = 12.0 * s as f32;
    let cap_px = 12.0 * s as f32;
    let (gap, padx, h) = (dev(8.0, s), dev(14.0, s), dev(36.0, s));
    let cap_h = dev(22.0, s);
    let cap_pad = dev(7.0, s);
    let width_of = |it: &HintItem| -> i32 {
        match it {
            HintItem::Key(k) => {
                (scene.text.measure_mono(k, cap_px).ceil() as i32 + 2 * cap_pad).max(cap_h)
            }
            HintItem::Label(l) => scene.text.measure(l, label_px).ceil() as i32,
        }
    };
    let total: i32 =
        items.iter().map(&width_of).sum::<i32>() + gap * (items.len() as i32 - 1) + 2 * padx;
    let x0 = (canvas.w - total) / 2;
    let y0 = canvas.h - dev(24.0, s) - h;
    let pill = IRect::new(x0.max(0), y0.max(0), total, h);
    panel(canvas, pill, h as f32 / 2.0, s);
    let mut x = pill.x + padx;
    for it in &items {
        let w = width_of(it);
        match it {
            HintItem::Key(k) => {
                let r = IRect::new(x, pill.y + (h - cap_h) / 2, w, cap_h);
                let t = dev(1.0, s);
                canvas.fill_rrect(r.grow(t), dev(6.0, s) as f32 + t as f32, tokens::LINE, 255);
                canvas.fill_rrect(r, dev(6.0, s) as f32, tokens::RAISED, 255);
                let tw = scene.text.measure_mono(k, cap_px);
                let ty = r.y as f32 + (cap_h as f32 - scene.text.line_height_mono(cap_px)) / 2.0;
                scene.text.draw_mono(
                    canvas,
                    r.x as f32 + (w as f32 - tw) / 2.0,
                    ty,
                    k,
                    cap_px,
                    tokens::TEXT_2,
                );
            }
            HintItem::Label(l) => {
                let ty = pill.y as f32 + (h as f32 - scene.text.line_height(label_px)) / 2.0;
                scene
                    .text
                    .draw(canvas, x as f32, ty, l, label_px, tokens::TEXT);
            }
        }
        x += w + gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::picker::Effect;
    use cm_core::PhysicalSize;

    const W: u32 = 400;
    const H: u32 = 300;

    /// A frame where each pixel's color encodes its position, so crops are checkable.
    fn pattern() -> Frame {
        let mut rgba = Vec::with_capacity((W * H * 4) as usize);
        for y in 0..H {
            for x in 0..W {
                rgba.extend_from_slice(&[
                    (x % 251) as u8 + 1,
                    (y % 251) as u8 + 1,
                    ((x + y) % 251) as u8 + 1,
                    255,
                ]);
            }
        }
        Frame::from_rgba(W, H, &rgba).unwrap()
    }

    fn picker() -> Picker {
        Picker::new(
            Mode::Region,
            PhysicalSize {
                width: W,
                height: H,
            },
            1.0,
            vec![],
            false,
        )
    }

    fn render(frame: &Frame, p: &Picker, loupe: bool, hint: bool) -> Vec<u32> {
        let text = TextPainter::new().unwrap();
        let mut buf = vec![0u32; (W * H) as usize];
        let mut c = Canvas::new(&mut buf, W, H).unwrap();
        let scene = Scene {
            picker: p,
            scale: 1.0,
            text: &text,
            show_loupe: loupe,
            show_hint: hint,
            tab_to_list: false,
        };
        assert!(paint(&mut c, frame, &scene));
        buf
    }

    fn drag(p: &mut Picker, a: (f64, f64), b: (f64, f64)) {
        p.pointer_moved(a.0, a.1);
        p.press();
        p.pointer_moved(b.0, b.1);
        assert_eq!(p.release(), Effect::Redraw);
    }

    #[test]
    fn frame_rejects_wrong_lengths() {
        assert!(Frame::from_rgba(2, 2, &[0; 15]).is_none());
        assert!(Frame::from_rgba(0, 2, &[]).is_none());
    }

    /// The multi-threaded conversion of a big frame gives exactly the per-pixel formula
    /// (`c * 45 / 100`), also across the chunk seams.
    #[test]
    fn large_frame_conversion_matches_the_formula() {
        let (w, h) = (1531u32, 811u32); // > 1 M pixels: the parallel path, odd chunk sizes
        let rgba: Vec<u8> = (0..w as usize * h as usize * 4)
            .map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
            .collect();
        let frame = Frame::from_rgba(w, h, &rgba).unwrap();
        for (i, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
            let (r, g, b) = (u32::from(px[0]), u32::from(px[1]), u32::from(px[2]));
            assert_eq!(frame.bright[i], (r << 16) | (g << 8) | b);
            assert_eq!(
                frame.dimmed()[i],
                ((r * 45 / 100) << 16) | ((g * 45 / 100) << 8) | (b * 45 / 100)
            );
        }
    }

    /// DESIGN §2: the scrim keeps 45% of the frame's brightness outside the selection.
    #[test]
    fn scrim_dims_to_forty_five_percent() {
        let frame = pattern();
        let buf = render(&frame, &picker(), false, false);
        let bright = frame.pixel(10, 10).unwrap();
        let dim = buf[(10 * W + 10) as usize];
        let ch = |v: u32, s: u32| (v >> s) & 255;
        for s in [16, 8, 0] {
            assert_eq!(ch(dim, s), ch(bright, s) * 45 / 100);
        }
    }

    /// CAP-1.AC2: inside the selection the screen shows the frozen frame untouched (outline
    /// drawn over the edge pixels only, handles on the border).
    #[test]
    fn cap1_ac2_selection_interior_is_the_frozen_frame() {
        let frame = pattern();
        let mut p = picker();
        drag(&mut p, (100.0, 80.0), (300.0, 220.0));
        let buf = render(&frame, &p, false, false);
        // Handles are 8 px squares centered on the border, so they reach 4 px inward.
        for y in 86i32..214 {
            for x in 106i32..294 {
                assert_eq!(
                    buf[(y as u32 * W + x as u32) as usize],
                    frame.pixel(x, y).unwrap(),
                    "({x},{y})"
                );
            }
        }
        // Just outside the outline is still dimmed.
        assert_ne!(buf[(79 * W + 50) as usize], frame.pixel(50, 79).unwrap());
    }

    /// DESIGN §5.1 / R4-14: the selection edge is a 2 px `rec` outline at 70% over the frozen
    /// frame, not the retired violet.
    #[test]
    fn selection_outline_is_rec_at_seventy_percent_inside_the_edge() {
        let frame = pattern();
        let mut p = picker();
        drag(&mut p, (100.0, 80.0), (300.0, 220.0));
        let buf = render(&frame, &p, false, false);
        let want = |x, y| {
            blend(
                frame.pixel(x, y).unwrap(),
                tokens::REC,
                tokens::OUTLINE_ALPHA,
            )
        };
        assert_eq!(buf[(120 * W + 100) as usize], want(100, 120));
        assert_eq!(buf[(120 * W + 101) as usize], want(101, 120), "2 px wide");
        assert_eq!(buf[(80 * W + 150) as usize], want(150, 80));
        assert_ne!(
            buf[(120 * W + 102) as usize],
            want(102, 120),
            "and no wider"
        );
    }

    /// DESIGN §5.1: white crop brackets (4 px, 28 px arms) sit just outside each corner and the
    /// edge between them is not drawn.
    #[test]
    fn crop_brackets_are_white_and_outside_the_corners() {
        let frame = pattern();
        let mut p = picker();
        drag(&mut p, (100.0, 80.0), (300.0, 220.0));
        let buf = render(&frame, &p, false, false);
        let at = |x: i32, y: i32| buf[(y as u32 * W + x as u32) as usize];
        // Top-left bracket: the horizontal arm above the selection edge, the vertical arm left of it.
        assert_eq!(at(110, 77), tokens::WHITE);
        assert_eq!(at(97, 90), tokens::WHITE);
        // Bottom-right bracket mirrors it.
        assert_eq!(at(290, 222), tokens::WHITE);
        assert_eq!(at(302, 210), tokens::WHITE);
        // Beyond the 28 px arm the scrim shows again; the middle of the edge has no bracket.
        assert_ne!(at(140, 77), tokens::WHITE);
        assert_ne!(at(240, 77), tokens::WHITE);
    }

    /// Hover candidates (window, display) get the same chrome as a selection.
    #[test]
    fn size_chip_text_is_the_physical_size_and_scale() {
        assert_eq!(scale_chip(2.0), "2\u{d7} \u{b7} native");
        assert_eq!(scale_chip(1.0), "1\u{d7} \u{b7} native");
        assert_eq!(scale_chip(1.5), "1.5\u{d7} \u{b7} native");
    }

    /// The painter's colors are the design kit's (`design/tokens/tokens.json`).
    #[test]
    fn tokens_match_the_design_kit() {
        const KIT: &str = include_str!("../../../design/tokens/tokens.json");
        let v: serde_json::Value = serde_json::from_str(KIT).unwrap();
        let hex = |name: &str| {
            let s = v["color"][name]["$value"].as_str().unwrap();
            u32::from_str_radix(s.trim_start_matches('#'), 16).unwrap()
        };
        assert_eq!(tokens::INK, hex("ink"));
        assert_eq!(tokens::SURFACE, hex("surface"));
        assert_eq!(tokens::RAISED, hex("raised"));
        assert_eq!(tokens::LINE, hex("line"));
        assert_eq!(tokens::TEXT, hex("text"));
        assert_eq!(tokens::TEXT_2, hex("text-2"));
        assert_eq!(tokens::TEXT_3, hex("text-3"));
        assert_eq!(tokens::REC, hex("rec"));
        // The retired Lens Violet is gone.
        assert_ne!(tokens::REC, 0x009D_8FFF);
    }

    /// CAP-1.AC2: crop of the frozen frame is the exact RGBA of that area.
    #[test]
    fn cap1_ac2_crop_matches_the_frame_pixels() {
        let frame = pattern();
        let rect = PhysicalRect {
            origin: PhysicalPoint { x: 7, y: 9 },
            size: PhysicalSize {
                width: 5,
                height: 3,
            },
        };
        let crop = frame.crop_rgba(rect).unwrap();
        assert_eq!(crop.len(), 5 * 3 * 4);
        let p = frame.pixel(9, 10).unwrap();
        let i = (5 + 2) * 4;
        assert_eq!(
            &crop[i..i + 4],
            &[(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]
        );
        let outside = PhysicalRect {
            origin: PhysicalPoint { x: 398, y: 0 },
            size: PhysicalSize {
                width: 5,
                height: 3,
            },
        };
        assert!(frame.crop_rgba(outside).is_none());
    }

    #[test]
    fn crosshair_is_drawn_until_a_selection_exists() {
        let frame = pattern();
        let mut p = picker();
        p.pointer_moved(200.0, 150.0);
        let with = render(&frame, &p, false, false);
        let without = {
            let q = picker();
            render(&frame, &q, false, false)
        };
        assert_ne!(
            with[(150 * W + 20) as usize],
            without[(150 * W + 20) as usize]
        );
        drag(&mut p, (100.0, 80.0), (300.0, 220.0));
        let sel = render(&frame, &p, false, false);
        // Row 150 far outside the selection is plain scrim again.
        assert_eq!(
            sel[(150 * W + 20) as usize],
            without[(150 * W + 20) as usize]
        );
    }

    /// CAP-3.AC1: the loupe's center cell shows the pointer pixel and its label has the coords.
    #[test]
    fn cap3_ac1_loupe_center_is_the_pointer_pixel() {
        let frame = pattern();
        let mut p = picker();
        p.pointer_moved(100.0, 200.0);
        let buf = render(&frame, &p, true, false);
        let layout = loupe_layout(p.pixel().unwrap(), p.size(), 1.0);
        // Pick a point inside the center cell but away from its outline.
        let (cx, cy) = (layout.circle.origin.x + 64, layout.circle.origin.y + 64);
        assert_eq!(
            buf[(cy as u32 * W + cx as u32) as usize],
            frame.pixel(100, 200).unwrap()
        );
        // One cell to the right is the next source pixel.
        let right = buf[(cy as u32 * W + (cx + 8) as u32) as usize];
        assert_eq!(right, frame.pixel(101, 200).unwrap());
    }

    #[test]
    fn hint_and_readout_paint_without_panicking_at_the_corners() {
        let frame = pattern();
        let mut p = picker();
        drag(&mut p, (0.0, 0.0), (399.0, 299.0));
        let _ = render(&frame, &p, true, true);
        let mut q = picker();
        q.pointer_moved(399.0, 299.0);
        let _ = render(&frame, &q, true, true);
    }

    #[test]
    fn paint_refuses_a_mismatched_canvas() {
        let frame = pattern();
        let p = picker();
        let text = TextPainter::new().unwrap();
        let mut buf = vec![0u32; 10 * 10];
        let mut c = Canvas::new(&mut buf, 10, 10).unwrap();
        let scene = Scene {
            picker: &p,
            scale: 1.0,
            text: &text,
            show_loupe: false,
            show_hint: false,
            tab_to_list: false,
        };
        assert!(!paint(&mut c, &frame, &scene));
    }
}
