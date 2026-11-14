//! Draws the Long Shot bar and its Accessibility card (design board S1) into the software canvas.
//! The layout and every decision are in [`crate::long_bar`]; this only paints. Not run on a real
//! screen by the author (no display): the tests check that the pixels land where the layout says.

use crate::canvas::{Canvas, IRect};
use crate::icons::{self, Icon};
use crate::long_bar::{BarLayout, Entry, Item};
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
