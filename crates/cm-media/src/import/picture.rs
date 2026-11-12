//! Importing pictures (IMP-6; `specs/features/18-import.md`): PNG and JPEG only.
//!
//! Everything here is local and needs no FFmpeg. The content decides, never the extension
//! ([`sniff`]); the size limits are decided from the header alone ([`parse_png_header`],
//! [`parse_jpeg_header`]) so a decompression bomb never reaches the decoder; the pixels leave as a
//! plain 8-bit RGBA PNG with no ancillary chunk ([`encode_plain_png`], ANN-2.AC3), with the EXIF
//! orientation applied and no metadata copied.

use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::Path;

use image::{DynamicImage, ImageReader, Limits, RgbaImage};

use super::classify::{Classification, MediaKind, PrepareReason, Status};
use super::{Problem, Reason};

/// Longest side accepted, in pixels.
pub const MAX_SIDE: u32 = 16_384;
/// Most pixels accepted (50 megapixels; maintainer decision of Nov 12, 2026).
pub const MAX_PIXELS: u64 = 50_000_000;
/// How much of the file the header parsers see (a JPEG may carry large metadata before its frame).
pub const HEAD_BYTES: usize = 4 << 20;
/// Bytes of the file's end checked for the end marker (a cut-short file lacks it).
const TAIL_BYTES: u64 = 4096;

/// The two formats that can be imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
        }
    }
}

/// What the first bytes of a file say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sniff {
    Image(ImageFormat),
    /// A picture format that is recognised and refused: `gif`, `webp`, `heic`, `avif`, `tiff`,
    /// `bmp`, `svg`, `raw`.
    Refused(&'static str),
    /// Not a picture this module knows: the file goes to the media probe.
    Other,
}

/// Names a picture format from the first bytes of a file (use at least 1 KiB when the file may be
/// an SVG). Pure.
pub fn sniff(head: &[u8]) -> Sniff {
    let at = |i: usize, s: &[u8]| head.get(i..i + s.len()) == Some(s);
    if at(0, &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Sniff::Image(ImageFormat::Png);
    }
    if at(0, &[0xFF, 0xD8, 0xFF]) {
        return Sniff::Image(ImageFormat::Jpeg);
    }
    if at(0, b"GIF87a") || at(0, b"GIF89a") {
        return Sniff::Refused("gif");
    }
    if at(0, b"RIFF") && at(8, b"WEBP") {
        return Sniff::Refused("webp");
    }
    if at(4, b"ftyp") {
        let brand = head.get(8..12).unwrap_or(&[]);
        match brand {
            b"heic" | b"heix" | b"heim" | b"heis" | b"hevc" | b"hevx" | b"mif1" | b"msf1" => {
                return Sniff::Refused("heic")
            }
            b"avif" | b"avis" => return Sniff::Refused("avif"),
            b"crx " => return Sniff::Refused("raw"),
            _ => {}
        }
    }
    if at(0, b"II*\0") || at(0, b"MM\0*") || at(0, b"II+\0") || at(0, b"MM\0+") {
        // Canon CR2 is a TIFF with "CR" at byte 8.
        return Sniff::Refused(if at(8, b"CR") { "raw" } else { "tiff" });
    }
    if at(0, b"FUJIFILMCCD-RAW")
        || at(0, b"IIRO")
        || at(0, b"IIRS")
        || at(0, b"IIU\0")
        || at(0, b"FOVb")
    {
        return Sniff::Refused("raw");
    }
    if at(0, b"BM") {
        // The DIB header size that follows the 14-byte file header.
        let dib = head
            .get(14..18)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        if matches!(dib, Some(12 | 40 | 52 | 56 | 64 | 108 | 124)) {
            return Sniff::Refused("bmp");
        }
    }
    if looks_like_svg(head) {
        return Sniff::Refused("svg");
    }
    Sniff::Other
}

fn looks_like_svg(head: &[u8]) -> bool {
    let head = head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head);
    let start = head
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(head.len());
    let text = &head[start..];
    let lower: Vec<u8> = text.iter().take(2048).map(u8::to_ascii_lowercase).collect();
    let has = |needle: &[u8]| lower.windows(needle.len()).any(|w| w == needle);
    lower.starts_with(b"<svg")
        || ((lower.starts_with(b"<?xml") || lower.starts_with(b"<!doctype svg")) && has(b"<svg"))
}

