//! Annotation and GPU compositor rendering (preview and export).
//!
//! Implemented: the CPU annotation renderer (ANN-1, ANN-2) and, in [`compose`], the headless
//! `wgpu` frame compositor (background, padding, rounded screen, camera layouts; M4). The
//! compositor draws one frame from a plain [`compose::Scene`]; mapping the edit list to scenes,
//! decoding and encoding are separate steps.
//!
//! ```
//! use cm_render::{render_annotations, parse_ops_str};
//! let base = image::RgbaImage::from_pixel(64, 48, image::Rgba([240, 240, 240, 255]));
//! let ops = parse_ops_str(r##"[{"id":"op_1","op":"rect","rect":[8,8,40,24],"color":"#FF3B3B","width":4}]"##).unwrap();
//! let out = render_annotations(&base, &ops).unwrap();
//! assert_eq!(out.dimensions(), (64, 48));
//! ```
//!
//! Output is deterministic for the bundled-font [`Renderer::new`]: same input, same bytes.

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod compose;
pub mod ocr;
pub mod ops;
mod redact;
mod render;
mod text;

pub use compose::{ClickMark, ClickStyle, Compositor, CursorLayer, CursorSprite, MAX_CLICKS};
pub use ops::{
    parse_ops, parse_ops_str, validate_op, AnnotationOp, Color, OpKind, Pt, Rc,
    BLUR_STRENGTH_RANGE, PIXELATE_BLOCK_RANGE,
};
pub use render::{render_annotations, Renderer};

use cm_core::CmError;
use image::RgbaImage;

/// Encodes as a plain PNG: only IHDR, IDAT and IEND chunks, no ancillary chunks or metadata
/// (ANN-2.AC3: exports carry no extra data).
pub fn encode_png(img: &RgbaImage) -> Result<Vec<u8>, CmError> {
    use image::codecs::png::PngEncoder;
    use image::ImageEncoder;
    let mut buf = Vec::new();
    PngEncoder::new(&mut buf)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| CmError::invalid_input(format!("png encoding failed: {e}")))?;
    Ok(buf)
}
