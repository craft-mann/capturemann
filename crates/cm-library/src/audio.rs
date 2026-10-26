//! Audio bundles in the library grid (VOC-1.AC10): waveform peaks and the facts a card shows.
//!
//! Peaks are computed once from the bundle's WAV tracks (`mic`, `system`) and cached in
//! `peaks.json` inside the bundle, so the grid never decodes audio twice. The cache records the
//! size of each source file and is rebuilt when one changes. Everything here is headless and
//! read-mostly: the only write is the cache file.

use std::path::Path;

use cm_core::CmError;
use serde_json::{json, Value};

use crate::manifest::RecManifest;

/// Cache file inside the bundle.
pub const PEAKS_FILE: &str = "peaks.json";
/// Bars in the cached waveform (the grid card draws up to this many).
pub const PEAK_BUCKETS: usize = 96;
/// A bar is never lower than this (of 255), so silence still shows a line.
pub const PEAK_FLOOR: u8 = 6;

/// Largest absolute sample per bucket (0.0 to 1.0) over the whole WAV file, streamed so a
/// multi-hour file is never held in memory. Channels are folded together (the louder wins).
pub fn wav_peaks(path: &Path, buckets: usize) -> Result<Vec<f32>, CmError> {
    let io = |e: hound::Error| CmError::invalid_input(format!("{}: {e}", path.display()));
    let mut reader = hound::WavReader::open(path).map_err(io)?;
    let spec = reader.spec();
    let channels = usize::from(spec.channels.max(1));
    let frames = reader.duration() as usize;
    let mut out = vec![0f32; buckets];
    if frames == 0 || buckets == 0 {
        return Ok(out);
    }
    let mut feed = |frame: usize, v: f32| {
        let b = (frame * buckets / frames).min(buckets - 1);
        let v = v.abs();
        if v > out[b] {
            out[b] = v;
        }
    };
    match (spec.sample_format, spec.bits_per_sample) {
        (hound::SampleFormat::Float, _) => {
            for (i, s) in reader.samples::<f32>().enumerate() {
                feed(i / channels, s.map_err(io)?);
            }
        }
        (hound::SampleFormat::Int, bits) => {
            let scale = 1.0 / (1i64 << (bits.clamp(2, 32) - 1)) as f32;
            for (i, s) in reader.samples::<i32>().enumerate() {
                feed(i / channels, s.map_err(io)? as f32 * scale);
            }
        }
    }
    Ok(out)
}

/// Turns raw bucket peaks into bars of 0 to 255: scaled so the loudest is 255 (a quiet voice note
/// is as readable as a loud lecture), with a square-root curve so speech shows its shape, and
/// never below [`PEAK_FLOOR`].
pub fn normalize(raw: &[f32]) -> Vec<u8> {
    let max = raw.iter().copied().fold(0f32, f32::max);
    raw.iter()
        .map(|&v| {
            if max <= 0.0 {
                PEAK_FLOOR
            } else {
                ((v / max).sqrt() * 255.0)
                    .round()
                    .clamp(f32::from(PEAK_FLOOR), 255.0) as u8
            }
        })
        .collect()
}

/// Combines several tracks bucket by bucket (the loudest wins), then normalizes.
pub fn mix_peaks(tracks: &[Vec<f32>]) -> Vec<u8> {
    let n = tracks.iter().map(Vec::len).max().unwrap_or(0);
    let mut mixed = vec![0f32; n];
    for t in tracks {
        for (m, v) in mixed.iter_mut().zip(t) {
            *m = m.max(*v);
        }
    }
    normalize(&mixed)
}

