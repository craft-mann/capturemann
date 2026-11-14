//! CMYK and YCCK JPEG pictures, converted to 8-bit sRGB (IMP-6.AC10 to AC12, 0.3.4).
//!
//! * With an embedded ICC profile the profile does the work (`moxcms`, a pure Rust colour engine).
//! * Without one, or when the profile cannot be read, the standard conversion `R = (255-C)(255-K)/255`
//!   (the same for G and B) is used. The row then says "profile not read" only when a profile was
//!   there and could not be used.
//! * Adobe writes CMYK JPEG files with **inverted** values (255 is no ink); a file with the Adobe
//!   APP14 marker is inverted back first, so a white patch is white and a black patch is black. A
//!   4-component file without the marker is taken as stored.
//! * YCCK (Adobe transform 2) is turned into CMYK the way libjpeg does it, then handled as above.
//!
//! The JPEG decoder (`zune-jpeg`) only unpacks the four samples per pixel (`ColorSpace::CMYK`); the
//! colour work is done here, in strips of rows so the conversion never holds a second full copy.

use image::RgbaImage;
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;
use zune_jpeg::JpegDecoder;

/// What the embedded colour profile of a CMYK JPEG allows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileState {
    /// The file has no ICC profile: the standard conversion is used.
    Absent,
    /// A CMYK profile the colour engine can use.
    Usable,
    /// There is a profile, but it is damaged, is not a CMYK profile or cannot be used: the standard
    /// conversion is used and the row says "profile not read".
    Unreadable,
}

/// How a 4-component JPEG stores its colours.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CmykInfo {
    /// The Adobe APP14 marker is there: the stored values are inverted.
    pub adobe: bool,
    /// Adobe transform 2: the first three components are Y, Cb and Cr.
    pub ycck: bool,
    pub profile: ProfileState,
}

/// The parts of the JPEG segments the conversion needs.
#[derive(Debug, Default)]
pub struct Meta {
    /// The colour transform byte of the Adobe APP14 marker (0 CMYK, 2 YCCK), and where it sits.
    pub adobe_transform: Option<(usize, u8)>,
    /// The embedded ICC profile, its APP2 chunks joined in order.
    pub icc: Option<Vec<u8>>,
}

