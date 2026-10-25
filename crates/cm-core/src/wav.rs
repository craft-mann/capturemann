//! Crash-safe 32-bit float WAV writer with RF64 for very long recordings.
//!
//! The 80-byte header is written up front with valid sizes for the data written so far, and the
//! size fields are patched in place about once per second of audio (and on [`WavWriter::finalize`]),
//! followed by an `fdatasync` so a power loss loses about one second, not the file (R3-12). A
//! process crash therefore leaves a file whose header is at most ~1 s stale and that any reader
//! (hound, FFmpeg) opens. A file truncated in an unclean shutdown (power loss, disk full) can have a
//! header larger or smaller than the bytes on disk; [`repair_wav`] fixes it from the file length.
//!
//! # Length (R3-1, REC-2.AC5: no length limit)
//!
//! Plain RIFF/WAVE holds 4 GiB of data, about 3.1 h of 48 kHz stereo float. The header starts as
//! `RIFF` + a 28-byte `JUNK` chunk. When the data would pass the RIFF limit the writer rewrites
//! the same 80 bytes in place as `RF64` with the JUNK chunk turned into the `ds64` chunk (64-bit
//! riff, data and sample counts), so one file keeps growing; FFmpeg reads it. (`hound` does not
//! read RF64, nothing in the app does.) Old files with the 44-byte header are still read and
//! repaired.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// Header length: RIFF header (12) + `JUNK`/`ds64` chunk (36) + `fmt ` chunk (24) + `data` chunk
/// header (8).
pub const WAV_HEADER_LEN: u64 = 80;
const FORMAT_IEEE_FLOAT: u16 = 3;
const BYTES_PER_SAMPLE: u16 = 4;
/// Patch the header every this many frames' worth of bytes (1 s at 48 kHz).
const PATCH_EVERY_FRAMES: u64 = 48_000;
const STAGE_BYTES: usize = 64 * 1024;
/// Largest data chunk of a plain RIFF file (with a margin for the header and rounding).
const RIFF_MAX_DATA: u64 = u32::MAX as u64 - 1024;

/// What a WAV header says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WavInfo {
    pub channels: u16,
    pub sample_rate: u32,
    pub data_bytes: u64,
}

impl WavInfo {
    pub fn frames(&self) -> u64 {
        self.data_bytes / (u64::from(self.channels.max(1)) * u64::from(BYTES_PER_SAMPLE))
    }
}

/// Streaming writer for interleaved f32 samples.
pub struct WavWriter {
    file: Option<File>,
    path: PathBuf,
    stage: Vec<u8>,
    channels: u16,
    sample_rate: u32,
    data_bytes: u64,
    patched_bytes: u64,
    patch_every_bytes: u64,
    rf64_after: u64,
    fault_after: Option<u64>,
}

fn fmt_chunk(h: &mut [u8], at: usize, channels: u16, sample_rate: u32) {
    let block_align = channels * BYTES_PER_SAMPLE;
    h[at..at + 4].copy_from_slice(b"fmt ");
    h[at + 4..at + 8].copy_from_slice(&16u32.to_le_bytes());
    h[at + 8..at + 10].copy_from_slice(&FORMAT_IEEE_FLOAT.to_le_bytes());
    h[at + 10..at + 12].copy_from_slice(&channels.to_le_bytes());
    h[at + 12..at + 16].copy_from_slice(&sample_rate.to_le_bytes());
    h[at + 16..at + 20].copy_from_slice(&(sample_rate * u32::from(block_align)).to_le_bytes());
    h[at + 20..at + 22].copy_from_slice(&block_align.to_le_bytes());
    h[at + 22..at + 24].copy_from_slice(&(BYTES_PER_SAMPLE * 8).to_le_bytes());
}

