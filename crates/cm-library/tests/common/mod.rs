#![allow(dead_code, clippy::unwrap_used)]
use cm_core::Actor;
use cm_library::ops::{AnnotationOp, Color, OpKind, Pt, Rc};
use time::macros::datetime;
use time::OffsetDateTime;

pub const T0: OffsetDateTime = datetime!(2026-10-19 09:12:03 UTC);

pub fn t(mins: i64) -> OffsetDateTime {
    T0 + time::Duration::minutes(mins)
}

pub fn png(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, seed, 255])
    });
    let mut out = Vec::new();
    img.write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

pub fn red() -> Color {
    Color::parse("#FF3B3B").unwrap()
}

pub fn arrow() -> AnnotationOp {
    AnnotationOp::new(
        "",
        OpKind::Arrow {
            from: Pt { x: 120.0, y: 300.0 },
            to: Pt { x: 380.0, y: 210.0 },
            color: red(),
            width: 4.0,
        },
    )
}

pub fn step(n: u32) -> AnnotationOp {
    AnnotationOp::new(
        "",
        OpKind::Step {
            at: Pt {
                x: 10.0 * f64::from(n),
                y: 10.0,
            },
            n,
            size: 28.0,
            color: red(),
        },
    )
}

pub fn rect_op() -> AnnotationOp {
    AnnotationOp::new(
        "",
        OpKind::Rect {
            rect: Rc {
                x: 1.0,
                y: 2.0,
                w: 30.0,
                h: 40.0,
            },
            color: red(),
            width: 4.0,
            fill: None,
            radius: 6.0,
        },
    )
}

pub fn user() -> Actor {
    Actor::User
}
pub fn agent() -> Actor {
    Actor::agent("claude").unwrap()
}
