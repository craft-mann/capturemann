//! Pure tests of the picture import (IMP-6.AC2 to AC6): headers, limits, orientation, colour
//! types, damaged files. No FFmpeg, no library.
#![allow(clippy::unwrap_used)]

use super::*;
use image::{GrayImage, Rgba, RgbaImage};

// ---- builders -----------------------------------------------------------------------------

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = (data.len() as u32).to_be_bytes().to_vec();
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    v.extend_from_slice(&body);
    v.extend_from_slice(&crc32(&body).to_be_bytes());
    v
}

/// A zlib stream of stored (uncompressed) blocks.
fn zlib_stored(raw: &[u8]) -> Vec<u8> {
    let mut out = vec![0x78, 0x01];
    if raw.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    }
    let mut chunks = raw.chunks(65535).peekable();
    while let Some(c) = chunks.next() {
        out.push(u8::from(chunks.peek().is_none()));
        out.extend_from_slice(&(c.len() as u16).to_le_bytes());
        out.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        out.extend_from_slice(c);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in raw {
        a = (a + u32::from(x)) % 65521;
        b = (b + a) % 65521;
    }
    out.extend_from_slice(&((b << 16) | a).to_be_bytes());
    out
}

const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

fn ihdr(w: u32, h: u32, depth: u8, color: u8) -> Vec<u8> {
    let mut d = w.to_be_bytes().to_vec();
    d.extend_from_slice(&h.to_be_bytes());
    d.extend_from_slice(&[depth, color, 0, 0, 0]);
    chunk(b"IHDR", &d)
}

/// A PNG with a header and an end but no real pixel data: enough for the header checks.
fn png_header_only(w: u32, h: u32, depth: u8, color: u8, extra: &[Vec<u8>]) -> Vec<u8> {
    let mut v = SIG.to_vec();
    v.extend(ihdr(w, h, depth, color));
    for c in extra {
        v.extend_from_slice(c);
    }
    v.extend(chunk(b"IDAT", &zlib_stored(&[])));
    v.extend(chunk(b"IEND", &[]));
    v
}

/// A real 2x2 palette PNG (the second colour is see-through through tRNS).
fn palette_png() -> Vec<u8> {
    let mut v = SIG.to_vec();
    v.extend(ihdr(2, 2, 8, 3));
    v.extend(chunk(
        b"PLTE",
        &[255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 0],
    ));
    v.extend(chunk(b"tRNS", &[255, 0, 255, 255]));
    // Two rows, filter byte 0 each: indexes 0,1 / 2,3.
    v.extend(chunk(b"IDAT", &zlib_stored(&[0, 0, 1, 0, 2, 3])));
    v.extend(chunk(b"IEND", &[]));
    v
}

fn encode(img: &image::DynamicImage, fmt: image::ImageFormat) -> Vec<u8> {
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), fmt).unwrap();
    out
}

/// Inserts an EXIF APP1 block with one orientation entry right after SOI.
fn with_orientation(jpeg: &[u8], orientation: u16, little: bool) -> Vec<u8> {
    let mut tiff = Vec::new();
    type Writers = (fn(u16) -> [u8; 2], fn(u32) -> [u8; 4]);
    let (u16b, u32b): Writers = if little {
        tiff.extend_from_slice(b"II*\0");
        (u16::to_le_bytes, u32::to_le_bytes)
    } else {
        tiff.extend_from_slice(b"MM\0*");
        (u16::to_be_bytes, u32::to_be_bytes)
    };
    tiff.extend_from_slice(&u32b(8));
    tiff.extend_from_slice(&u16b(1));
    tiff.extend_from_slice(&u16b(0x0112));
    tiff.extend_from_slice(&u16b(3));
    tiff.extend_from_slice(&u32b(1));
    tiff.extend_from_slice(&u16b(orientation));
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&u32b(0));
    let mut seg = b"Exif\0\0".to_vec();
    seg.extend(tiff);
    let mut out = vec![0xFF, 0xD8, 0xFF, 0xE1];
    out.extend_from_slice(&((seg.len() + 2) as u16).to_be_bytes());
    out.extend(seg);
    out.extend_from_slice(&jpeg[2..]);
    out
}

fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let p = dir.path().join(name);
    std::fs::write(&p, bytes).unwrap();
    p
}

fn rgba_png(w: u32, h: u32) -> Vec<u8> {
    let mut img = RgbaImage::new(w, h);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = Rgba([x as u8, y as u8, 7, 255]);
    }
    encode(
        &image::DynamicImage::ImageRgba8(img),
        image::ImageFormat::Png,
    )
}

fn rgb_jpeg(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(w, h, |x, y| image::Rgb([x as u8 * 4, y as u8 * 4, 9]));
    encode(
        &image::DynamicImage::ImageRgb8(img),
        image::ImageFormat::Jpeg,
    )
}

fn never() -> bool {
    false
}

