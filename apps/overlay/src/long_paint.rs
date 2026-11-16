//! Draws the Long Shot bar and its Accessibility card (design board S1) into the software canvas.
//! The layout and every decision are in [`crate::long_bar`]; this only paints. Not run on a real
//! screen by the author (no display): the tests check that the pixels land where the layout says.

use crate::canvas::{Canvas, IRect};
use crate::icons::{self, Icon};
use crate::long_bar::{BarLayout, BarMode, Entry, Item};
use crate::paint::{dev, panel, tokens, Scene};

/// What the painter needs of the bar.
pub struct LongScene<'a> {
    pub layout: &'a BarLayout,
}

pub fn draw(canvas: &mut Canvas<'_>, scene: &Scene<'_>, long: &LongScene<'_>) {
    let s = scene.scale;
    let l = long.layout;
    // The card first: it sits above the bar and must not cover it.
    if let Some(card) = &l.card {
        panel(canvas, card.rect, dev(14.0, s) as f32, s);
        let title_px = (15.0 * s) as f32;
        let body_px = (13.0 * s) as f32;
        let (x, y, t) = &card.title;
        scene
            .text
            .draw(canvas, *x as f32, *y as f32, t, title_px, tokens::TEXT);
        for (x, y, t) in &card.body {
            scene
                .text
                .draw(canvas, *x as f32, *y as f32, t, body_px, tokens::TEXT_2);
        }
        for b in &card.buttons {
            button(canvas, scene, b, true);
        }
    }
    panel(canvas, l.bar, dev(16.0, s) as f32, s);
    // The segmented controls sit in an outlined, darker container.
    for seg in &l.segments {
        let r = dev(12.0, s) as f32;
        canvas.fill_rrect(*seg, r, tokens::LINE, 255);
        canvas.fill_rrect(seg.grow(-dev(1.0, s)), r - 1.0, tokens::INK, 255);
    }
    for e in &l.entries {
        button(canvas, scene, e, false);
        if let Some(k) = e.keycap_rect {
            keycap(canvas, k, s);
        }
    }
    for d in &l.dividers {
        canvas.fill_rect(d.0, tokens::LINE, 255);
    }
    // Design S24: the XS Preview tag above the top-right corner of the Long Shot and Click Guide
    // buttons. It is drawn last so the button's rectangle and label keep their size.
    for e in &l.entries {
        if matches!(
            e.item,
            Item::Mode(BarMode::LongShot) | Item::Mode(BarMode::ClickGuide)
        ) {
            preview_tag(canvas, scene, e.rect);
        }
    }
}

/// The XS Preview tag's colours (dark theme; the overlay is always dark): `preview.border`,
/// `preview.text`.
const PREVIEW_BORDER: u32 = 0x0055_555C;
const PREVIEW_TEXT: u32 = 0x00C9_C9CF;
/// The tag: 14 px high, 5 px padding, 9 px text (design S24, size XS).
pub const TAG_H: f64 = 14.0;
pub const TAG_PAD: f64 = 5.0;
pub const TAG_TEXT_PX: f64 = 9.0;

/// Where the tag goes: its right edge 2 px past the button's right edge, its bottom 4 px inside
/// the button's top edge. `text_w` is the label's width in device px.
pub fn preview_tag_rect(button: IRect, text_w: i32, scale: f64) -> IRect {
    let h = dev(TAG_H, scale);
    let w = text_w + 2 * dev(TAG_PAD, scale) + 2 * dev(1.0, scale);
    IRect::new(
        button.x + button.w + dev(2.0, scale) - w,
        button.y + dev(4.0, scale) - h,
        w,
        h,
    )
}

fn preview_tag(canvas: &mut Canvas<'_>, scene: &Scene<'_>, button: IRect) {
    let s = scene.scale;
    let px = (TAG_TEXT_PX * s) as f32;
    let label = cm_i18n::tr_ctx!("preview label", "Preview");
    let tw = scene.text.measure(&label, px);
    let r = preview_tag_rect(button, tw.ceil() as i32, s);
    let radius = r.h as f32 / 2.0;
    // The ground first (it hides the bar's border under the tag), then a dashed 1 px outline:
    // a solid ring whose top and bottom edges get 2 px gaps every 4 px.
    canvas.fill_rrect(r, radius, PREVIEW_BORDER, 255);
    canvas.fill_rrect(r.grow(-dev(1.0, s)), radius - 1.0, tokens::SURFACE, 255);
    let (step, gap, one) = (dev(4.0, s), dev(2.0, s), dev(1.0, s));
    let mut x = r.x + r.h / 2;
    while x + gap < r.x + r.w - r.h / 2 {
        canvas.fill_rect(IRect::new(x, r.y, gap, one), tokens::SURFACE, 255);
        canvas.fill_rect(
            IRect::new(x, r.y + r.h - one, gap, one),
            tokens::SURFACE,
            255,
        );
        x += step;
    }
    let th = scene.text.line_height(px);
    scene.text.draw(
        canvas,
        r.x as f32 + (r.w as f32 - tw) / 2.0,
        r.y as f32 + (r.h as f32 - th) / 2.0,
        &label,
        px,
        PREVIEW_TEXT,
    );
}

