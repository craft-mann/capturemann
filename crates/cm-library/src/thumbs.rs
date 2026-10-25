//! Poster/thumbnail generation for screenshots (`thumbs/poster.jpg`).

use crate::bundle::ShotBundle;
use crate::util::{io_err, write_atomic};
use cm_core::CmError;
use image::imageops::FilterType;
use std::path::PathBuf;

/// File name of the screenshot poster inside `thumbs/`.
pub const POSTER_FILE: &str = "poster.jpg";
/// Default longest edge of the poster in physical px (2x of a 256 px grid card).
pub const DEFAULT_POSTER_EDGE: u32 = 512;

/// Writes `thumbs/poster.jpg`, scaled to fit `max_edge`, from `render.png` when it is current
/// (so annotations show in the grid) and from `original.png` when the bundle has no ops.
/// Never upscales.
///
/// When the bundle has ops but no current render, the original must not be used: it would show
/// whatever a blur or pixelate is meant to hide (R1-12). An existing poster is then kept, unless
/// the ops include a redaction (or ops this build cannot read), in which case the old poster may
/// show the unredacted pixels and is removed. Both cases return an error, so the caller can
/// report that the thumbnail waits for the render.
pub fn write_poster(bundle: &ShotBundle, max_edge: u32) -> Result<PathBuf, CmError> {
    if max_edge == 0 {
        return Err(CmError::invalid_field("max_edge", "must be positive"));
    }
    let src = if bundle.render_is_stale() {
        let m = bundle.manifest();
        if !m.ops.is_empty() || !m.unknown_ops.is_empty() {
            let poster = bundle.thumbs_dir().join(POSTER_FILE);
            let redacts = !m.unknown_ops.is_empty() || m.ops.iter().any(|o| o.kind.is_redaction());
            if redacts {
                let _ = std::fs::remove_file(&poster);
            }
            return Err(CmError::invalid_input(
                "the annotated render is not ready; the thumbnail is not updated",
            ));
        }
        bundle.original_path()
    } else {
        bundle.render_path()
    };
    let img = image::open(&src)
        .map_err(|e| CmError::invalid_input(format!("cannot decode `{}`: {e}", src.display())))?;
    let (w, h) = (img.width(), img.height());
    let img = if w.max(h) > max_edge {
        let scale = f64::from(max_edge) / f64::from(w.max(h));
        let nw = ((f64::from(w) * scale).round() as u32).max(1);
        let nh = ((f64::from(h) * scale).round() as u32).max(1);
        img.resize_exact(nw, nh, FilterType::Triangle)
    } else {
        img
    };
    let rgb = img.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85)
        .encode_image(&rgb)
        .map_err(|e| CmError::internal(format!("cannot encode poster: {e}")))?;
    let dir = bundle.thumbs_dir();
    std::fs::create_dir_all(&dir).map_err(|e| io_err("cannot create", &dir, &e))?;
    let path = dir.join(POSTER_FILE);
    write_atomic(&path, &out)?;
    Ok(path)
}