fn header(
    channels: u16,
    sample_rate: u32,
    data_bytes: u64,
    rf64_after: u64,
) -> [u8; WAV_HEADER_LEN as usize] {
    let mut h = [0u8; WAV_HEADER_LEN as usize];
    h[8..12].copy_from_slice(b"WAVE");
    fmt_chunk(&mut h, 48, channels, sample_rate);
    h[72..76].copy_from_slice(b"data");
    if data_bytes > rf64_after {
        h[0..4].copy_from_slice(b"RF64");
        h[4..8].copy_from_slice(&u32::MAX.to_le_bytes());
        h[12..16].copy_from_slice(b"ds64");
        h[16..20].copy_from_slice(&28u32.to_le_bytes());
        h[20..28].copy_from_slice(&(WAV_HEADER_LEN - 8 + data_bytes).to_le_bytes());
        h[28..36].copy_from_slice(&data_bytes.to_le_bytes());
        let frames = data_bytes / (u64::from(channels.max(1)) * u64::from(BYTES_PER_SAMPLE));
        h[36..44].copy_from_slice(&frames.to_le_bytes());
        h[72..76].copy_from_slice(b"data");
        h[76..80].copy_from_slice(&u32::MAX.to_le_bytes());
    } else {
        let data = u32::try_from(data_bytes).unwrap_or(u32::MAX);
        h[0..4].copy_from_slice(b"RIFF");
        h[4..8].copy_from_slice(&(72 + data).to_le_bytes());
        h[12..16].copy_from_slice(b"JUNK");
        h[16..20].copy_from_slice(&28u32.to_le_bytes());
        h[76..80].copy_from_slice(&data.to_le_bytes());
    }
    h
}

impl WavWriter {
    /// Creates (truncating) `path` and writes the header for an empty data chunk.
    pub fn create(path: &Path, channels: u16, sample_rate: u32) -> io::Result<Self> {
        if channels == 0 || sample_rate == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "channels and sample rate must be non-zero",
            ));
        }
        let mut file = File::create(path)?;
        file.write_all(&header(channels, sample_rate, 0, RIFF_MAX_DATA))?;
        Ok(WavWriter {
            file: Some(file),
            path: path.to_owned(),
            stage: Vec::with_capacity(STAGE_BYTES),
            channels,
            sample_rate,
            data_bytes: 0,
            patched_bytes: 0,
            patch_every_bytes: PATCH_EVERY_FRAMES
                * u64::from(channels)
                * u64::from(BYTES_PER_SAMPLE)
                * u64::from(sample_rate.max(1))
                / 48_000,
            rf64_after: RIFF_MAX_DATA,
            fault_after: None,
        })
    }

    /// Switches to RF64 once the data passes `bytes` (tests use a small value; the default is the
    /// RIFF limit). Call before writing.
    #[must_use]
    pub fn with_rf64_after(mut self, bytes: u64) -> Self {
        self.rf64_after = bytes.min(RIFF_MAX_DATA);
        self
    }

    /// Test hook: from `bytes` of data on, `write` fails with a disk-full error (what a full disk
    /// does), leaving what was written intact.
    #[doc(hidden)]
    #[must_use]
    pub fn with_fault_after(mut self, bytes: u64) -> Self {
        self.fault_after = Some(bytes);
        self
    }

    pub fn channels(&self) -> u16 {
        self.channels
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Frames written so far (including staged, not yet flushed ones).
    pub fn frames(&self) -> u64 {
        self.data_bytes / (u64::from(self.channels) * u64::from(BYTES_PER_SAMPLE))
    }

    /// Appends interleaved samples. The slice length should be a multiple of the channel count; a
    /// trailing partial frame is ignored. There is no length limit (RF64 past 4 GiB).
    pub fn write(&mut self, samples: &[f32]) -> io::Result<()> {
        let ch = usize::from(self.channels);
        let samples = &samples[..samples.len() - samples.len() % ch];
        let add = samples.len() as u64 * u64::from(BYTES_PER_SAMPLE);
        if self.fault_after.is_some_and(|f| self.data_bytes + add > f) {
            return Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "injected fault: no space left on device",
            ));
        }
        for s in samples {
            self.stage.extend_from_slice(&s.to_le_bytes());
        }
        self.data_bytes += add;
        if self.stage.len() >= STAGE_BYTES {
            self.flush_stage()?;
        }
        if self.data_bytes - self.patched_bytes >= self.patch_every_bytes {
            self.patch()?;
        }
        Ok(())
    }

    fn flush_stage(&mut self) -> io::Result<()> {
        if let Some(f) = self.file.as_mut() {
            f.write_all(&self.stage)?;
        }
        self.stage.clear();
        Ok(())
    }

    /// Flushes staged samples, rewrites the size fields and syncs the data to disk.
    pub fn patch(&mut self) -> io::Result<()> {
        self.flush_stage()?;
        let h = header(
            self.channels,
            self.sample_rate,
            self.data_bytes,
            self.rf64_after,
        );
        if let Some(f) = self.file.as_mut() {
            f.seek(SeekFrom::Start(0))?;
            f.write_all(&h)?;
            f.seek(SeekFrom::End(0))?;
            f.sync_data()?;
        }
        self.patched_bytes = self.data_bytes;
        Ok(())
    }

    /// Writes the final header, syncs the file and returns the number of frames. If the disk
    /// refuses (full), the file is repaired from its length instead and what is on disk is kept
    /// (R3-1); the error is still returned.
    pub fn finalize(mut self) -> io::Result<u64> {
        let frames = self.frames();
        let res = self.patch().and_then(|()| match self.file.take() {
            Some(f) => f.sync_all(),
            None => Ok(()),
        });
        match res {
            Ok(()) => Ok(frames),
            Err(e) => {
                self.file = None;
                let _ = repair_wav(&self.path);
                Err(e)
            }
        }
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        if self.file.is_some() {
            // Best effort on an early drop (error paths); never panics.
            if self.patch().is_err() {
                self.file = None;
                let _ = repair_wav(&self.path);
            }
        }
    }
}