/// Walks the segments up to the start of the scan. Pure. Never fails: what is not understood is not
/// reported (the decoder is the one that rejects a damaged file).
pub fn scan(bytes: &[u8]) -> Meta {
    let mut meta = Meta::default();
    let mut chunks: Vec<(u8, Vec<u8>)> = Vec::new();
    let mut pos = 2usize;
    while pos + 4 <= bytes.len() {
        if bytes[pos] != 0xFF {
            break;
        }
        let code = bytes[pos + 1];
        if code == 0xFF {
            pos += 1;
            continue;
        }
        pos += 2;
        if matches!(code, 0x01 | 0xD0..=0xD8) {
            continue;
        }
        if code == 0xDA || code == 0xD9 {
            break;
        }
        let Some(len) = bytes
            .get(pos..pos + 2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
        else {
            break;
        };
        if len < 2 {
            break;
        }
        let Some(data) = bytes.get(pos + 2..pos + len) else {
            break;
        };
        match code {
            // "Adobe", version, two flags words, then the transform byte.
            0xEE if data.len() >= 12 && data.starts_with(b"Adobe") => {
                meta.adobe_transform = Some((pos + 2 + 11, data[11]));
            }
            0xE2 if data.len() > 14 && data.starts_with(b"ICC_PROFILE\0") => {
                chunks.push((data[12], data[14..].to_vec()));
            }
            _ => {}
        }
        pos += len;
    }
    if !chunks.is_empty() {
        chunks.sort_by_key(|c| c.0);
        meta.icc = Some(chunks.into_iter().flat_map(|c| c.1).collect());
    }
    meta
}

/// What the colour engine makes of a profile.
fn engine(icc: &[u8]) -> Option<std::sync::Arc<moxcms::Transform8BitExecutor>> {
    let src = ColorProfile::new_from_slice(icc).ok()?;
    if src.color_space != DataColorSpace::Cmyk {
        return None;
    }
    src.create_transform_8bit(
        // Four samples per pixel in, as the engine takes CMYK.
        Layout::Rgba,
        &ColorProfile::new_srgb(),
        Layout::Rgb,
        TransformOptions::default(),
    )
    .ok()
}

/// The colour facts of a 4-component file, from the bytes of its header. Pure apart from reading
/// the profile.
pub fn info_from(bytes: &[u8]) -> CmykInfo {
    let meta = scan(bytes);
    let transform = meta.adobe_transform.map(|t| t.1);
    CmykInfo {
        adobe: transform.is_some(),
        ycck: transform == Some(2),
        profile: match meta.icc.as_deref() {
            None => ProfileState::Absent,
            Some(icc) if engine(icc).is_some() => ProfileState::Usable,
            Some(_) => ProfileState::Unreadable,
        },
    }
}

/// `(255 - c) * (255 - k) / 255`, rounded: the standard conversion of one channel, on ink values.
fn naive(c: u8, k: u8) -> u8 {
    let t = u32::from(255 - c) * u32::from(255 - k) + 127;
    ((t + (t >> 8)) >> 8) as u8
}

/// libjpeg's YCCK to CMYK on one pixel: the YCC part gives R, G, B and C, M, Y are their inverse;
/// K passes through.
fn ycck_pixel(px: &mut [u8; 4]) {
    let (y, cb, cr) = (
        f32::from(px[0]),
        f32::from(px[1]) - 128.0,
        f32::from(px[2]) - 128.0,
    );
    let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
    let r = clamp(y + 1.402 * cr);
    let g = clamp(y - 0.344_136 * cb - 0.714_136 * cr);
    let b = clamp(y + 1.772 * cb);
    px[0] = 255 - r;
    px[1] = 255 - g;
    px[2] = 255 - b;
}

/// Rows converted at a time.
const STRIP_ROWS: usize = 128;

/// Decodes a CMYK or YCCK JPEG into 8-bit RGBA (alpha 255). `bytes` is the whole file.
pub fn decode_to_rgba(bytes: &[u8], info: &CmykInfo) -> Result<RgbaImage, String> {
    let meta = scan(bytes);
    // A YCCK file is unpacked as plain four-component data (transform byte 0) and turned into CMYK
    // here: the decoder only knows how to turn it into RGB.
    let patched;
    let data: &[u8] = match meta.adobe_transform {
        Some((at, 2)) => {
            let mut v = bytes.to_vec();
            v[at] = 0;
            patched = v;
            &patched
        }
        _ => bytes,
    };
    let options = DecoderOptions::default().jpeg_set_out_colorspace(ColorSpace::CMYK);
    let mut decoder = JpegDecoder::new_with_options(ZCursor::new(data), options);
    decoder.decode_headers().map_err(|e| e.to_string())?;
    let (w, h) = decoder.dimensions().ok_or_else(|| "no size".to_owned())?;
    let mut samples = decoder.decode().map_err(|e| e.to_string())?;
    let row = w * 4;
    if samples.len() != row * h {
        return Err("the decoder returned a different number of samples".to_owned());
    }
    let transform = match (info.profile, meta.icc.as_deref()) {
        (ProfileState::Usable, Some(icc)) => engine(icc),
        _ => None,
    };
    let mut rgba = vec![255u8; w * h * 4];
    let mut rgb = vec![0u8; STRIP_ROWS.min(h) * w * 3];
    for (strip, out) in samples
        .chunks_mut(STRIP_ROWS * row)
        .zip(rgba.chunks_mut(STRIP_ROWS * w * 4))
    {
        if info.ycck {
            strip.as_chunks_mut::<4>().0.iter_mut().for_each(ycck_pixel);
        }
        // Back to ink values: 0 is no ink.
        if info.adobe {
            strip.iter_mut().for_each(|v| *v = 255 - *v);
        }
        let px = strip.len() / 4;
        match &transform {
            Some(t) => {
                t.transform(strip, &mut rgb[..px * 3])
                    .map_err(|e| e.to_string())?;
                let (out_px, _) = out.as_chunks_mut::<4>();
                let (rgb_px, _) = rgb.as_chunks::<3>();
                for (o, c) in out_px.iter_mut().zip(rgb_px) {
                    o[..3].copy_from_slice(c);
                }
            }
            None => {
                let (out_px, _) = out.as_chunks_mut::<4>();
                let (cmyk_px, _) = strip.as_chunks::<4>();
                for (o, c) in out_px.iter_mut().zip(cmyk_px) {
                    o[0] = naive(c[0], c[3]);
                    o[1] = naive(c[1], c[3]);
                    o[2] = naive(c[2], c[3]);
                }
            }
        }
    }
    drop(samples);
    RgbaImage::from_raw(w as u32, h as u32, rgba).ok_or_else(|| "size mismatch".to_owned())
}
