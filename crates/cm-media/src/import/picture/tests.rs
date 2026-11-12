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
fn a_cmyk_jpeg_is_refused_by_name() {
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xC0, 0, 20, 8, 0, 8, 0, 8, 4];
    jpeg.extend_from_slice(&[1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0, 4, 0x11, 0, 0xFF, 0xD9]);
    let p = parse_jpeg_header(&jpeg).unwrap_err();
    assert_eq!(p.reason, Reason::UnsupportedFormat);
    assert_eq!(p.format.as_deref(), Some("jpeg_cmyk"));
    assert_eq!(
        p.data(),
        serde_json::json!({"reason": "unsupported_format", "format": "jpeg_cmyk"})
    );
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