/// Reads the header of a float WAV written by [`WavWriter`] (80-byte header, RIFF or RF64) or an
/// older 44-byte-header file.
pub fn read_info(path: &Path) -> io::Result<WavInfo> {
    let f = File::open(path)?;
    let mut h = Vec::with_capacity(128);
    f.take(128).read_to_end(&mut h)?;
    parse_header(&h).map(|(info, _)| info)
}

fn bad(msg: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.to_owned())
}

fn le32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn le64(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8).map(|s| {
        let mut a = [0u8; 8];
        a.copy_from_slice(s);
        u64::from_le_bytes(a)
    })
}

/// Walks the chunks up to `data`: the info and the length of the header (where the samples start).
fn parse_header(h: &[u8]) -> io::Result<(WavInfo, u64)> {
    let rf64 = match h.get(0..4) {
        Some(b"RIFF") => false,
        Some(b"RF64") => true,
        _ => return Err(bad("not a RIFF/RF64 WAV")),
    };
    if h.get(8..12) != Some(b"WAVE") {
        return Err(bad("not a WAVE file"));
    }
    let mut at = 12usize;
    let mut fmt: Option<(u16, u32)> = None;
    let mut ds64_data: Option<u64> = None;
    loop {
        let id = h.get(at..at + 4).ok_or_else(|| bad("no data chunk"))?;
        let size = le32(h, at + 4).ok_or_else(|| bad("truncated header"))?;
        match id {
            b"fmt " => {
                let ch = h
                    .get(at + 10..at + 12)
                    .map(|s| u16::from_le_bytes([s[0], s[1]]))
                    .ok_or_else(|| bad("truncated fmt"))?;
                let rate = le32(h, at + 12).ok_or_else(|| bad("truncated fmt"))?;
                fmt = Some((ch, rate));
            }
            b"ds64" if rf64 => {
                ds64_data = le64(h, at + 16);
            }
            b"data" => {
                let (channels, sample_rate) = fmt.ok_or_else(|| bad("data before fmt"))?;
                let data_bytes = match (rf64, ds64_data) {
                    (true, Some(d)) => d,
                    _ => u64::from(size),
                };
                return Ok((
                    WavInfo {
                        channels,
                        sample_rate,
                        data_bytes,
                    },
                    (at + 8) as u64,
                ));
            }
            _ => {}
        }
        at += 8 + size as usize + (size as usize & 1);
        if at > h.len() {
            return Err(bad("no data chunk"));
        }
    }
}

