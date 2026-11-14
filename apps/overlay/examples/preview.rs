//! Renders the picker chrome over a PNG to a file, without a window, for design review.
//!
//! `cargo run -p cm-overlay --example preview -- in.png out.png [scale] [x0 y0 x1 y1]`
//! The pointer ends at (x1, y1) with a drag from (x0, y0); without coordinates only the
//! crosshair, loupe and hint are drawn. With `CM_PREVIEW_LONG=available|needs_accessibility|
//! unavailable` the picker is in Long Shot mode and the bar is drawn; `CM_PREVIEW_CARD=1` opens
//! the Accessibility card.

use cm_core::PhysicalSize;
use cm_overlay::canvas::Canvas;
use cm_overlay::long_bar::{self, AutoSupport, BarState, Item, Scroll};
use cm_overlay::long_paint::LongScene;
use cm_overlay::paint::{paint, Frame, Scene};
use cm_overlay::picker::{Mode, Picker};
use cm_overlay::text::TextPainter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (input, output) = (a.first().ok_or("input.png")?, a.get(1).ok_or("output.png")?);
    let scale: f64 = a.get(2).map_or(Ok(2.0), |s| s.parse())?;
    let img = image::open(input)?.to_rgba8();
    let (w, h) = (img.width(), img.height());
    let frame = Frame::from_rgba(w, h, img.as_raw()).ok_or("bad frame")?;
    let long_env = std::env::var("CM_PREVIEW_LONG").ok();
    let mut picker = Picker::new(
        if long_env.is_some() {
            Mode::Long
        } else {
            Mode::Region
        },
        PhysicalSize {
            width: w,
            height: h,
        },
        scale,
        vec![],
        false,
    );
    let n: Vec<f64> = a.iter().skip(3).filter_map(|s| s.parse().ok()).collect();
    if let [x0, y0, x1, y1] = n[..] {
        picker.pointer_moved(x0, y0);
        picker.press();
        picker.pointer_moved(x1, y1);
        picker.release();
        picker.pointer_moved(x1, y1);
    } else {
        picker.pointer_moved(f64::from(w) * 0.4, f64::from(h) * 0.4);
    }
    let text = TextPainter::new().ok_or("font")?;
    let mut buf = vec![0u32; (w * h) as usize];
    let mut canvas = Canvas::new(&mut buf, w, h).ok_or("canvas")?;
    let long_state = long_env.as_deref().and_then(AutoSupport::parse).map(|a| {
        let mut st = BarState::new(a);
        if std::env::var_os("CM_PREVIEW_CARD").is_some() {
            st.activate(Item::Scroll(Scroll::Auto));
        }
        st
    });
    let layout = long_state
        .map(|st| long_bar::layout(&st, w as i32, h as i32, scale, &|s, px| text.measure(s, px)));
    let scene = Scene {
        long: layout.as_ref().map(|layout| LongScene { layout }),
        picker: &picker,
        scale,
        text: &text,
        show_loupe: true,
        show_hint: true,
        tab_to_list: false,
    };
    if !paint(&mut canvas, &frame, &scene) {
        return Err("size mismatch".into());
    }
    let mut out = image::RgbaImage::new(w, h);
    for (i, px) in out.pixels_mut().enumerate() {
        let c = buf[i];
        *px = image::Rgba([(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
    }
    out.save(output)?;
    Ok(())
}