fn loaded(path: &std::path::Path) -> Loaded {
    load(path, &never).ok().flatten().unwrap()
}

// ---- sniffing (IMP-6.AC4) -----------------------------------------------------------------

#[test]
fn the_first_bytes_decide_the_format() {
    assert_eq!(sniff(&rgba_png(2, 2)), Sniff::Image(ImageFormat::Png));
    assert_eq!(
        sniff(&[0xFF, 0xD8, 0xFF, 0xE0]),
        Sniff::Image(ImageFormat::Jpeg)
    );
    let cases: [(&[u8], &str); 8] = [
        (b"GIF89a\x01\0\x01\0", "gif"),
        (b"RIFF\x10\0\0\0WEBPVP8 ", "webp"),
        (b"\0\0\0\x18ftypheic\0\0\0\0", "heic"),
        (b"\0\0\0\x18ftypmif1\0\0\0\0", "heic"),
        (b"\0\0\0\x18ftypavif\0\0\0\0", "avif"),
        (b"II*\0\x08\0\0\0\x00\x00", "tiff"),
        (b"II*\0\x10\0\0\0CR\x02\0", "raw"),
        (
            b"\xEF\xBB\xBF  <?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            "svg",
        ),
    ];
    for (head, name) in cases {
        assert_eq!(sniff(head), Sniff::Refused(name), "{name}");
    }
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&[0; 12]);
    bmp.extend_from_slice(&40u32.to_le_bytes());
    assert_eq!(sniff(&bmp), Sniff::Refused("bmp"));
    // An MP4 with an ordinary brand, text that is not SVG and nothing at all go to the probe.
    assert_eq!(sniff(b"\0\0\0\x18ftypisom\0\0\0\0"), Sniff::Other);
    assert_eq!(sniff(b"hello <svg is a word"), Sniff::Other);
    assert_eq!(sniff(b""), Sniff::Other);
}

#[test]
fn a_png_that_is_a_jpeg_imports_as_a_jpeg_and_a_gif_named_png_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let as_png = write(&dir, "b.png", &rgb_jpeg(8, 6));
    let info = inspect(&as_png).unwrap();
    assert_eq!(
        (info.format, info.width, info.height),
        (ImageFormat::Jpeg, 8, 6)
    );
    let l = loaded(&as_png);
    assert_eq!((l.width, l.height), (8, 6));
    // A refused format is named by `Problem::unsupported_format` in the daemon (inspect itself
    // reports it from the first bytes too).
    let gif = write(
        &dir,
        "a.png",
        b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xFF\xFF\xFF;",
    );
    let p = inspect(&gif).unwrap_err();
    assert_eq!(
        (p.reason, p.format.as_deref()),
        (Reason::UnsupportedFormat, Some("gif"))
    );
    assert_eq!(p.message, "GIF files are not supported yet.");
}

// ---- limits from the header (IMP-6.AC3) ---------------------------------------------------

#[test]
fn limits_are_decided_from_the_header_alone() {
    assert!(check_limits(ImageFormat::Png, 16_384, 3_000).is_ok());
    assert!(check_limits(ImageFormat::Png, 7_071, 7_071).is_ok());
    for (w, h) in [(16_385, 10), (10, 16_385), (7_072, 7_072), (40_000, 40_000)] {
        let p = check_limits(ImageFormat::Png, w, h).unwrap_err();
        assert_eq!(p.reason, Reason::TooLarge, "{w}x{h}");
        assert_eq!(p.data()["reason"], "too_large");
    }
    // A bomb: a header that claims 60,000 x 60,000 and has no pixel data. The decoder is never
    // reached, so nothing is allocated and the answer is `too_large`, not `unreadable_file`.
    let dir = tempfile::tempdir().unwrap();
    let bomb = write(
        &dir,
        "bomb.png",
        &png_header_only(60_000, 60_000, 8, 6, &[]),
    );
    assert_eq!(inspect(&bomb).unwrap_err().reason, Reason::TooLarge);
    let wide = write(&dir, "wide.png", &png_header_only(16_385, 1, 8, 6, &[]));
    assert_eq!(inspect(&wide).unwrap_err().reason, Reason::TooLarge);
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 17, 8];
    jpeg.extend_from_slice(&20_000u16.to_be_bytes());
    jpeg.extend_from_slice(&20_000u16.to_be_bytes());
    jpeg.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1, 0xFF, 0xD9]);
    let big = write(&dir, "big.jpg", &jpeg);
    assert_eq!(inspect(&big).unwrap_err().reason, Reason::TooLarge);
}

// ---- JPEG headers: orientation, CMYK ------------------------------------------------------

