//! Poster (thumbnail) decoding for the library grid.

use image::RgbaImage;

use crate::recording::files::PosterError;

/// Widest decoded poster in physical px: a 240 px card at 2x is 480 px, and the library
/// writes posters 512 px wide, so those are used as they are (resizing would allocate a
/// float intermediate of 16 bytes per pixel that the allocator then keeps). Bigger source
/// images are downscaled so 60 cached thumbnails stay around 40 MB.
pub const MAX_POSTER_WIDTH: u32 = 512;

/// Decodes a JPEG/PNG poster to BGRA8 (the byte order GPUI's atlas expects), scaled to fit
/// `max_width`. Returns the image and its size.
pub fn decode_poster(bytes: &[u8], max_width: u32) -> Result<RgbaImage, PosterError> {
    let img = image::load_from_memory(bytes).map_err(|e| PosterError::Decode(e.to_string()))?;
    let img = if img.width() > max_width {
        let h = ((u64::from(img.height()) * u64::from(max_width)) / u64::from(img.width())).max(1);
        // Box filter straight from the 8-bit pixels: no float intermediate.
        img.thumbnail_exact(max_width, h as u32)
    } else {
        img
    };
    let mut rgba = img.to_rgba8();
    for px in rgba.as_chunks_mut::<4>().0 {
        px.swap(0, 2);
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn jpeg(w: u32, h: u32) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([200, 100, 50]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
            .encode_image(&img)
            .unwrap();
        out
    }

    /// LIB-1: poster decodes to BGRA and is bounded in width.
    #[test]
    fn decodes_to_bgra_and_downscales() {
        let small = decode_poster(&jpeg(100, 60), 480).unwrap();
        assert_eq!((small.width(), small.height()), (100, 60));
        let p = small.get_pixel(50, 30).0;
        // R=200 G=100 B=50 stored as B G R A (JPEG tolerance).
        assert!(p[0].abs_diff(50) < 8 && p[2].abs_diff(200) < 8, "{p:?}");
        assert_eq!(p[3], 255);
        let big = decode_poster(&jpeg(1920, 1080), 480).unwrap();
        assert_eq!((big.width(), big.height()), (480, 270));
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(decode_poster(b"nope", 480).is_err());
    }
}