/// The small Enter keycap after Start: an outlined box with the return icon.
fn keycap(canvas: &mut Canvas<'_>, r: IRect, s: f64) {
    let radius = dev(6.0, s) as f32;
    canvas.fill_rrect(r, radius, tokens::LINE, 255);
    canvas.fill_rrect(r.grow(-dev(1.0, s)), radius - 1.0, tokens::RAISED, 255);
    let sz = dev(12.0, s);
    icons::draw(
        canvas,
        Icon::Enter,
        r.x + (r.w - sz) / 2,
        r.y + (r.h - sz) / 2,
        sz,
        tokens::TEXT_3,
    );
}

/// One button: a selected mode is `ink` on the light `text` fill, a selected segment is `text` on
/// `raised`, a primary button is light in the bar and `rec` in the card, the rest are plain (segments `text_3`); each has its icon
/// before the label, the direction segment only the icon.
fn button(canvas: &mut Canvas<'_>, scene: &Scene<'_>, e: &Entry, in_card: bool) {
    let s = scene.scale;
    let px = (crate::long_bar::LABEL_PX * s) as f32;
    let radius = dev(10.0, s) as f32;
    let segment = matches!(e.item, Item::Scroll(_) | Item::Dir(_));
    let (fill, label) = if e.primary {
        // Start in the bar is light like the selected mode (design S1); the card's primary
        // button keeps the record colour.
        (
            Some(if in_card { tokens::REC } else { tokens::TEXT }),
            tokens::INK,
        )
    } else if e.inert {
        (None, tokens::TEXT_2)
    } else if e.selected && segment {
        (Some(tokens::RAISED), tokens::TEXT)
    } else if e.selected {
        (Some(tokens::TEXT), tokens::INK)
    } else if in_card {
        (Some(tokens::RAISED), tokens::TEXT)
    } else if e.dimmed || segment {
        (None, tokens::TEXT_3)
    } else {
        (None, tokens::TEXT_2)
    };
    if let Some(c) = fill {
        canvas.fill_rrect(e.rect, radius, c, 255);
    }
    let isz = dev(crate::long_bar::ICON_PX, s);
    let icon_y = e.rect.y + (e.rect.h - isz) / 2;
    if e.icon_only {
        if let Some(i) = e.icon {
            let ix = e.rect.x + (e.rect.w - isz) / 2;
            icons::draw(canvas, i, ix, icon_y, isz, label);
        }
        return;
    }
    let lw = scene.text.measure(&e.label, px);
    let th = scene.text.line_height(px);
    let gap = dev(8.0, s) as f32;
    let icon_w = e.icon.map_or(0.0, |_| isz as f32 + gap);
    let content = icon_w + lw;
    let mut x = e.rect.x as f32 + (e.rect.w as f32 - content) / 2.0;
    let y = e.rect.y as f32 + (e.rect.h as f32 - th) / 2.0;
    if let Some(i) = e.icon {
        icons::draw(canvas, i, x.round() as i32, icon_y, isz, label);
        x += icon_w;
    }
    scene.text.draw(canvas, x, y, &e.label, px, label);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Design S24: the tag is 14 px high, ends 2 px past the button's right edge and sits above
    /// its top-right corner without covering the label row.
    #[test]
    fn the_preview_tag_sits_above_the_buttons_top_right_corner() {
        let button = IRect::new(100, 200, 120, 40);
        let r = preview_tag_rect(button, 36, 1.0);
        assert_eq!(r.h, 14);
        assert_eq!(r.x + r.w, 100 + 120 + 2);
        assert!(r.y < button.y && r.y + r.h <= button.y + 4);
        // At 2x everything doubles; the tag stays inside the same corner.
        let r2 = preview_tag_rect(IRect::new(200, 400, 240, 80), 72, 2.0);
        assert_eq!((r2.h, r2.x + r2.w), (28, 200 + 240 + 4));
    }
}