#[test]
fn exif_orientation_is_read_in_both_byte_orders_and_ignored_when_invalid() {
    let jpg = rgb_jpeg(12, 8);
    for little in [true, false] {
        for o in 1..=8u16 {
            let info = parse_jpeg_header(&with_orientation(&jpg, o, little)).unwrap();
            assert_eq!(u16::from(info.orientation), o);
            // Stored size is 12x8; the displayed size swaps for 5 to 8.
            assert_eq!(info.displayed(), if o >= 5 { (8, 12) } else { (12, 8) });
        }
    }
    let odd = parse_jpeg_header(&with_orientation(&jpg, 9, true)).unwrap();
    assert_eq!(odd.orientation, 1);
    assert_eq!(parse_jpeg_header(&jpg).unwrap().orientation, 1);
    assert_eq!(exif_orientation(b"junk"), None);
    assert_eq!(exif_orientation(b"II*\0\xFF\xFF\xFF\xFF"), None);
}

#[test]
fn a_jpeg_without_a_frame_is_unreadable() {
    let cases: [&[u8]; 4] = [
        &[0xFF, 0xD8],
        &[0xFF, 0xD8, 0xFF, 0xD9],
        &[0xFF, 0xD8, 0xFF, 0xE1, 0xFF, 0xFF],
        b"\xFF\xD8\xFF",
    ];
    for bad in cases {
        assert_eq!(
            parse_jpeg_header(bad).unwrap_err().reason,
            Reason::UnreadableFile
        );
    }
}

// ---- orientation is applied, nothing else is kept (IMP-6.AC2) -----------------------------

/// A 3x2 PNG whose pixel (x, y) holds the value 10 * y + x, with an eXIf chunk.
fn grid_png_with_exif(o: u16) -> Vec<u8> {
    let mut img = RgbaImage::new(3, 2);
    for (x, y, p) in img.enumerate_pixels_mut() {
        *p = Rgba([(10 * y + x) as u8, 0, 0, 255]);
    }
    let png = encode(
        &image::DynamicImage::ImageRgba8(img),
        image::ImageFormat::Png,
    );
    let mut tiff = b"MM\0*".to_vec();
    tiff.extend_from_slice(&8u32.to_be_bytes());
    tiff.extend_from_slice(&1u16.to_be_bytes());
    tiff.extend_from_slice(&0x0112u16.to_be_bytes());
    tiff.extend_from_slice(&3u16.to_be_bytes());
    tiff.extend_from_slice(&1u32.to_be_bytes());
    tiff.extend_from_slice(&o.to_be_bytes());
    tiff.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let idat = png.windows(4).position(|w| w == b"IDAT").unwrap() - 4;
    let mut out = png[..idat].to_vec();
    out.extend(chunk(b"eXIf", &tiff));
    out.extend_from_slice(&png[idat..]);
    out
}

#[test]
fn every_orientation_lands_upright_and_leaves_a_plain_png() {
    // Where the pixel at (x, y) of the stored 3x2 picture ends up, per EXIF orientation.
    let place = |o: u8, x: u32, y: u32| -> (u32, u32) {
        let (w, h) = (3, 2);
        match o {
            1 => (x, y),
            2 => (w - 1 - x, y),
            3 => (w - 1 - x, h - 1 - y),
            4 => (x, h - 1 - y),
            5 => (y, x),
            6 => (h - 1 - y, x),
            7 => (h - 1 - y, w - 1 - x),
            _ => (y, w - 1 - x),
        }
    };
    let dir = tempfile::tempdir().unwrap();
    for o in 1..=8u8 {
        let path = write(
            &dir,
            &format!("o{o}.png"),
            &grid_png_with_exif(u16::from(o)),
        );
        let l = loaded(&path);
        assert_eq!(l.info.orientation, o);
        let (dw, dh) = if o >= 5 { (2, 3) } else { (3, 2) };
        assert_eq!((l.width, l.height), (dw, dh), "size for {o}");
        let out = image::load_from_memory(&l.png).unwrap().to_rgba8();
        for y in 0..2 {
            for x in 0..3 {
                let (nx, ny) = place(o, x, y);
                assert_eq!(
                    out.get_pixel(nx, ny).0[0],
                    (10 * y + x) as u8,
                    "o={o} ({x},{y})"
                );
            }
        }
        // No EXIF or other ancillary chunk (checked by parsing the chunks).
        let kinds = png_chunk_types(&l.png);
        assert!(
            kinds
                .iter()
                .all(|k| matches!(k.as_str(), "IHDR" | "IDAT" | "IEND")),
            "{kinds:?}"
        );
        assert_eq!(kinds.first().map(String::as_str), Some("IHDR"));
        assert_eq!(kinds.last().map(String::as_str), Some("IEND"));
    }
}

#[test]
fn a_jpeg_with_orientation_6_arrives_rotated_and_without_exif() {
    let dir = tempfile::tempdir().unwrap();
    let img = image::RgbImage::from_pixel(16, 8, image::Rgb([200, 30, 30]));
    let jpg = encode(
        &image::DynamicImage::ImageRgb8(img),
        image::ImageFormat::Jpeg,
    );
    let path = write(&dir, "phone.jpg", &with_orientation(&jpg, 6, true));
    let info = inspect(&path).unwrap();
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::Oriented])
    );
    let l = loaded(&path);
    assert_eq!((l.width, l.height), (8, 16));
    for tag in [&b"eXIf"[..], b"Exif", b"iCCP", b"tEXt"] {
        assert!(!l.png.windows(4).any(|w| w == tag));
    }
    let out = image::load_from_memory(&l.png).unwrap();
    assert_eq!((out.width(), out.height()), (8, 16));
}