/// The WAV files of a bundle's mic and system tracks, from the manifest (`tracks.<name>.file`)
/// with `<name>.wav` as the fallback, with their sizes.
fn source_files(dir: &Path, manifest: Option<&RecManifest>) -> Vec<(String, u64)> {
    let mut out = Vec::new();
    for name in ["mic", "system"] {
        let file = manifest
            .and_then(|m| m.tracks.get(name))
            .filter(|t| t.get("missing").and_then(Value::as_bool) != Some(true))
            .and_then(|t| t.get("file").and_then(Value::as_str))
            .map_or_else(|| format!("{name}.wav"), str::to_owned);
        if let Ok(meta) = std::fs::metadata(dir.join(&file)) {
            if meta.is_file() {
                out.push((file, meta.len()));
            }
        }
    }
    out
}

fn read_manifest(dir: &Path) -> Option<RecManifest> {
    let text = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    RecManifest::from_json(&text).ok()
}

/// Peaks of an audio bundle: the cached `peaks.json` when it still matches the sources,
/// otherwise computed now and cached. `None` when the bundle has no readable WAV track. A failed
/// cache write is not an error (the bars are still returned).
pub fn bundle_peaks(dir: &Path) -> Option<Vec<u8>> {
    let manifest = read_manifest(dir);
    let sources = source_files(dir, manifest.as_ref());
    if sources.is_empty() {
        return None;
    }
    let key = json!(sources
        .iter()
        .map(|(f, n)| json!([f, n]))
        .collect::<Vec<_>>());
    if let Some(cached) = std::fs::read_to_string(dir.join(PEAKS_FILE))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
    {
        if cached.get("sources") == Some(&key) {
            if let Some(bars) = cached.get("peaks").and_then(Value::as_array) {
                let bars: Vec<u8> = bars
                    .iter()
                    .filter_map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                    .collect();
                if !bars.is_empty() {
                    return Some(bars);
                }
            }
        }
    }
    let tracks: Vec<Vec<f32>> = sources
        .iter()
        .filter_map(|(f, _)| wav_peaks(&dir.join(f), PEAK_BUCKETS).ok())
        .collect();
    if tracks.is_empty() {
        return None;
    }
    let bars = mix_peaks(&tracks);
    let doc = json!({ "v": 1, "buckets": bars.len(), "sources": key, "peaks": bars });
    if let Ok(text) = serde_json::to_string(&doc) {
        let _ = crate::util::write_atomic(&dir.join(PEAKS_FILE), text.as_bytes());
    }
    Some(bars)
}

/// What an audio card shows besides the waveform.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AudioCardInfo {
    /// The preset id the recording was made with (`room`, `online`, `lecture`, `product`, `note`),
    /// when the manifest records one (`preset`, or `settings.preset`).
    pub preset: Option<String>,
    /// Number of speakers once speaker labels exist.
    pub speakers: Option<usize>,
    pub has_transcript: bool,
    pub has_report: bool,
}

