//! Scene geometry: where each tile goes and which part of its source it shows. Pure arithmetic,
//! no GPU.

use cm_core::CmError;

use super::{Bubble, BubbleShape, Layout, Ring, Scene};

/// Largest padding (EDT-3), as a percentage of the output width (and, proportionally, the height).
pub const MAX_PADDING_PCT: f32 = 20.0;
/// Largest corner radius (EDT-3), in output pixels.
pub const MAX_RADIUS_PX: f32 = 24.0;
/// Bubble safe margin to the frame edge, as a fraction of the shorter output side.
const BUBBLE_MARGIN: f32 = 0.02;
/// Corner radius of a rounded-square bubble, as a fraction of its side.
const ROUNDED_BUBBLE_RADIUS: f32 = 0.22;
/// Gap between the screen and the camera in the side-by-side layout, fraction of the width.
const SIDE_GAP: f32 = 0.015;

/// The area inside the frame after EDT-3 padding, `[x, y, w, h]` in output pixels. `padding_pct`
/// is a fraction of the frame width on the left and right; the top and bottom keep the same
/// fraction of the height, so a screen of the frame's own aspect gets a border that is
/// proportional on every side (10% on 1280x720: 128 px at the sides, 72 px above and below). A
/// screen of another aspect is fitted inside this area by `contain`: by width, falling back to
/// height when that would overflow, and centered.
pub fn padded_cell(frame: (u32, u32), padding_pct: f32) -> [f32; 4] {
    let (wf, hf) = (frame.0 as f32, frame.1 as f32);
    let p = padding_pct.clamp(0.0, MAX_PADDING_PCT) / 100.0;
    let (pad_x, pad_y) = ((p * wf).round(), (p * hf).round());
    [
        pad_x,
        pad_y,
        (wf - 2.0 * pad_x).max(1.0),
        (hf - 2.0 * pad_y).max(1.0),
    ]
}

/// One drawn rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tile {
    /// `[x, y, w, h]` in output pixels, snapped to whole pixels.
    pub rect: [f32; 4],
    /// The shown part of the source as `[u0, v0, u1, v1]` texture coordinates.
    pub src: [f32; 4],
    pub radius: f32,
    /// Shadow opacity, 0 for none.
    pub shadow: f32,
    pub ring: Option<Ring>,
    pub mirror: bool,
}

/// The result of [`plan`]: `None` means the tile is not drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Plan {
    pub screen: Option<Tile>,
    pub camera: Option<Tile>,
}