// ---- colour types (IMP-6.AC5) -------------------------------------------------------------

#[test]
fn sixteen_bit_palette_and_grey_pictures_become_8_bit_rgba() {
    let dir = tempfile::tempdir().unwrap();
    // 16-bit RGBA with a half see-through pixel.
    let mut wide = image::ImageBuffer::<Rgba<u16>, Vec<u16>>::new(2, 1);
    wide.put_pixel(0, 0, Rgba([65535, 0, 0, 32768]));
    wide.put_pixel(1, 0, Rgba([0, 65535, 0, 65535]));
    let p16 = write(
        &dir,
        "wide.png",
        &encode(
            &image::DynamicImage::ImageRgba16(wide),
            image::ImageFormat::Png,
        ),
    );
    let info = inspect(&p16).unwrap();
    assert!(info.converted);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::Converted])
    );
    let out = image::load_from_memory(&loaded(&p16).png)
        .unwrap()
        .to_rgba8();
    assert_eq!(out.get_pixel(0, 0).0[0], 255);
    assert!(
        (120..=136).contains(&out.get_pixel(0, 0).0[3]),
        "alpha kept"
    );
    assert_eq!(out.get_pixel(1, 0).0, [0, 255, 0, 255]);

    // A palette PNG with tRNS.
    let pal = write(&dir, "pal.png", &palette_png());
    assert!(inspect(&pal).unwrap().converted);
    let out = image::load_from_memory(&loaded(&pal).png)
        .unwrap()
        .to_rgba8();
    assert_eq!(out.get_pixel(0, 0).0, [255, 0, 0, 255]);
    assert_eq!(out.get_pixel(1, 0).0[3], 0, "palette transparency kept");
    assert_eq!(out.get_pixel(1, 1).0, [255, 255, 0, 255]);

    // A greyscale PNG and a greyscale JPEG.
    let grey = GrayImage::from_pixel(4, 4, image::Luma([90]));
    let g = write(
        &dir,
        "g.png",
        &encode(
            &image::DynamicImage::ImageLuma8(grey.clone()),
            image::ImageFormat::Png,
        ),
    );
    assert!(inspect(&g).unwrap().converted);
    let gj = write(
        &dir,
        "g.jpg",
        &encode(
            &image::DynamicImage::ImageLuma8(grey),
            image::ImageFormat::Jpeg,
        ),
    );
    assert!(inspect(&gj).unwrap().converted);
    let l = loaded(&gj);
    assert_eq!(
        image::load_from_memory(&l.png).unwrap().color(),
        image::ColorType::Rgba8
    );
}

#[test]
fn a_plain_rgba_png_is_ready_and_keeps_its_transparency() {
    let dir = tempfile::tempdir().unwrap();
    let mut img = RgbaImage::from_pixel(4, 4, Rgba([10, 20, 30, 255]));
    img.put_pixel(2, 2, Rgba([1, 2, 3, 0]));
    let p = write(
        &dir,
        "t.png",
        &encode(
            &image::DynamicImage::ImageRgba8(img),
            image::ImageFormat::Png,
        ),
    );
    assert_eq!(classify_image(&inspect(&p).unwrap()).status, Status::Ready);
    let out = image::load_from_memory(&loaded(&p).png).unwrap().to_rgba8();
    assert_eq!(out.get_pixel(2, 2).0[3], 0);
    assert_eq!(out.get_pixel(0, 0).0, [10, 20, 30, 255]);
}

#[test]
fn an_animated_png_is_marked_first_frame_only() {
    let png = png_header_only(2, 2, 8, 6, &[chunk(b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0])]);
    let info = parse_png_header(&png).unwrap();
    assert!(info.animated);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::FirstFrameOnly])
    );
}

// ---- damaged files (IMP-6.AC6) ------------------------------------------------------------

#[test]
fn truncated_empty_and_garbage_files_are_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let png = rgba_png(64, 64);
    let cut = write(&dir, "cut.png", &png[..png.len() / 2]);
    assert_eq!(inspect(&cut).unwrap_err().reason, Reason::UnreadableFile);
    let jpg = rgb_jpeg(64, 64);
    let cutj = write(&dir, "cut.jpg", &jpg[..jpg.len() / 2]);
    assert_eq!(inspect(&cutj).unwrap_err().reason, Reason::UnreadableFile);
    let empty = write(&dir, "empty.png", b"");
    let p = inspect(&empty).unwrap_err();
    assert_eq!(
        (p.reason, p.message.as_str()),
        (Reason::UnreadableFile, "The file is empty.")
    );
    let junk = write(&dir, "junk.png", b"this is not a picture at all");
    assert_eq!(inspect(&junk).unwrap_err().reason, Reason::UnreadableFile);
    // A good header around data that is not pixels: the decoder says so.
    let mut bad = png_header_only(4, 4, 8, 6, &[]);
    let at = bad.windows(4).position(|w| w == b"IDAT").unwrap() + 4;
    bad[at] ^= 0xFF;
    let badp = write(&dir, "bad.png", &bad);
    assert!(matches!(
        load(&badp, &never),
        Err(LoadError::Problem(p)) if p.reason == Reason::UnreadableFile
    ));
}