/// Repairs the size fields of a WAV left by a crash or truncation: the data chunk becomes
/// everything after the header, rounded down to whole frames (RF64 if it is longer than a RIFF
/// file can say). Returns the repaired info.
pub fn repair_wav(path: &Path) -> io::Result<WavInfo> {
    let mut f = OpenOptions::new().read(true).write(true).open(path)?;
    let len = f.metadata()?.len();
    let mut h = Vec::with_capacity(128);
    (&mut f).take(128).read_to_end(&mut h)?;
    let (info, header_len) = parse_header(&h)?;
    let frame = u64::from(info.channels.max(1)) * u64::from(BYTES_PER_SAMPLE);
    let body = len.saturating_sub(header_len);
    let data_bytes = body / frame * frame;
    f.seek(SeekFrom::Start(0))?;
    if header_len != WAV_HEADER_LEN {
        // A file from before RF64 support (44 bytes, or a header with extra chunks from another
        // writer): patch the two 32-bit sizes in place.
        let data = u32::try_from(data_bytes.min(RIFF_MAX_DATA)).unwrap_or(u32::MAX);
        let data_bytes = u64::from(data) / frame * frame;
        let data = data_bytes as u32;
        f.seek(SeekFrom::Start(4))?;
        f.write_all(&((header_len - 8) as u32 + data).to_le_bytes())?;
        f.seek(SeekFrom::Start(header_len - 4))?;
        f.write_all(&data.to_le_bytes())?;
        f.set_len(header_len + data_bytes)?;
        f.sync_all()?;
        return Ok(WavInfo { data_bytes, ..info });
    }
    f.write_all(&header(
        info.channels,
        info.sample_rate,
        data_bytes,
        RIFF_MAX_DATA,
    ))?;
    f.set_len(header_len + data_bytes)?;
    f.sync_all()?;
    Ok(WavInfo { data_bytes, ..info })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn samples(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i % 1000) as f32 / 1000.0 - 0.5).collect()
    }

    #[test]
    fn round_trips_through_hound() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        let mut w = WavWriter::create(&p, 2, 48_000).unwrap();
        let s = samples(2 * 4800);
        w.write(&s).unwrap();
        assert_eq!(w.frames(), 4800);
        assert_eq!(w.finalize().unwrap(), 4800);
        let mut r = hound::WavReader::open(&p).unwrap();
        let spec = r.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (2, 48_000, 32)
        );
        assert_eq!(spec.sample_format, hound::SampleFormat::Float);
        let back: Vec<f32> = r.samples::<f32>().map(|s| s.unwrap()).collect();
        assert_eq!(back, s);
    }

    #[test]
    fn a_crashed_writer_leaves_a_readable_file_at_most_a_second_stale() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("crash.wav");
        let mut w = WavWriter::create(&p, 1, 48_000).unwrap();
        // 2.5 s of audio, then "crash": never finalize and never drop (the OS keeps what was written).
        w.write(&samples(120_000)).unwrap();
        let copy = dir.path().join("copy.wav");
        std::fs::copy(&p, &copy).unwrap();
        std::mem::forget(w);
        let r = hound::WavReader::open(&copy).unwrap();
        // Header was patched after the second boundary: at least 2 s are declared and readable.
        let declared = r.duration();
        assert!((96_000..=120_000).contains(&declared), "{declared}");
        let n = hound::WavReader::open(&copy)
            .unwrap()
            .samples::<f32>()
            .filter(|s| s.is_ok())
            .count();
        assert_eq!(n as u32, declared);
    }

    #[test]
    fn repair_fixes_a_truncated_file_and_a_stale_header() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.wav");
        let mut w = WavWriter::create(&p, 2, 48_000).unwrap();
        w.write(&samples(2 * 10_000)).unwrap();
        w.finalize().unwrap();
        // Cut in the middle of a frame.
        let f = OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(WAV_HEADER_LEN + 2 * 4 * 6_000 + 3).unwrap();
        drop(f);
        // Header still claims 10_000 frames; hound would hit an unexpected EOF.
        assert!(hound::WavReader::open(&p)
            .unwrap()
            .samples::<f32>()
            .any(|s| s.is_err()));
        let info = repair_wav(&p).unwrap();
        assert_eq!(info.frames(), 6_000);
        let mut r = hound::WavReader::open(&p).unwrap();
        assert_eq!(r.duration(), 6_000);
        assert!(r.samples::<f32>().all(|s| s.is_ok()));
        assert_eq!(read_info(&p).unwrap(), info);
    }

    #[test]
    fn rejects_bad_params_and_garbage() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WavWriter::create(&dir.path().join("x.wav"), 0, 48_000).is_err());
        let p = dir.path().join("junk.wav");
        std::fs::write(&p, vec![1u8; 100]).unwrap();
        assert!(read_info(&p).is_err());
        assert!(repair_wav(&p).is_err());
    }

    /// R3-1 / REC-2.AC5: past the RIFF limit one file keeps growing as RF64 (a small injected
    /// limit stands in for 4 GiB).
    #[test]
    fn r3_1_the_file_continues_as_rf64_past_the_riff_limit() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("long.wav");
        let mut w = WavWriter::create(&p, 2, 48_000)
            .unwrap()
            .with_rf64_after(40_000);
        let s = samples(2 * 1000);
        for _ in 0..30 {
            w.write(&s).unwrap();
        }
        assert_eq!(w.finalize().unwrap(), 30_000);
        let bytes = std::fs::read(&p).unwrap();
        assert_eq!(&bytes[0..4], b"RF64");
        assert_eq!(&bytes[12..16], b"ds64");
        assert_eq!(bytes.len() as u64, WAV_HEADER_LEN + 30_000 * 8);
        let info = read_info(&p).unwrap();
        assert_eq!(
            (info.channels, info.sample_rate, info.frames()),
            (2, 48_000, 30_000)
        );
        // A stale header (cut short) is repaired from the length and stays RF64-readable.
        let f = OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(WAV_HEADER_LEN + 29_000 * 8 + 5).unwrap();
        drop(f);
        assert_eq!(repair_wav(&p).unwrap().frames(), 29_000);
        assert_eq!(read_info(&p).unwrap().frames(), 29_000);
        // FFmpeg reads RF64 (skipped without ffprobe).
        if let Ok(out) = std::process::Command::new("ffprobe")
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "csv=p=0",
            ])
            .arg(&p)
            .output()
        {
            if out.status.success() {
                let d: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().unwrap();
                assert!((d - 29_000.0 / 48_000.0).abs() < 0.01, "{d}");
            }
        }
    }

    #[test]
    fn a_file_below_the_limit_stays_plain_riff_that_hound_reads() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("short.wav");
        let mut w = WavWriter::create(&p, 1, 48_000)
            .unwrap()
            .with_rf64_after(40_000);
        w.write(&samples(1000)).unwrap();
        w.finalize().unwrap();
        assert_eq!(&std::fs::read(&p).unwrap()[0..4], b"RIFF");
        assert_eq!(hound::WavReader::open(&p).unwrap().duration(), 1000);
    }

    /// Files written before RF64 support (44-byte header) are still read and repaired.
    #[test]
    fn legacy_44_byte_headers_are_still_understood() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("old.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut w = hound::WavWriter::create(&p, spec).unwrap();
        for i in 0..2 * 5000 {
            w.write_sample((i % 100) as f32 / 100.0).unwrap();
        }
        w.finalize().unwrap();
        assert_eq!(read_info(&p).unwrap().frames(), 5000);
        // hound's header is 44 bytes or a little longer (a `fact` chunk): either way not ours.
        let hdr = std::fs::metadata(&p).unwrap().len() - 8 * 5000;
        assert_ne!(hdr, WAV_HEADER_LEN);
        let f = OpenOptions::new().write(true).open(&p).unwrap();
        f.set_len(hdr + 8 * 3000 + 1).unwrap();
        drop(f);
        assert_eq!(repair_wav(&p).unwrap().frames(), 3000);
        assert_eq!(hound::WavReader::open(&p).unwrap().duration(), 3000);
    }

    #[test]
    fn drop_without_finalize_still_patches() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("d.wav");
        {
            let mut w = WavWriter::create(&p, 1, 48_000).unwrap();
            w.write(&samples(100)).unwrap();
        }
        assert_eq!(hound::WavReader::open(&p).unwrap().duration(), 100);
    }
}