/// What the header of a PNG or JPEG says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// EXIF orientation 1 to 8 (1: upright). Dimensions are those stored, before the rotation.
    pub orientation: u8,
    /// 16-bit, palette or greyscale pixels: the import converts them to 8-bit RGBA.
    pub converted: bool,
    /// An animated PNG: only its first frame is imported.
    pub animated: bool,
}

impl ImageInfo {
    /// The size after the orientation is applied.
    pub fn displayed(&self) -> (u32, u32) {
        if (5..=8).contains(&self.orientation) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

fn unreadable(message: &str) -> Problem {
    Problem::new(Reason::UnreadableFile, message)
}

fn cut_short(format: ImageFormat) -> Problem {
    unreadable(&format!(
        "The {} image can't be read. It may be damaged or cut short.",
        if format == ImageFormat::Png {
            "PNG"
        } else {
            "JPEG"
        }
    ))
}

/// The limits, from the stored size. `Ok` when the picture may be decoded.
pub fn check_limits(format: ImageFormat, width: u32, height: u32) -> Result<(), Problem> {
    if width == 0 || height == 0 {
        return Err(cut_short(format));
    }
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        let mut p = Problem::new(
            Reason::TooLarge,
            format!(
                "The image is {width}\u{d7}{height} px. CaptureMann imports pictures up to {MAX_SIDE} px on a side and {} megapixels.",
                MAX_PIXELS / 1_000_000
            ),
        );
        p.format = Some(format.as_str().to_owned());
        return Err(p);
    }
    Ok(())
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Reads a PNG's chunks up to the first `IDAT`. Pure. Does not apply the limits.
pub fn parse_png_header(head: &[u8]) -> Result<ImageInfo, Problem> {
    let bad = || cut_short(ImageFormat::Png);
    if !head.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        return Err(bad());
    }
    let mut pos = 8usize;
    let mut info: Option<ImageInfo> = None;
    loop {
        let Some(h) = head.get(pos..pos + 8) else {
            return Err(bad());
        };
        let len = be32(&h[0..4]) as usize;
        let kind = &h[4..8];
        let data_at = pos + 8;
        match kind {
            b"IHDR" => {
                let d = head.get(data_at..data_at + 13).ok_or_else(bad)?;
                let (depth, color) = (d[8], d[9]);
                info = Some(ImageInfo {
                    format: ImageFormat::Png,
                    width: be32(&d[0..4]),
                    height: be32(&d[4..8]),
                    orientation: 1,
                    // Truecolour and truecolour-with-alpha at 8 bits are used as they are.
                    converted: depth == 16 || color == 3 || color == 0 || color == 4,
                    animated: false,
                });
            }
            b"acTL" => {
                if let Some(i) = info.as_mut() {
                    i.animated = true;
                }
            }
            b"eXIf" => {
                if let (Some(i), Some(d)) = (info.as_mut(), head.get(data_at..data_at + len)) {
                    i.orientation = exif_orientation(d).unwrap_or(1);
                }
            }
            b"IDAT" => return info.ok_or_else(bad),
            b"IEND" => return Err(bad()),
            _ => {}
        }
        // length, type, data, CRC
        pos = data_at
            .checked_add(len)
            .and_then(|p| p.checked_add(4))
            .ok_or_else(bad)?;
    }
}

/// Reads a JPEG's segments up to the start of the frame. Pure. Does not apply the limits.
/// A CMYK or YCCK file is [`Problem`] `unsupported_format` with `format: "jpeg_cmyk"`.
pub fn parse_jpeg_header(head: &[u8]) -> Result<ImageInfo, Problem> {
    let bad = || cut_short(ImageFormat::Jpeg);
    if !head.starts_with(&[0xFF, 0xD8]) {
        return Err(bad());
    }
    let mut pos = 2usize;
    let mut orientation = 1u8;
    loop {
        // A marker is 0xFF, optional 0xFF fill bytes, then the marker code.
        while head.get(pos) == Some(&0xFF) && head.get(pos + 1) == Some(&0xFF) {
            pos += 1;
        }
        if head.get(pos) != Some(&0xFF) {
            return Err(bad());
        }
        let code = *head.get(pos + 1).ok_or_else(bad)?;
        pos += 2;
        match code {
            // Standalone markers: no length.
            0x01 | 0xD0..=0xD7 | 0xD8 => continue,
            // End of image or start of scan before a frame header.
            0xD9 | 0xDA => return Err(bad()),
            _ => {}
        }
        let len = head
            .get(pos..pos + 2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
            .ok_or_else(bad)?;
        if len < 2 {
            return Err(bad());
        }
        let data = head.get(pos + 2..pos + len).ok_or_else(bad)?;
        match code {
            0xE1 if data.starts_with(b"Exif\0\0") => {
                if let Some(o) = exif_orientation(&data[6..]) {
                    orientation = o;
                }
            }
            // SOF0..SOF15 except DHT (C4), JPG (C8) and DAC (CC).
            0xC0..=0xCF if !matches!(code, 0xC4 | 0xC8 | 0xCC) => {
                if data.len() < 6 {
                    return Err(bad());
                }
                let (precision, height, width, comps) = (
                    data[0],
                    u32::from(u16::from_be_bytes([data[1], data[2]])),
                    u32::from(u16::from_be_bytes([data[3], data[4]])),
                    data[5],
                );
                if comps == 4 {
                    let mut p = Problem::unsupported_format("jpeg_cmyk");
                    p.message = "CMYK JPEG files are not supported yet. Export the picture as RGB and import that.".to_owned();
                    return Err(p);
                }
                // Baseline, extended sequential and progressive Huffman, 8 bits; the decoder
                // reads nothing else (arithmetic, lossless and 12-bit files).
                if !matches!(code, 0xC0..=0xC2) || precision != 8 {
                    let mut p = Problem::unsupported_format("jpeg_variant");
                    p.message = "This kind of JPEG (arithmetic coding, lossless or more than 8 bits) is not supported yet.".to_owned();
                    return Err(p);
                }
                return Ok(ImageInfo {
                    format: ImageFormat::Jpeg,
                    width,
                    height,
                    orientation,
                    converted: comps == 1,
                    animated: false,
                });
            }
            _ => {}
        }
        pos += len;
    }
}

/// The orientation (1 to 8) of an EXIF block (a TIFF header and its first directory). Pure.
pub fn exif_orientation(tiff: &[u8]) -> Option<u8> {
    let little = match tiff.get(0..4)? {
        b"II*\0" => true,
        b"MM\0*" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b = tiff.get(i..i + 2)?;
        Some(if little {
            u16::from_le_bytes([b[0], b[1]])
        } else {
            u16::from_be_bytes([b[0], b[1]])
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b = tiff.get(i..i + 4)?;
        Some(if little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        })
    };
    let ifd = usize::try_from(u32_at(4)?).ok()?;
    let n = usize::from(u16_at(ifd)?);
    for i in 0..n {
        let e = ifd.checked_add(2 + i * 12)?;
        if u16_at(e)? == 0x0112 {
            let v = u16_at(e + 8)?;
            return (1..=8).contains(&v).then_some(v as u8);
        }
    }
    None
}

/// Reads the header of the file at `path` and applies the limits, without decoding. Also checks
/// the end of the file, so a cut-short picture is `unreadable_file` before anything is decoded.
/// Returns [`Sniff`]-style problems: a refused format, a CMYK JPEG, `too_large`, `unreadable_file`.
pub fn inspect(path: &Path) -> Result<ImageInfo, Problem> {
    let io = |e: std::io::Error| unreadable(&format!("The file can't be read: {e}."));
    let mut f = std::fs::File::open(path).map_err(io)?;
    let size = f.metadata().map_err(io)?.len();
    let mut head = Vec::new();
    (&mut f)
        .take(HEAD_BYTES as u64)
        .read_to_end(&mut head)
        .map_err(io)?;
    if head.is_empty() {
        return Err(unreadable("The file is empty."));
    }
    let format = match sniff(&head) {
        Sniff::Image(f) => f,
        Sniff::Refused(name) => return Err(Problem::unsupported_format(name)),
        Sniff::Other => {
            return Err(unreadable(
                "The file can't be read. It may be damaged, cut short or not a picture.",
            ))
        }
    };
    let info = match format {
        ImageFormat::Png => parse_png_header(&head)?,
        ImageFormat::Jpeg => parse_jpeg_header(&head)?,
    };
    check_limits(format, info.width, info.height)?;
    // The end marker: IEND for a PNG, EOI near the end for a JPEG.
    let tail_len = size.min(TAIL_BYTES);
    let mut tail = vec![0u8; tail_len as usize];
    f.seek(SeekFrom::Start(size - tail_len)).map_err(io)?;
    f.read_exact(&mut tail).map_err(io)?;
    if !ends_properly(format, &tail) {
        return Err(cut_short(format));
    }
    Ok(info)
}

/// Whether the last bytes of a file hold the end of the picture. Pure.
pub fn ends_properly(format: ImageFormat, tail: &[u8]) -> bool {
    match format {
        ImageFormat::Png => {
            tail.len() >= 12
                && tail[tail.len() - 12..]
                    == [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xAE, 0x42, 0x60, 0x82]
        }
        ImageFormat::Jpeg => tail.windows(2).any(|w| w == [0xFF, 0xD9]),
    }
}

/// The status of an inspected picture: Ready, or Prepared with what was done to the pixels.
pub fn classify_image(info: &ImageInfo) -> Classification {
    let mut why = Vec::new();
    if info.converted {
        why.push(PrepareReason::Converted);
    }
    if info.orientation != 1 {
        why.push(PrepareReason::Oriented);
    }
    if info.animated {
        why.push(PrepareReason::FirstFrameOnly);
    }
    Classification {
        kind: MediaKind::Image,
        status: if why.is_empty() {
            Status::Ready
        } else {
            Status::Prepared(why)
        },
    }
}

/// A decoded picture, ready to be stored.
pub struct Loaded {
    pub info: ImageInfo,
    /// Width and height after the orientation.
    pub width: u32,
    pub height: u32,
    /// A plain 8-bit RGBA PNG: IHDR, IDAT and IEND only.
    pub png: Vec<u8>,
}

/// Why a picture could not be loaded.
#[derive(Debug)]
pub enum LoadError {
    Problem(Problem),
    Other(cm_core::CmError),
}

impl From<Problem> for LoadError {
    fn from(p: Problem) -> Self {
        LoadError::Problem(p)
    }
}

/// Decodes `path` (checked by [`inspect`] first), applies the orientation, converts to 8-bit
/// RGBA and encodes the plain PNG. One decoded copy is held at a time besides the conversion
/// step. `cancel` is checked between the steps.
pub fn load(path: &Path, cancel: &dyn Fn() -> bool) -> Result<Option<Loaded>, LoadError> {
    let info = inspect(path)?;
    let bytes = std::fs::read(path)
        .map_err(|e| LoadError::Problem(unreadable(&format!("The file can't be read: {e}."))))?;
    if cancel() {
        return Ok(None);
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    // A 16-bit RGBA picture of the largest allowed size is 400 MB.
    limits.max_alloc = Some(1 << 30);
    let fmt = match info.format {
        ImageFormat::Png => image::ImageFormat::Png,
        ImageFormat::Jpeg => image::ImageFormat::Jpeg,
    };
    let mut reader = ImageReader::with_format(Cursor::new(bytes), fmt);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|_| LoadError::Problem(cut_short(info.format)))?;
    if decoded.width() != info.width || decoded.height() != info.height {
        return Err(LoadError::Problem(cut_short(info.format)));
    }
    if cancel() {
        return Ok(None);
    }
    let mut img: DynamicImage = decoded;
    if info.orientation != 1 {
        if let Some(o) = image::metadata::Orientation::from_exif(info.orientation) {
            img.apply_orientation(o);
        }
    }
    let rgba = img.into_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let png = encode_plain_png(&rgba).map_err(LoadError::Other)?;
    Ok(Some(Loaded {
        info,
        width,
        height,
        png,
    }))
}

/// Encodes as a plain PNG: only IHDR, IDAT and IEND chunks, no ancillary chunks or metadata
/// (ANN-2.AC3). Fast compression: an import of a large picture should not take long.
pub fn encode_plain_png(img: &RgbaImage) -> Result<Vec<u8>, cm_core::CmError> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    use image::ImageEncoder;
    let mut buf = Vec::new();
    PngEncoder::new_with_quality(&mut buf, CompressionType::Fast, FilterType::Adaptive)
        .write_image(
            img.as_raw(),
            img.width(),
            img.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|e| cm_core::CmError::internal(format!("png encoding failed: {e}")))?;
    Ok(buf)
}

/// The chunk types of a PNG, in order (a test and the regression suite check that only
/// `IHDR`, `IDAT` and `IEND` remain).
pub fn png_chunk_types(png: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut pos = 8usize;
    while let Some(h) = png.get(pos..pos + 8) {
        let len = be32(&h[0..4]) as usize;
        out.push(String::from_utf8_lossy(&h[4..8]).into_owned());
        pos = pos + 8 + len + 4;
    }
    out
}

#[cfg(test)]
mod tests;