#[test]
fn cancel_stops_before_decoding() {
    let dir = tempfile::tempdir().unwrap();
    let p = write(&dir, "a.png", &rgba_png(8, 8));
    assert!(matches!(load(&p, &|| true), Ok(None)));
}

#[test]
fn png_chunks_are_listed_in_order() {
    let png = png_header_only(1, 1, 8, 6, &[chunk(b"tEXt", b"k\0v")]);
    assert_eq!(png_chunk_types(&png), ["IHDR", "tEXt", "IDAT", "IEND"]);
}

// ---- CMYK and YCCK JPEG (IMP-6.AC10 to AC12, 0.3.4) --------------------------------------------

/// Bits written MSB first with the 0xFF stuffing of a JPEG scan.
#[derive(Default)]
struct Bits {
    out: Vec<u8>,
    acc: u32,
    n: u32,
}

impl Bits {
    fn put(&mut self, v: u32, len: u32) {
        for i in (0..len).rev() {
            self.acc = (self.acc << 1) | ((v >> i) & 1);
            self.n += 1;
            if self.n == 8 {
                let b = self.acc as u8;
                self.out.push(b);
                if b == 0xFF {
                    self.out.push(0);
                }
                self.acc = 0;
                self.n = 0;
            }
        }
    }
    fn finish(mut self) -> Vec<u8> {
        while self.n != 0 {
            self.put(1, 1);
        }
        self.out
    }
}

fn segment(code: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![0xFF, code];
    v.extend_from_slice(&((body.len() + 2) as u16).to_be_bytes());
    v.extend_from_slice(body);
    v
}

/// A four-component baseline JPEG, `blocks.len()` flat 8x8 blocks side by side: each block is one
/// DC value per component (no other coefficient), so the decoder returns exactly those values.
/// `adobe` is the APP14 transform byte, `icc` an embedded profile.
fn four_component_jpeg(blocks: &[[u8; 4]], adobe: Option<u8>, icc: Option<&[u8]>) -> Vec<u8> {
    let mut j = vec![0xFF, 0xD8];
    if let Some(t) = adobe {
        let mut b = b"Adobe".to_vec();
        b.extend_from_slice(&[0, 100, 0, 0, 0, 0, t]);
        j.extend(segment(0xEE, &b));
    }
    if let Some(icc) = icc {
        let mut b = b"ICC_PROFILE\0".to_vec();
        b.extend_from_slice(&[1, 1]);
        b.extend_from_slice(icc);
        j.extend(segment(0xE2, &b));
    }
    let mut dqt = vec![0u8];
    dqt.extend_from_slice(&[8u8; 64]);
    j.extend(segment(0xDB, &dqt));
    let mut sof = vec![8u8, 0, 8];
    sof.extend_from_slice(&((blocks.len() * 8) as u16).to_be_bytes());
    sof.push(4);
    for i in 0..4u8 {
        sof.extend_from_slice(&[i + 1, 0x11, 0]);
    }
    j.extend(segment(0xC0, &sof));
    // DC table: nine codes of four bits (categories 0 to 8); AC table: only end-of-block, code "0".
    let mut dc = vec![0x00u8];
    let mut counts = [0u8; 16];
    counts[3] = 9;
    dc.extend_from_slice(&counts);
    dc.extend(0..9u8);
    j.extend(segment(0xC4, &dc));
    let mut ac = vec![0x10u8];
    let mut counts = [0u8; 16];
    counts[0] = 1;
    ac.extend_from_slice(&counts);
    ac.push(0);
    j.extend(segment(0xC4, &ac));
    let mut sos = vec![4u8];
    for i in 0..4u8 {
        sos.extend_from_slice(&[i + 1, 0x00]);
    }
    sos.extend_from_slice(&[0, 63, 0]);
    j.extend(segment(0xDA, &sos));
    let mut bits = Bits::default();
    let mut pred = [0i32; 4];
    for block in blocks {
        for (c, v) in block.iter().enumerate() {
            let level = i32::from(*v) - 128;
            let diff = level - pred[c];
            pred[c] = level;
            let cat = if diff == 0 {
                0
            } else {
                32 - diff.unsigned_abs().leading_zeros()
            };
            bits.put(cat, 4);
            if cat > 0 {
                let v = if diff > 0 {
                    diff
                } else {
                    diff + (1 << cat) - 1
                };
                bits.put(v as u32, cat);
            }
            bits.put(0, 1);
        }
    }
    j.extend(bits.finish());
    j.extend_from_slice(&[0xFF, 0xD9]);
    j
}