/// Reads the facts of an audio bundle's card. Cheap: one manifest read and two `stat`s.
pub fn card_info(dir: &Path) -> AudioCardInfo {
    let m = read_manifest(dir);
    let preset = m.as_ref().and_then(|m| {
        m.extra
            .get("preset")
            .or_else(|| m.settings.get("preset"))
            .and_then(Value::as_str)
            .map(str::to_owned)
    });
    let speakers = m
        .as_ref()
        .and_then(|m| m.extra.get("speakers"))
        .and_then(Value::as_array)
        .map(Vec::len)
        .filter(|n| *n > 0);
    let has_report = m
        .as_ref()
        .and_then(|m| m.extra.get("reports"))
        .and_then(Value::as_array)
        .is_some_and(|a| !a.is_empty())
        || std::fs::read_dir(dir.join("reports")).is_ok_and(|mut d| d.next().is_some());
    AudioCardInfo {
        preset,
        speakers,
        has_transcript: dir.join("transcript.json").is_file(),
        has_report,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_wav(path: &Path, channels: u16, float: bool, samples: &[f32]) {
        let spec = hound::WavSpec {
            channels,
            sample_rate: 8000,
            bits_per_sample: if float { 32 } else { 16 },
            sample_format: if float {
                hound::SampleFormat::Float
            } else {
                hound::SampleFormat::Int
            },
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for &s in samples {
            if float {
                w.write_sample(s).unwrap();
            } else {
                w.write_sample((s * 32767.0) as i16).unwrap();
            }
        }
        w.finalize().unwrap();
    }

    /// First half silent, second half loud: the bars follow it, in both sample formats.
    #[test]
    fn peaks_follow_the_loudness() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = vec![0.0f32; 4000];
        s.extend(std::iter::repeat_n(0.5f32, 4000));
        for float in [true, false] {
            let p = dir.path().join(format!("t{float}.wav"));
            write_wav(&p, 1, float, &s);
            let raw = wav_peaks(&p, 8).unwrap();
            assert!(raw[..4].iter().all(|v| *v < 0.01), "{raw:?}");
            assert!(raw[4..].iter().all(|v| (*v - 0.5).abs() < 0.01), "{raw:?}");
            let bars = normalize(&raw);
            assert_eq!(bars.len(), 8);
            assert_eq!(bars[0], PEAK_FLOOR);
            assert_eq!(bars[7], 255);
        }
    }

    #[test]
    fn stereo_frames_and_silence() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.wav");
        // Frames of (left 0, right 0.8): the loudest channel counts, frames are not split.
        let s: Vec<f32> = (0..2000).flat_map(|_| [0.0, 0.8]).collect();
        write_wav(&p, 2, true, &s);
        let raw = wav_peaks(&p, 4).unwrap();
        assert!(raw.iter().all(|v| (*v - 0.8).abs() < 0.01));
        assert_eq!(normalize(&[0.0, 0.0]), vec![PEAK_FLOOR, PEAK_FLOOR]);
        assert!(normalize(&[]).is_empty());
        // A missing or empty file is an error or all zero, never a panic.
        assert!(wav_peaks(&dir.path().join("none.wav"), 4).is_err());
    }

    #[test]
    fn mixing_takes_the_louder_track() {
        let bars = mix_peaks(&[vec![0.1, 0.0], vec![0.0, 0.4]]);
        assert_eq!(bars[1], 255);
        assert!(bars[0] < 255 && bars[0] > PEAK_FLOOR);
    }

    /// The bundle helper falls back to `mic.wav`, writes `peaks.json`, reuses it, and rebuilds it
    /// when a source changes size.
    #[test]
    fn bundle_peaks_are_cached_and_refreshed() {
        let dir = tempfile::tempdir().unwrap();
        let mic = dir.path().join("mic.wav");
        write_wav(&mic, 1, false, &vec![0.3f32; 8000]);
        let first = bundle_peaks(dir.path()).unwrap();
        assert_eq!(first.len(), PEAK_BUCKETS);
        assert!(dir.path().join(PEAKS_FILE).is_file());
        // Poison the cache bars but keep its key: the cached bars are returned as they are.
        let mut doc: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join(PEAKS_FILE)).unwrap())
                .unwrap();
        doc["peaks"] = json!([1, 2, 3]);
        std::fs::write(dir.path().join(PEAKS_FILE), doc.to_string()).unwrap();
        assert_eq!(bundle_peaks(dir.path()).unwrap(), vec![1, 2, 3]);
        // A longer file changes the key: recomputed.
        write_wav(&mic, 1, false, &vec![0.3f32; 16000]);
        assert_eq!(bundle_peaks(dir.path()).unwrap().len(), PEAK_BUCKETS);
        // No tracks: nothing.
        let empty = tempfile::tempdir().unwrap();
        assert!(bundle_peaks(empty.path()).is_none());
    }

    #[test]
    fn card_info_reads_the_bundle() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(card_info(dir.path()), AudioCardInfo::default());
        std::fs::write(dir.path().join("transcript.json"), "{}").unwrap();
        std::fs::create_dir(dir.path().join("reports")).unwrap();
        std::fs::write(dir.path().join("reports").join("minutes.json"), "{}").unwrap();
        let i = card_info(dir.path());
        assert!(i.has_transcript && i.has_report);
        assert_eq!(i.speakers, None);
    }
}