/// Ease-in-out for a layout change (`0..=1` in, `0..=1` out).
fn ease(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// A tile shrunk to nothing at the middle of `t`: what a camera grows from or into when it is
/// absent on the other side of a layout change.
fn collapsed(t: &Tile) -> Tile {
    Tile {
        rect: [
            t.rect[0] + t.rect[2] / 2.0,
            t.rect[1] + t.rect[3] / 2.0,
            0.0,
            0.0,
        ],
        radius: 0.0,
        shadow: 0.0,
        ring: t.ring.map(|r| Ring { width_px: 0.0, ..r }),
        ..*t
    }
}

fn blend_tile(a: &Tile, b: &Tile, t: f32) -> Tile {
    let ring = match (a.ring, b.ring) {
        (None, None) => None,
        (ra, rb) => {
            let color = rb.or(ra).map_or([0; 4], |r| r.color);
            Some(Ring {
                width_px: lerp(
                    ra.map_or(0.0, |r| r.width_px),
                    rb.map_or(0.0, |r| r.width_px),
                    t,
                ),
                color,
            })
        }
    };
    let mut rect = [0.0; 4];
    let mut src = [0.0; 4];
    for i in 0..4 {
        rect[i] = lerp(a.rect[i], b.rect[i], t).round();
        src[i] = lerp(a.src[i], b.src[i], t);
    }
    rect[2] = rect[2].max(1.0);
    rect[3] = rect[3].max(1.0);
    Tile {
        rect,
        src,
        radius: lerp(a.radius, b.radius, t),
        shadow: lerp(a.shadow, b.shadow, t),
        ring,
        mirror: b.mirror,
    }
}

/// CAM-3.AC3: the tiles part way (`t` from 0 to 1, eased) through a smooth change from the layout
/// `from` to the scene's own layout. The camera moves and resizes between its two places (it grows
/// from, or shrinks into, its own middle when it is absent on one side); the screen is drawn at
/// its place on whichever side has it, so the camera moves over it instead of fading. At `t >= 1`
/// this is `plan(scene)`, at `t <= 0` it is the plan of `from`.
pub fn plan_blend(scene: &Scene<'_>, from: Layout, t: f32) -> Result<Plan, CmError> {
    let to_plan = plan(scene)?;
    if t >= 1.0 || !t.is_finite() {
        return Ok(to_plan);
    }
    let mut older = *scene;
    older.layout = from;
    let from_plan = plan(&older)?;
    if t <= 0.0 {
        return Ok(from_plan);
    }
    let e = ease(t);
    let camera = match (from_plan.camera, to_plan.camera) {
        (Some(a), Some(b)) => Some(blend_tile(&a, &b, e)),
        (None, Some(b)) => Some(blend_tile(&collapsed(&b), &b, e)),
        (Some(a), None) => Some(blend_tile(&a, &collapsed(&a), e)),
        (None, None) => None,
    };
    let screen = match (from_plan.screen, to_plan.screen) {
        (Some(a), Some(b)) => Some(blend_tile(&a, &b, e)),
        (a, b) => b.or(a),
    };
    Ok(Plan { screen, camera })
}

/// Computes the tiles of a scene. Validates sizes, frames and ranges.
pub fn plan(scene: &Scene<'_>) -> Result<Plan, CmError> {
    let (w, h) = scene.output;
    if w == 0 || h == 0 || w > 16_384 || h > 16_384 {
        return Err(CmError::invalid_field(
            "output",
            format!("output size {w}x{h} is out of range"),
        ));
    }
    for (name, v) in [
        ("padding_pct", scene.padding_pct),
        ("radius_px", scene.radius_px),
    ] {
        if !v.is_finite() {
            return Err(CmError::invalid_field(name, "must be a finite number"));
        }
    }
    let content = padded_cell((w, h), scene.padding_pct);
    let radius = scene.radius_px.clamp(0.0, MAX_RADIUS_PX);
    let shadow = if scene.shadow { 0.35 } else { 0.0 };

    if let Some(s) = &scene.screen {
        s.frame.validate("screen")?;
    }
    if let Some(c) = &scene.camera {
        c.frame.validate("camera")?;
    }
    if let Some(a) = &scene.annotations {
        a.validate("annotations")?;
    }
    if let Some(c) = &scene.cursor {
        validate_cursor(c)?;
    }

    let screen_tile = |cell: [f32; 4]| -> Result<Tile, CmError> {
        let s = scene
            .screen
            .as_ref()
            .ok_or_else(|| CmError::invalid_input("this layout needs a screen layer"))?;
        let crop = s.crop.unwrap_or([0.0, 0.0, 1.0, 1.0]);
        validate_crop(crop)?;
        let src_aspect = (crop[2] * s.frame.width as f32) / (crop[3] * s.frame.height as f32);
        Ok(Tile {
            rect: contain(cell, src_aspect),
            src: [crop[0], crop[1], crop[0] + crop[2], crop[1] + crop[3]],
            radius,
            shadow,
            ring: None,
            mirror: false,
        })
    };
    let camera_cover = |cell: [f32; 4], radius: f32, shadow: f32, ring: Option<Ring>| {
        let c = scene.camera.as_ref()?;
        let cam_aspect = c.frame.width as f32 / c.frame.height as f32;
        Some(Tile {
            rect: cell,
            src: cover_src(cam_aspect, cell[2] / cell[3]),
            radius,
            shadow,
            ring,
            mirror: c.mirror,
        })
    };

    match scene.layout {
        Layout::ScreenOnly => Ok(Plan {
            screen: Some(screen_tile(content)?),
            camera: None,
        }),
        Layout::ScreenBubble(b) => {
            let screen = screen_tile(content)?;
            let camera = bubble_cell(&b, w, h).and_then(|(cell, r)| {
                camera_cover(cell, r, if scene.shadow { 0.3 } else { 0.0 }, b.ring)
            });
            Ok(Plan {
                screen: Some(screen),
                camera,
            })
        }
        Layout::CameraFull => {
            let camera = camera_cover(content, radius, shadow, None)
                .ok_or_else(|| CmError::invalid_input("this layout needs a camera layer"))?;
            Ok(Plan {
                screen: None,
                camera: Some(camera),
            })
        }
        Layout::SideBySide => {
            if scene.camera.is_none() {
                return Ok(Plan {
                    screen: Some(screen_tile(content)?),
                    camera: None,
                });
            }
            let gap = (SIDE_GAP * w as f32).round();
            let avail = (content[2] - gap).max(2.0);
            let left_w = (avail * 2.0 / 3.0).round();
            let right_w = avail - left_w;
            let left = [content[0], content[1], left_w, content[3]];
            let right = [content[0] + left_w + gap, content[1], right_w, content[3]];
            Ok(Plan {
                screen: Some(screen_tile(left)?),
                camera: camera_cover(right, radius, shadow, None),
            })
        }
    }
}

fn validate_cursor(c: &super::CursorLayer<'_>) -> Result<(), CmError> {
    let finite = [c.pos.0, c.pos.1, c.scale, c.opacity, c.effect_radius_px]
        .iter()
        .all(|v| v.is_finite());
    if !finite || c.scale <= 0.0 || !(0.0..=1.0).contains(&c.opacity) {
        return Err(CmError::invalid_field(
            "cursor",
            "position, scale and opacity must be finite, with a positive scale and an opacity from 0 to 1",
        ));
    }
    if let Some(s) = &c.sprite {
        s.frame.validate("cursor.sprite")?;
        if s.frame.format != super::PixelFormat::Rgba8 {
            return Err(CmError::invalid_field(
                "cursor.sprite",
                "the cursor sprite must be RGBA8",
            ));
        }
        if !(s.hotspot.0.is_finite() && s.hotspot.1.is_finite()) {
            return Err(CmError::invalid_field(
                "cursor.sprite",
                "the hotspot must be finite",
            ));
        }
    }
    for m in c.clicks {
        if !(m.pos.0.is_finite() && m.pos.1.is_finite() && m.progress.is_finite()) {
            return Err(CmError::invalid_field("cursor.clicks", "must be finite"));
        }
    }
    Ok(())
}

fn validate_crop(c: [f32; 4]) -> Result<(), CmError> {
    let ok = c.iter().all(|v| v.is_finite())
        && c[0] >= 0.0
        && c[1] >= 0.0
        && c[2] > 0.0
        && c[3] > 0.0
        && c[0] + c[2] <= 1.0 + 1e-4
        && c[1] + c[3] <= 1.0 + 1e-4;
    if ok {
        Ok(())
    } else {
        Err(CmError::invalid_field(
            "crop",
            "the crop rectangle must lie inside the frame",
        ))
    }
}

/// The largest rectangle of the given aspect (w / h) that fits in `cell`, centered.
fn contain(cell: [f32; 4], aspect: f32) -> [f32; 4] {
    let cell_aspect = cell[2] / cell[3];
    let (w, h) = if aspect > cell_aspect {
        (cell[2], cell[2] / aspect)
    } else {
        (cell[3] * aspect, cell[3])
    };
    let (w, h) = (w.round().max(1.0), h.round().max(1.0));
    [
        (cell[0] + (cell[2] - w) / 2.0).round(),
        (cell[1] + (cell[3] - h) / 2.0).round(),
        w,
        h,
    ]
}

/// The centered part of a source of aspect `src_aspect` that fills a tile of `tile_aspect`
/// without distortion, as `[u0, v0, u1, v1]`.
fn cover_src(src_aspect: f32, tile_aspect: f32) -> [f32; 4] {
    if src_aspect > tile_aspect {
        let f = tile_aspect / src_aspect;
        [(1.0 - f) / 2.0, 0.0, (1.0 + f) / 2.0, 1.0]
    } else {
        let f = src_aspect / tile_aspect;
        [0.0, (1.0 - f) / 2.0, 1.0, (1.0 + f) / 2.0]
    }
}

/// The bubble rectangle and corner radius in output pixels, clamped fully inside the frame.
fn bubble_cell(b: &Bubble, w: u32, h: u32) -> Option<([f32; 4], f32)> {
    let (wf, hf) = (w as f32, h as f32);
    let short = wf.min(hf);
    let size = if b.size.is_finite() { b.size } else { 0.2 };
    let d = (size.clamp(0.05, 1.0) * short).round().max(2.0);
    let margin = (BUBBLE_MARGIN * short).round();
    let clamp_axis = |center: f32, total: f32| {
        let lo = d / 2.0 + margin;
        let hi = total - d / 2.0 - margin;
        if lo >= hi {
            total / 2.0
        } else {
            center.clamp(lo, hi)
        }
    };
    let cx = clamp_axis(
        if b.center.0.is_finite() {
            b.center.0 * wf
        } else {
            0.0
        },
        wf,
    );
    let cy = clamp_axis(
        if b.center.1.is_finite() {
            b.center.1 * hf
        } else {
            hf
        },
        hf,
    );
    let radius = match b.shape {
        BubbleShape::Circle => d / 2.0,
        BubbleShape::Rounded => (d * ROUNDED_BUBBLE_RADIUS).round(),
    };
    Some((
        [(cx - d / 2.0).round(), (cy - d / 2.0).round(), d, d],
        radius,
    ))
}

#[cfg(test)]
mod tests {
    use super::super::{Background, CameraLayer, Frame, ScreenLayer};
    use super::*;

    fn px(w: u32, h: u32) -> Vec<u8> {
        vec![0; (w * h * 4) as usize]
    }

    fn scene<'a>(out: (u32, u32), s: &'a [u8], sw: u32, sh: u32) -> Scene<'a> {
        Scene {
            output: out,
            background: Background::None,
            padding_pct: 0.0,
            radius_px: 0.0,
            shadow: false,
            screen: Some(ScreenLayer::new(Frame::rgba(sw, sh, s))),
            camera: None,
            layout: Layout::ScreenOnly,
            annotations: None,
            cursor: None,
            captions: None,
        }
    }

    /// EDT-3.AC1: 8% padding insets the screen by 8% of the frame width at the left and right and
    /// keeps the aspect, so the border is proportional on every side.
    #[test]
    fn edt3_ac1_padding_is_a_percentage_of_the_width() {
        let s = px(160, 90);
        let mut sc = scene((1920, 1080), &s, 160, 90);
        sc.padding_pct = 8.0;
        sc.radius_px = 12.0;
        let p = plan(&sc).unwrap();
        let t = p.screen.unwrap();
        // 8% of 1920 = 153.6 -> 154 at the sides; the 16:9 screen is fitted by width in the
        // 1612 x 908 area (8% of 1080 = 86.4 -> 86 above and below) and centered.
        assert_eq!(t.rect[0], 154.0);
        assert_eq!(t.rect[2], 1612.0);
        assert_eq!(t.rect[3], (1612.0_f32 * 9.0 / 16.0).round());
        assert_eq!(t.radius, 12.0);
        let left = t.rect[0];
        let right = 1920.0 - (t.rect[0] + t.rect[2]);
        assert!((left - right).abs() <= 1.0, "centered: {left} vs {right}");
        let (top, bottom) = (t.rect[1], 1080.0 - (t.rect[1] + t.rect[3]));
        assert!((top - bottom).abs() <= 1.0, "centered: {top} vs {bottom}");
        assert!((top - 86.0).abs() <= 1.0, "top border {top}");
    }

    /// The owner's example: 10% on 1280x720 gives 128 px at the sides and 72 px above and below.
    #[test]
    fn edt3_ten_percent_on_720p_is_128_by_72() {
        let s = px(160, 90);
        let mut sc = scene((1280, 720), &s, 160, 90);
        sc.padding_pct = 10.0;
        let t = plan(&sc).unwrap().screen.unwrap();
        assert_eq!(t.rect, [128.0, 72.0, 1024.0, 576.0]);
    }

    /// A screen that is taller than the padded area is fitted by height instead of overflowing.
    #[test]
    fn edt3_tall_screen_falls_back_to_height() {
        let s = px(90, 160);
        let mut sc = scene((1280, 720), &s, 90, 160);
        sc.padding_pct = 10.0;
        let t = plan(&sc).unwrap().screen.unwrap();
        assert_eq!(t.rect[3], 576.0);
        assert_eq!(t.rect[1], 72.0);
        assert_eq!(t.rect[2], 324.0);
        assert_eq!(t.rect[0], (1280.0_f32 - 324.0) / 2.0);
    }

    /// EDT-3.AC3: no padding and the same aspect fills the frame edge to edge.
    #[test]
    fn edt3_ac3_no_padding_fills_the_frame() {
        let s = px(160, 90);
        let sc = scene((1920, 1080), &s, 160, 90);
        let t = plan(&sc).unwrap().screen.unwrap();
        assert_eq!(t.rect, [0.0, 0.0, 1920.0, 1080.0]);
        assert_eq!(t.src, [0.0, 0.0, 1.0, 1.0]);
    }

    /// EDT-3 edge case: a 16:9 screen in a 9:16 frame is fit by width and centered.
    #[test]
    fn edt3_portrait_output_fits_by_width_and_centers() {
        let s = px(160, 90);
        let sc = scene((1080, 1920), &s, 160, 90);
        let t = plan(&sc).unwrap().screen.unwrap();
        assert_eq!(t.rect[2], 1080.0);
        assert_eq!(t.rect[3], 608.0);
        assert_eq!(t.rect[1], ((1920.0 - 608.0) / 2.0_f32).round());
    }

    /// CAM-3.AC4: the bubble lies fully inside the frame wherever it is asked to be.
    #[test]
    fn cam3_ac4_bubble_is_clamped_inside_the_frame() {
        let s = px(160, 90);
        let c = px(64, 48);
        for out in [(1920, 1080), (1080, 1920), (1080, 1080)] {
            for center in [(0.0, 0.0), (1.0, 1.0), (-3.0, 7.0), (0.5, 0.5)] {
                let mut sc = scene(out, &s, 160, 90);
                sc.camera = Some(CameraLayer {
                    frame: Frame::rgba(64, 48, &c),
                    mirror: false,
                });
                sc.layout = Layout::ScreenBubble(Bubble {
                    size: 0.3,
                    center,
                    ..Bubble::bottom_left()
                });
                let t = plan(&sc).unwrap().camera.unwrap();
                assert!(t.rect[0] >= 0.0 && t.rect[1] >= 0.0, "{out:?} {center:?}");
                assert!(t.rect[0] + t.rect[2] <= out.0 as f32, "{out:?} {center:?}");
                assert!(t.rect[1] + t.rect[3] <= out.1 as f32, "{out:?} {center:?}");
            }
        }
    }

    /// The camera is center-cropped, not stretched, into a square bubble.
    #[test]
    fn bubble_crops_a_wide_camera_to_a_square() {
        let s = px(160, 90);
        let c = px(160, 90);
        let mut sc = scene((1920, 1080), &s, 160, 90);
        sc.camera = Some(CameraLayer {
            frame: Frame::rgba(160, 90, &c),
            mirror: true,
        });
        sc.layout = Layout::ScreenBubble(Bubble::bottom_left());
        let t = plan(&sc).unwrap().camera.unwrap();
        assert!(t.mirror);
        let (u0, u1) = (t.src[0], t.src[2]);
        assert!((u1 - u0 - 90.0 / 160.0).abs() < 1e-4);
        assert_eq!((t.src[1], t.src[3]), (0.0, 1.0));
    }

    /// CAM-3.AC3: a smooth change starts at the old layout, ends at the new one and moves the
    /// bubble in between (the camera grows from its own middle when it was hidden).
    #[test]
    fn cam3_ac3_blend_runs_from_the_old_layout_to_the_new_one() {
        let s = px(160, 90);
        let c = px(160, 90);
        let mut sc = scene((1920, 1080), &s, 160, 90);
        sc.camera = Some(CameraLayer {
            frame: Frame::rgba(160, 90, &c),
            mirror: false,
        });
        let corner = Bubble::bottom_left();
        let top_right = Bubble {
            center: (1.0, 0.0),
            ..corner
        };
        sc.layout = Layout::ScreenBubble(top_right);
        let from = Layout::ScreenBubble(corner);
        let old = plan(&{
            let mut o = sc;
            o.layout = from;
            o
        })
        .unwrap();
        let new = plan(&sc).unwrap();
        assert_eq!(plan_blend(&sc, from, 0.0).unwrap(), old);
        assert_eq!(plan_blend(&sc, from, 1.0).unwrap(), new);
        let mid = plan_blend(&sc, from, 0.5).unwrap().camera.unwrap();
        let (a, b) = (old.camera.unwrap().rect, new.camera.unwrap().rect);
        for i in 0..2 {
            assert!(
                (mid.rect[i] - (a[i] + b[i]) / 2.0).abs() <= 1.0,
                "the bubble is half way: {:?} between {a:?} and {b:?}",
                mid.rect
            );
        }
        // Screen only -> bubble: the camera grows from nothing at the middle of its place.
        let early = plan_blend(&sc, Layout::ScreenOnly, 0.1)
            .unwrap()
            .camera
            .unwrap();
        assert!(early.rect[2] < b[2] * 0.5, "starts small: {:?}", early.rect);
        // Bubble -> camera full: the screen stays under the growing camera until the end.
        sc.layout = Layout::CameraFull;
        let p = plan_blend(&sc, from, 0.5).unwrap();
        assert!(p.screen.is_some() && p.camera.is_some());
        assert!(plan_blend(&sc, from, 1.0).unwrap().screen.is_none());
    }

    #[test]
    fn camera_full_and_side_by_side_layouts() {
        let s = px(160, 90);
        let c = px(160, 90);
        let mut sc = scene((1920, 1080), &s, 160, 90);
        sc.camera = Some(CameraLayer {
            frame: Frame::rgba(160, 90, &c),
            mirror: false,
        });
        sc.layout = Layout::CameraFull;
        let p = plan(&sc).unwrap();
        assert!(p.screen.is_none());
        assert_eq!(p.camera.unwrap().rect, [0.0, 0.0, 1920.0, 1080.0]);

        sc.layout = Layout::SideBySide;
        let p = plan(&sc).unwrap();
        let (s, c) = (p.screen.unwrap(), p.camera.unwrap());
        assert!(s.rect[0] + s.rect[2] <= c.rect[0], "no overlap");
        assert!(c.rect[0] + c.rect[2] <= 1920.0);
        // The screen takes about two thirds of the width.
        assert!((s.rect[2] / (s.rect[2] + c.rect[2]) - 2.0 / 3.0).abs() < 0.03);
    }

    #[test]
    fn missing_layers_and_bad_input_are_rejected() {
        let s = px(160, 90);
        let mut sc = scene((1920, 1080), &s, 160, 90);
        sc.layout = Layout::CameraFull;
        assert!(plan(&sc).is_err(), "camera full without a camera");
        sc.layout = Layout::ScreenOnly;
        sc.screen = None;
        assert!(plan(&sc).is_err(), "screen layout without a screen");

        let mut sc = scene((0, 1080), &s, 160, 90);
        assert!(plan(&sc).is_err());
        sc.output = (1920, 1080);
        sc.padding_pct = f32::NAN;
        assert!(plan(&sc).is_err());
        sc.padding_pct = 0.0;
        sc.screen.as_mut().unwrap().crop = Some([0.5, 0.5, 0.8, 0.8]);
        assert!(plan(&sc).is_err(), "crop outside the frame");
        let short = vec![0u8; 10];
        let sc = scene((64, 64), &short, 160, 90);
        assert!(plan(&sc).is_err(), "wrong byte count");
    }

    #[test]
    fn padding_and_radius_are_clamped_to_the_spec_ranges() {
        let s = px(160, 90);
        let mut sc = scene((1000, 562), &s, 160, 90);
        sc.padding_pct = 90.0;
        sc.radius_px = 400.0;
        let t = plan(&sc).unwrap().screen.unwrap();
        assert_eq!(t.radius, MAX_RADIUS_PX);
        assert!(t.rect[0] >= 200.0 - 1.0, "padding capped at 20%");
        assert!(t.rect[1] >= 112.0 - 1.0, "and at 20% of the height");
    }

    /// A zoom crop changes the aspect the screen is fitted with.
    #[test]
    fn crop_sets_the_fitted_aspect() {
        let s = px(160, 90);
        let mut sc = scene((1000, 1000), &s, 160, 90);
        sc.screen.as_mut().unwrap().crop = Some([0.25, 0.0, 0.5625, 1.0]);
        let t = plan(&sc).unwrap().screen.unwrap();
        // 0.5625 * 160 = 90 px wide, 90 px tall: square.
        assert_eq!(t.rect, [0.0, 0.0, 1000.0, 1000.0]);
        assert_eq!(t.src, [0.25, 0.0, 0.8125, 1.0]);
    }
}