/// Six reference patches as ink values (0 is no ink): white, black, cyan, magenta, yellow, red.
const INK: [[u8; 4]; 6] = [
    [0, 0, 0, 0],
    [0, 0, 0, 255],
    [255, 0, 0, 0],
    [0, 255, 0, 0],
    [0, 0, 255, 0],
    [0, 255, 255, 0],
];

/// The sRGB values those patches must come out as.
const SRGB: [[u8; 3]; 6] = [
    [255, 255, 255],
    [0, 0, 0],
    [0, 255, 255],
    [255, 0, 255],
    [255, 255, 0],
    [255, 0, 0],
];

fn inverted(ink: &[[u8; 4]]) -> Vec<[u8; 4]> {
    ink.iter().map(|p| p.map(|v| 255 - v)).collect()
}

/// The mean colour of each 8x8 patch of a decoded PNG.
fn patch_means(png: &[u8]) -> Vec<[f32; 3]> {
    let img = image::load_from_memory(png).unwrap().to_rgba8();
    (0..img.width() / 8)
        .map(|k| {
            let mut sum = [0f32; 3];
            for y in 0..8 {
                for x in 0..8 {
                    let p = img.get_pixel(k * 8 + x, y);
                    for c in 0..3 {
                        sum[c] += f32::from(p[c]);
                    }
                }
            }
            sum.map(|v| v / 64.0)
        })
        .collect()
}

fn assert_patches(png: &[u8], want: &[[u8; 3]], tolerance: f32) {
    let got = patch_means(png);
    assert_eq!(got.len(), want.len());
    for (i, (g, w)) in got.iter().zip(want).enumerate() {
        for c in 0..3 {
            assert!(
                (g[c] - f32::from(w[c])).abs() <= tolerance,
                "patch {i} channel {c}: got {} want {}",
                g[c],
                w[c]
            );
        }
    }
}

