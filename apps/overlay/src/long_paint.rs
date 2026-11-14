//! Draws the Long Shot bar and its Accessibility card (design board S1) into the software canvas.
//! The layout and every decision are in [`crate::long_bar`]; this only paints. Not run on a real
//! screen by the author (no display): the tests check that the pixels land where the layout says.

use crate::canvas::{Canvas, IRect};
use crate::long_bar::{BarLayout, Entry};
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
    for e in &l.entries {
        button(canvas, scene, e, false);
    }
    for d in &l.dividers {
        canvas.fill_rect(d.0, tokens::LINE, 255);
    }
}

/// One button: the selected mode or segment is `text` on `ink`, a primary button is `rec`, the
/// rest are plain labels; a dimmed one is `text_3`.
fn button(canvas: &mut Canvas<'_>, scene: &Scene<'_>, e: &Entry, in_card: bool) {
    let s = scene.scale;
    let px = (crate::long_bar::LABEL_PX * s) as f32;
    let radius = dev(10.0, s) as f32;
    let (fill, label) = if e.primary {
        (Some(tokens::REC), tokens::INK)
    } else if e.selected {
        (Some(tokens::TEXT), tokens::INK)
    } else if in_card {
        (Some(tokens::RAISED), tokens::TEXT)
    } else if e.dimmed {
        (None, tokens::TEXT_3)
    } else {
        (None, tokens::TEXT_2)
    };
    if let Some(c) = fill {
        canvas.fill_rrect(e.rect, radius, c, 255);
    }
    let lw = scene.text.measure(&e.label, px);
    let th = scene.text.line_height(px);
    let cap = e.keycap.map(|k| {
        (
            k,
            scene.text.measure_mono(k, px * 0.92),
            dev(14.0, s) as f32,
        )
    });
    let gap = dev(8.0, s) as f32;
    let content = lw + cap.map_or(0.0, |(_, w, pad)| gap + w + pad);
    let x = e.rect.x as f32 + (e.rect.w as f32 - content) / 2.0;
    let y = e.rect.y as f32 + (e.rect.h as f32 - th) / 2.0;
    scene.text.draw(canvas, x, y, &e.label, px, label);
    if let Some((k, w, pad)) = cap {
        let cx = x + lw + gap;
        let ch = dev(22.0, s);
        let r = IRect::new(
            cx.round() as i32,
            e.rect.y + (e.rect.h - ch) / 2,
            (w + pad).round() as i32,
            ch,
        );
        canvas.fill_rrect(r, dev(6.0, s) as f32, tokens::INK, 70);
        let mh = scene.text.line_height_mono(px * 0.92);
        scene.text.draw_mono(
            canvas,
            r.x as f32 + pad / 2.0,
            r.y as f32 + (ch as f32 - mh) / 2.0,
            k,
            px * 0.92,
            label,
        );
    }
}
