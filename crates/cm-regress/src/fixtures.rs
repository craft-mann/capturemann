//! Small committed fixtures (embedded in the binary) and generators for the rest. Provenance of the
//! committed files: `tests/data/regress/README.md`.

pub const CLIP_MP4: &[u8] = include_bytes!("../../../tests/data/regress/clip-h264.mp4");
pub const CLIP_MOV: &[u8] = include_bytes!("../../../tests/data/regress/clip-h264.mov");
pub const CLIP_MKV: &[u8] = include_bytes!("../../../tests/data/regress/clip-h264.mkv");
pub const CLIP_ROTATED: &[u8] = include_bytes!("../../../tests/data/regress/clip-rotated.mp4");
pub const CLIP_VFR: &[u8] = include_bytes!("../../../tests/data/regress/clip-vfr.mp4");
pub const TONE_MP3: &[u8] = include_bytes!("../../../tests/data/regress/tone.mp3");
pub const REDACT_PNG: &[u8] = include_bytes!("../../../tests/data/regress/redact-text.png");

/// CRC-32 (IEEE), as PNG uses it.
pub fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (n, slot) in table.iter_mut().enumerate() {
        let mut c = n as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for b in data {
        c = table[((c ^ u32::from(*b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for d in data {
        a = (a + u32::from(*d)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// An 8-bit RGB PNG (stored deflate blocks, no compression), filled by `pixel(x, y)`.
pub fn make_png(w: u32, h: u32, pixel: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(((w * 3 + 1) * h) as usize);
    for y in 0..h {
        raw.push(0);
        for x in 0..w {
            raw.extend_from_slice(&pixel(x, y));
        }
    }
    let mut z = vec![0x78, 0x01];
    let mut chunks = raw.chunks(65_535).peekable();
    while let Some(c) = chunks.next() {
        z.push(u8::from(chunks.peek().is_none()));
        z.extend_from_slice(&(c.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(c.len() as u16)).to_le_bytes());
        z.extend_from_slice(c);
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// Checks the signature, every chunk's CRC and the IHDR/IEND framing; returns (width, height).
pub fn png_info(bytes: &[u8]) -> Result<(u32, u32), String> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if bytes.len() < 33 || bytes[..8] != SIG {
        return Err(format!("not a PNG ({} bytes)", bytes.len()));
    }
    let mut pos = 8;
    let mut dims = None;
    let mut saw_idat = false;
    loop {
        if pos + 12 > bytes.len() {
            return Err("the PNG is cut off before IEND".to_owned());
        }
        let len = u32::from_be_bytes([bytes[pos], bytes[pos + 1], bytes[pos + 2], bytes[pos + 3]])
            as usize;
        let end = pos + 8 + len;
        if end + 4 > bytes.len() {
            return Err("a PNG chunk runs past the end of the file".to_owned());
        }
        let kind = &bytes[pos + 4..pos + 8];
        let crc = u32::from_be_bytes([bytes[end], bytes[end + 1], bytes[end + 2], bytes[end + 3]]);
        if crc32(&bytes[pos + 4..end]) != crc {
            return Err(format!(
                "bad CRC in chunk {}",
                String::from_utf8_lossy(kind)
            ));
        }
        match kind {
            b"IHDR" => {
                let d = &bytes[pos + 8..end];
                dims = Some((
                    u32::from_be_bytes([d[0], d[1], d[2], d[3]]),
                    u32::from_be_bytes([d[4], d[5], d[6], d[7]]),
                ));
            }
            b"IDAT" => saw_idat = true,
            b"IEND" => {
                let (w, h) = dims.ok_or("no IHDR")?;
                if !saw_idat || w == 0 || h == 0 {
                    return Err("the PNG has no image data".to_owned());
                }
                return Ok((w, h));
            }
            _ => {}
        }
        pos = end + 4;
    }
}

/// A 16-bit mono PCM WAV of a sine tone.
pub fn make_wav(freq: f64, secs: f64, rate: u32, amplitude: f64) -> Vec<u8> {
    let n = (secs * f64::from(rate)) as usize;
    let mut data = Vec::with_capacity(n * 2);
    for i in 0..n {
        let s = (2.0 * std::f64::consts::PI * freq * i as f64 / f64::from(rate)).sin() * amplitude;
        data.extend_from_slice(&((s * f64::from(i16::MAX)) as i16).to_le_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// (sample rate, channels, frames) of a PCM WAV.
pub fn wav_info(b: &[u8]) -> Result<(u32, u16, usize), String> {
    if b.len() < 44 || &b[..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err("not a WAV".to_owned());
    }
    let channels = u16::from_le_bytes([b[22], b[23]]);
    let rate = u32::from_le_bytes([b[24], b[25], b[26], b[27]]);
    let bits = u16::from_le_bytes([b[34], b[35]]);
    let data_len = u32::from_le_bytes([b[40], b[41], b[42], b[43]]) as usize;
    let frame = usize::from(channels) * usize::from(bits / 8);
    if frame == 0 {
        return Err("bad WAV header".to_owned());
    }
    Ok((rate, channels, data_len / frame))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_png_validates_and_reports_its_size() {
        let png = make_png(40, 30, |x, y| [x as u8 * 6, y as u8 * 8, 128]);
        assert_eq!(png_info(&png), Ok((40, 30)));
        // A large one needs several stored blocks.
        let big = make_png(300, 300, |x, y| [(x ^ y) as u8, 0, 0]);
        assert_eq!(png_info(&big), Ok((300, 300)));
    }

    #[test]
    fn a_damaged_png_is_refused() {
        let mut png = make_png(8, 8, |_, _| [1, 2, 3]);
        let n = png.len();
        png[n / 2] ^= 0xFF;
        assert!(png_info(&png).is_err());
        assert!(png_info(&png[..20]).is_err());
        assert!(png_info(b"hello hello hello hello hello hello hello").is_err());
    }

    #[test]
    fn committed_png_fixture_is_valid() {
        assert_eq!(png_info(REDACT_PNG), Ok((1000, 260)));
    }

    #[test]
    fn crc_vector() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn wav_round_trip() {
        let w = make_wav(440.0, 0.5, 8000, 0.5);
        assert_eq!(wav_info(&w), Ok((8000, 1, 4000)));
        assert!(wav_info(b"nope").is_err());
    }

    #[test]
    fn committed_clips_are_small() {
        for (n, b) in [
            ("mp4", CLIP_MP4),
            ("mov", CLIP_MOV),
            ("mkv", CLIP_MKV),
            ("rot", CLIP_ROTATED),
            ("vfr", CLIP_VFR),
            ("mp3", TONE_MP3),
        ] {
            assert!(b.len() < 200_000, "{n} is {} bytes", b.len());
        }
    }
}