/// sRGB (0 or 1 per channel, so no transfer curve) to CIE Lab under D50.
fn lab_of_srgb_corner(rgb: [f32; 3]) -> [f32; 3] {
    let m = [
        [0.436_074_7, 0.385_064_9, 0.143_080_4],
        [0.222_504_5, 0.716_878_6, 0.060_616_9],
        [0.013_932_2, 0.097_104_5, 0.714_173_3],
    ];
    let white = [0.9642f32, 1.0, 0.8249];
    let xyz: Vec<f32> = (0..3)
        .map(|r| (m[r][0] * rgb[0] + m[r][1] * rgb[1] + m[r][2] * rgb[2]) / white[r])
        .collect();
    let f = |t: f32| {
        if t > 0.008_856 {
            t.cbrt()
        } else {
            7.787 * t + 16.0 / 116.0
        }
    };
    let (fx, fy, fz) = (f(xyz[0]), f(xyz[1]), f(xyz[2]));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// A small CMYK ICC profile (version 2.2, a 2-point `mft2` look-up table from CMYK to Lab) that
/// reproduces the subtractive model on its corners: R = (1-C)(1-K) and so on. `black_l` replaces the
/// lightness of the all-black corner (K = 1), to prove the profile, not the standard formula, is used.
fn cmyk_icc(black_l: Option<f32>) -> Vec<u8> {
    let be32 = |v: u32| v.to_be_bytes().to_vec();
    let s15 = |v: f32| be32((v * 65536.0).round() as i32 as u32);
    let mut clut = Vec::new();
    for c in 0..2 {
        for m in 0..2 {
            for y in 0..2 {
                for k in 0..2 {
                    let rgb = [
                        ((1 - c) * (1 - k)) as f32,
                        ((1 - m) * (1 - k)) as f32,
                        ((1 - y) * (1 - k)) as f32,
                    ];
                    let mut lab = lab_of_srgb_corner(rgb);
                    if let (Some(l), true) = (black_l, (c, m, y, k) == (0, 0, 0, 1)) {
                        lab = [l, 0.0, 0.0];
                    }
                    // The legacy 16-bit Lab encoding of version 2 profiles.
                    for v in [
                        lab[0] * 65280.0 / 100.0,
                        (lab[1] + 128.0) * 256.0,
                        (lab[2] + 128.0) * 256.0,
                    ] {
                        clut.extend((v.round().clamp(0.0, 65535.0) as u16).to_be_bytes());
                    }
                }
            }
        }
    }
    let mut lut = b"mft2".to_vec();
    lut.extend_from_slice(&[0, 0, 0, 0, 4, 3, 2, 0]);
    for v in [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
        lut.extend(s15(v));
    }
    // Input and output tables of two entries each: the identity.
    lut.extend_from_slice(&[0, 2, 0, 2]);
    for _ in 0..4 {
        lut.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
    }
    lut.extend(clut);
    for _ in 0..3 {
        lut.extend_from_slice(&[0, 0, 0xFF, 0xFF]);
    }
    let mut wtpt = b"XYZ \0\0\0\0".to_vec();
    for v in [0.9642, 1.0, 0.8249] {
        wtpt.extend(s15(v));
    }
    let desc_text = b"cm test cmyk\0";
    let mut desc = b"desc\0\0\0\0".to_vec();
    desc.extend(be32(desc_text.len() as u32));
    desc.extend_from_slice(desc_text);
    desc.extend_from_slice(&[0; 4 + 4 + 2 + 1 + 67]);
    let tags: [(&[u8; 4], Vec<u8>); 3] = [(b"desc", desc), (b"wtpt", wtpt), (b"A2B0", lut)];
    let table_end = 128 + 4 + 12 * tags.len();
    let mut body = Vec::new();
    let mut table = be32(tags.len() as u32);
    for (sig, data) in &tags {
        table.extend_from_slice(*sig);
        table.extend(be32((table_end + body.len()) as u32));
        table.extend(be32(data.len() as u32));
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    let mut head = vec![0u8; 128];
    head[8..12].copy_from_slice(&0x0220_0000u32.to_be_bytes());
    head[12..16].copy_from_slice(b"prtr");
    head[16..20].copy_from_slice(b"CMYK");
    head[20..24].copy_from_slice(b"Lab ");
    head[24..26].copy_from_slice(&2026u16.to_be_bytes());
    head[26..28].copy_from_slice(&11u16.to_be_bytes());
    head[28..30].copy_from_slice(&14u16.to_be_bytes());
    head[36..40].copy_from_slice(b"acsp");
    head[68..72].copy_from_slice(&0x0000_F6D6u32.to_be_bytes());
    head[72..76].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    head[76..80].copy_from_slice(&0x0000_D32Du32.to_be_bytes());
    let mut icc = head;
    icc.extend(table);
    icc.extend(body);
    let total = icc.len() as u32;
    icc[0..4].copy_from_slice(&total.to_be_bytes());
    icc
}

fn import_cmyk(name: &str, jpeg: &[u8]) -> (ImageInfo, Loaded) {
    let dir = tempfile::tempdir().unwrap();
    let path = write(&dir, name, jpeg);
    let info = inspect(&path).unwrap();
    let l = loaded(&path);
    (info, l)
}

#[test]
fn a_cmyk_jpeg_is_described_not_refused() {
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), None);
    let info = parse_jpeg_header(&jpg).unwrap();
    assert_eq!((info.width, info.height), (48, 8));
    let c = info.cmyk.unwrap();
    assert!(c.adobe && !c.ycck);
    assert_eq!(c.profile, ProfileState::Absent);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::Converted])
    );
}

/// IMP-6.AC11: an Adobe (inverted) CMYK file without a profile keeps its colours: the white patch
/// is white and the black patch is black.
#[test]
fn imp6_ac11_an_adobe_cmyk_jpeg_without_a_profile_is_not_inverted() {
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), None);
    let (info, l) = import_cmyk("adobe.jpg", &jpg);
    assert_eq!((l.width, l.height), (48, 8));
    assert_patches(&l.png, &SRGB, 2.0);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::Converted])
    );
}

/// A 4-component file without the Adobe marker is taken as stored (0 is no ink).
#[test]
fn a_cmyk_jpeg_without_the_adobe_marker_is_taken_as_stored() {
    let jpg = four_component_jpeg(&INK, None, None);
    let (_, l) = import_cmyk("plain.jpg", &jpg);
    assert_patches(&l.png, &SRGB, 2.0);
}

/// IMP-6.AC10: with an embedded profile the six patches come out within 4 units per channel of the
/// reference sRGB values, at the size of the file, as a plain 8-bit PNG.
#[test]
fn imp6_ac10_a_cmyk_jpeg_with_a_profile_converts_to_srgb() {
    let icc = cmyk_icc(None);
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), Some(&icc));
    let (info, l) = import_cmyk("profile.jpg", &jpg);
    assert_eq!(info.cmyk.unwrap().profile, ProfileState::Usable);
    assert_eq!((l.width, l.height), (48, 8));
    assert_patches(&l.png, &SRGB, 4.0);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![PrepareReason::Converted])
    );
}

/// The profile, not the standard formula, decides when it is usable: a profile that maps full black
/// to mid grey gives a mid grey patch.
#[test]
fn a_usable_profile_overrides_the_standard_conversion() {
    let icc = cmyk_icc(Some(50.0));
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), Some(&icc));
    let (_, l) = import_cmyk("grey.jpg", &jpg);
    let black = patch_means(&l.png)[1];
    for v in black {
        assert!((v - 119.0).abs() <= 4.0, "{black:?}");
    }
}

/// IMP-6.AC12: a profile that cannot be read gives the standard conversion and the row says so; the
/// import never ends in `jpeg_cmyk`.
#[test]
fn imp6_ac12_an_unreadable_profile_falls_back_and_says_so() {
    let jpg = four_component_jpeg(
        &inverted(&INK),
        Some(0),
        Some(b"this is not a profile at all"),
    );
    let (info, l) = import_cmyk("badicc.jpg", &jpg);
    assert_eq!(info.cmyk.unwrap().profile, ProfileState::Unreadable);
    assert_patches(&l.png, &SRGB, 2.0);
    assert_eq!(
        classify_image(&info).status,
        Status::Prepared(vec![
            PrepareReason::Converted,
            PrepareReason::ProfileNotRead
        ])
    );
}

/// A profile for another colour model (here: the header says RGB) is "not read" as well.
#[test]
fn an_rgb_profile_in_a_cmyk_file_is_not_used() {
    let mut icc = cmyk_icc(None);
    icc[16..20].copy_from_slice(b"RGB ");
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), Some(&icc));
    let (info, l) = import_cmyk("rgbicc.jpg", &jpg);
    assert_eq!(info.cmyk.unwrap().profile, ProfileState::Unreadable);
    assert_patches(&l.png, &SRGB, 2.0);
}

/// YCCK (Adobe transform 2): the first three components are Y, Cb and Cr of the inverted colours.
#[test]
fn a_ycck_jpeg_converts_like_a_cmyk_one() {
    let ycck: Vec<[u8; 4]> = INK
        .iter()
        .map(|ink| {
            let [r, g, b] = [f32::from(ink[0]), f32::from(ink[1]), f32::from(ink[2])];
            let clamp = |v: f32| v.round().clamp(0.0, 255.0) as u8;
            [
                clamp(0.299 * r + 0.587 * g + 0.114 * b),
                clamp(128.0 - 0.168_736 * r - 0.331_264 * g + 0.5 * b),
                clamp(128.0 + 0.5 * r - 0.418_688 * g - 0.081_312 * b),
                255 - ink[3],
            ]
        })
        .collect();
    let jpg = four_component_jpeg(&ycck, Some(2), None);
    let info = parse_jpeg_header(&jpg).unwrap();
    assert!(info.cmyk.unwrap().ycck);
    let (_, l) = import_cmyk("ycck.jpg", &jpg);
    assert_patches(&l.png, &SRGB, 5.0);
    // And with a profile (YCCK is turned into CMYK first).
    let icc = cmyk_icc(None);
    let jpg = four_component_jpeg(&ycck, Some(2), Some(&icc));
    let (info, l) = import_cmyk("ycck_icc.jpg", &jpg);
    assert_eq!(info.cmyk.unwrap().profile, ProfileState::Usable);
    assert_patches(&l.png, &SRGB, 6.0);
}

/// The EXIF orientation applies to a converted CMYK file too, and the PNG stays plain.
#[test]
fn a_cmyk_jpeg_with_orientation_arrives_upright() {
    let jpg = four_component_jpeg(&inverted(&INK), Some(0), None);
    let rotated = with_orientation(&jpg, 6, true);
    let (info, l) = import_cmyk("rot.jpg", &rotated);
    assert_eq!(info.orientation, 6);
    assert_eq!((l.width, l.height), (8, 48));
    assert_eq!(
        png_chunk_types(&l.png).first().map(String::as_str),
        Some("IHDR")
    );
}

/// Manual check against a real CMYK profile (never run by default): writes the fixture JPEG and
/// our PNG into `CM_CMYK_OUT` so a colour-managed tool (macOS `sips`) can be compared by hand.
/// `CM_CMYK_ICC=<profile.icc> CM_CMYK_OUT=<dir> cargo test -p cm-media --lib real_profile -- --ignored`.
#[test]
#[ignore]
fn real_profile_writes_files_for_comparison() {
    let (Ok(icc), Ok(out)) = (std::env::var("CM_CMYK_ICC"), std::env::var("CM_CMYK_OUT")) else {
        println!("SKIPPED: set CM_CMYK_ICC and CM_CMYK_OUT");
        return;
    };
    let icc = std::fs::read(icc).unwrap();
    let mut ink = INK.to_vec();
    ink.extend([[128, 64, 32, 16], [40, 200, 90, 60], [255, 255, 255, 255]]);
    let jpg = four_component_jpeg(&inverted(&ink), Some(0), Some(&icc));
    let dir = std::path::Path::new(&out);
    std::fs::write(dir.join("real.jpg"), &jpg).unwrap();
    let (info, l) = import_cmyk("real.jpg", &jpg);
    println!("profile: {:?}", info.cmyk.unwrap().profile);
    std::fs::write(dir.join("ours.png"), &l.png).unwrap();
    for (i, m) in patch_means(&l.png).iter().enumerate() {
        println!("patch {i}: {m:?}");
    }
}
