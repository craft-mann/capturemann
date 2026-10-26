//! FFmpeg-gated golden tests of the audio export (VOC-2.AC2, AC3, AC4). Fixtures are tones and
//! deterministic noise written with `hound`: nothing touches a microphone, camera or screen.
//! Skipped (with a message) when FFmpeg lacks the aac encoder or the loudnorm and ebur128
//! filters, or, for WAV, the `wav` muxer and `pcm_s24le` encoder (rebuild with `make ffmpeg`).
#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::process::{Command, Stdio};

use cm_media::audio_export::{
    check_components, export_audio, AudioFormat, AudioOptions, AudioOutcome, TrackChoice,
};
use cm_media::tools::{locate, Tools};
use serde_json::{json, Value};

const TPS: i64 = 120_000;
const RATE: u32 = 48_000;
const SECS: u32 = 40;

fn tools() -> Option<Tools> {
    match locate() {
        Ok(t) if t.ffprobe.is_some() => Some(t),
        _ => {
            eprintln!("SKIPPED: FFmpeg/ffprobe not found (brew install ffmpeg)");
            None
        }
    }
}

fn available(t: &Tools, opts: &AudioOptions) -> bool {
    match check_components(t, opts) {
        Ok(()) => true,
        Err(e) => {
            eprintln!("SKIPPED: {e}");
            false
        }
    }
}

/// Deterministic noise (xorshift) through a one-pole low-pass: speech-like in level, and with
/// nothing in common with a tone.
fn noise(n: usize, seed: u64) -> Vec<f32> {
    let mut x = seed;
    let mut lp = 0.0f32;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            let r = ((x >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0;
            lp += 0.2 * (r - lp);
            lp
        })
        .collect()
}

fn write_wav(path: &Path, channels: u16, samples: impl Fn(usize) -> f32, frames: usize) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..frames {
        let v = (samples(i).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        for _ in 0..channels {
            w.write_sample(v).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// A bundle shaped like a recording: a noise "voice" in `mic.denoised.wav` (mono) and a 1013 Hz (not a divisor of the rate: a 1 kHz tone repeats every 192 bytes and FFmpeg probes the file as MPEG-TS)
/// tone in `system.wav` (stereo). `with_screen` adds a screen track entry (its file never
/// exists: the audio export must not read any picture).
fn bundle(with_screen: bool) -> (tempfile::TempDir, Value) {
    let dir = tempfile::tempdir().unwrap();
    let n = (RATE * SECS) as usize;
    let voice = noise(n, 0x9E37_79B9_7F4A_7C15);
    write_wav(
        &dir.path().join("mic.denoised.wav"),
        1,
        |i| voice[i] * 0.6,
        n,
    );
    write_wav(
        &dir.path().join("system.wav"),
        2,
        |i| 0.25 * (2.0 * std::f32::consts::PI * 1013.0 * i as f32 / RATE as f32).sin(),
        n,
    );
    let end = i64::from(SECS) * TPS;
    let mut tracks = json!({
        "mic": {"file": "mic.denoised.wav", "channels": 1, "start": 0, "end": end},
        "system": {"file": "system.wav", "channels": 2, "start": 0, "end": end}
    });
    if with_screen {
        tracks["screen"] = json!({"file": "screen.mp4", "size": [1280, 720], "fps": 30,
                                  "start": 0, "end": end});
    }
    let manifest = json!({
        "title": "Golden", "created": "2026-10-26T09:00:00Z", "tracks": tracks,
        // Cut 10 s..20 s: 30 s remain.
        "edits": [{"version": 2, "in": 0, "out": end, "cuts": [[10 * TPS, 20 * TPS]]}]
    });
    (dir, manifest)
}

fn probe(t: &Tools, file: &Path, entries: &str) -> Value {
    let out = Command::new(t.ffprobe.as_ref().unwrap())
        .args(["-v", "error", "-show_entries", entries, "-of", "json"])
        .arg(file)
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

fn duration_s(t: &Tools, file: &Path) -> f64 {
    probe(t, file, "format=duration")["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

/// Decodes any audio file to mono f32 at 48 kHz.
fn decode_mono(t: &Tools, file: &Path) -> Vec<f32> {
    let out = Command::new(&t.ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(file)
        .args(["-ac", "1", "-ar", "48000", "-f", "f32le", "-"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    let (samples, _) = out.stdout.as_chunks::<4>();
    samples.iter().map(|c| f32::from_le_bytes(*c)).collect()
}

/// Integrated loudness (LUFS) of a file, from `ebur128`'s summary.
fn integrated_lufs(t: &Tools, file: &Path) -> f64 {
    let out = Command::new(&t.ffmpeg)
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(file)
        .args(["-af", "ebur128=peak=true", "-f", "null", "-"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stderr).into_owned();
    let summary = text.rsplit("Summary:").next().unwrap();
    let line = summary
        .lines()
        .find(|l| l.trim_start().starts_with("I:"))
        .unwrap_or_else(|| panic!("no loudness summary in:\n{text}"));
    line.split_whitespace().nth(1).unwrap().parse().unwrap()
}

fn run(t: &Tools, dir: &Path, m: &Value, opts: &AudioOptions) -> AudioOutcome {
    let mut last = 0.0;
    let out = export_audio(
        t,
        dir,
        m,
        opts,
        &mut |p| {
            assert!(p >= last - 1e-9, "progress went backwards: {last} -> {p}");
            last = p;
        },
        &|| false,
    )
    .unwrap_or_else(|e| panic!("export failed: {:?}", e.into_cm()));
    assert!((last - 1.0).abs() < 1e-9, "progress ended at {last}");
    assert!(out.path.starts_with(dir.join("exports")));
    out
}

/// VOC-2.AC2: a cut of 10 s from a 40 s recording exports as M4A Mix of 30 s (± 20 ms), AAC
/// 48 kHz, at -16 LUFS (± 1), with the title and date tags.
#[test]
fn voc2_ac2_m4a_mix_after_a_cut() {
    let Some(t) = tools() else { return };
    let opts = AudioOptions::default();
    if !available(&t, &opts) {
        return;
    }
    let (dir, m) = bundle(false);
    let out = run(&t, dir.path(), &m, &opts);
    assert!(out.file.ends_with("Golden - audio.m4a"), "{}", out.file);
    assert_eq!(out.loudness_mode, "two_pass");
    let d = duration_s(&t, &out.path);
    assert!((d - 30.0).abs() <= 0.020, "duration {d}");
    assert!((out.duration_s - 30.0).abs() < 1e-6);
    let info = probe(
        &t,
        &out.path,
        "stream=codec_name,sample_rate,channels,bit_rate:format_tags=title,date",
    );
    let s = &info["streams"][0];
    assert_eq!(s["codec_name"], "aac");
    assert_eq!(s["sample_rate"], "48000");
    assert_eq!(s["channels"], 2);
    let bps: f64 = s["bit_rate"].as_str().unwrap().parse().unwrap();
    assert!((150_000.0..230_000.0).contains(&bps), "bit rate {bps}");
    assert_eq!(info["format"]["tags"]["title"], "Golden");
    assert!(info["format"]["tags"]["date"]
        .as_str()
        .unwrap()
        .starts_with("2026-10-26"));
    let lufs = integrated_lufs(&t, &out.path);
    assert!(
        (lufs + 16.0).abs() <= 1.0,
        "integrated loudness {lufs} LUFS"
    );
    assert_eq!(std::fs::metadata(&out.path).unwrap().len(), out.size_bytes);
}

/// WAV is 48 kHz 24-bit; loudness stays off unless asked for.
#[test]
fn wav_is_24_bit_48k_and_untouched_without_loudness() {
    let Some(t) = tools() else { return };
    let opts = AudioOptions {
        format: AudioFormat::Wav,
        tracks: TrackChoice::Mix,
        loudness_lufs: AudioOptions::default_loudness(AudioFormat::Wav),
    };
    if !available(&t, &opts) {
        return;
    }
    let (dir, m) = bundle(false);
    let out = run(&t, dir.path(), &m, &opts);
    assert_eq!(out.loudness_mode, "off");
    let info = probe(&t, &out.path, "stream=codec_name,sample_rate,channels");
    assert_eq!(info["streams"][0]["codec_name"], "pcm_s24le");
    assert_eq!(info["streams"][0]["sample_rate"], "48000");
    let d = duration_s(&t, &out.path);
    assert!((d - 30.0).abs() <= 0.020, "duration {d}");
    // With a target it is normalised too.
    let loud = AudioOptions {
        loudness_lufs: Some(-16.0),
        ..opts
    };
    let out = run(&t, dir.path(), &m, &loud);
    let lufs = integrated_lufs(&t, &out.path);
    assert!((lufs + 16.0).abs() <= 1.0, "{lufs}");
}

fn correlation(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    let (mut ab, mut aa, mut bb) = (0.0f64, 0.0f64, 0.0f64);
    for i in 0..n {
        let (x, y) = (f64::from(a[i]), f64::from(b[i]));
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    ab / (aa.sqrt() * bb.sqrt()).max(1e-12)
}

/// VOC-2.AC3: Mic only has no system audio (correlation with the system track below 0.1), and
/// System only is that track (a positive control for the measurement).
#[test]
fn voc2_ac3_mic_only_excludes_the_system_track() {
    let Some(t) = tools() else { return };
    let opts = AudioOptions {
        format: AudioFormat::Wav,
        tracks: TrackChoice::Mic,
        loudness_lufs: None,
    };
    if !available(&t, &opts) {
        return;
    }
    let (dir, m) = bundle(false);
    // The system track as it should appear after the cut: 0..10 s and 20..40 s.
    let system = decode_mono(&t, &dir.path().join("system.wav"));
    let r = RATE as usize;
    let kept: Vec<f32> = [&system[..10 * r], &system[20 * r..40 * r]].concat();

    let mic = run(&t, dir.path(), &m, &opts);
    assert!(mic.file.ends_with("audio-mic.wav"), "{}", mic.file);
    let got = decode_mono(&t, &mic.path);
    assert!((got.len() as f64 / r as f64 - 30.0).abs() <= 0.020);
    let c = correlation(&got, &kept);
    assert!(c.abs() < 0.1, "mic-only correlates with system.wav: {c}");

    let sys = run(
        &t,
        dir.path(),
        &m,
        &AudioOptions {
            tracks: TrackChoice::System,
            ..opts
        },
    );
    let got = decode_mono(&t, &sys.path);
    let c = correlation(&got, &kept);
    assert!(c > 0.99, "system-only should be the system track: {c}");
}

/// VOC-2.AC4: "Export audio only" of a screen recording: the same cuts, an M4A with no video
/// stream, and no picture file is read (the screen file does not even exist here).
#[test]
fn voc2_ac4_audio_only_of_a_screen_recording() {
    let Some(t) = tools() else { return };
    let opts = AudioOptions::default();
    if !available(&t, &opts) {
        return;
    }
    let (dir, m) = bundle(true);
    let out = run(&t, dir.path(), &m, &opts);
    let info = probe(&t, &out.path, "stream=codec_type");
    let streams = info["streams"].as_array().unwrap();
    assert_eq!(streams.len(), 1);
    assert_eq!(streams[0]["codec_type"], "audio");
    let d = duration_s(&t, &out.path);
    assert!((d - 30.0).abs() <= 0.020, "duration {d}");
}

/// Nothing to export is `invalid_input`, and a cancelled export leaves no file behind.
#[test]
fn no_audio_and_cancel() {
    let Some(t) = tools() else { return };
    let opts = AudioOptions::default();
    if !available(&t, &opts) {
        return;
    }
    let empty = tempfile::tempdir().unwrap();
    let e = export_audio(
        &t,
        empty.path(),
        &json!({"title": "x"}),
        &opts,
        &mut |_| {},
        &|| false,
    )
    .unwrap_err()
    .into_cm();
    assert_eq!(e.code().as_str(), "invalid_input");

    let (dir, m) = bundle(false);
    let r = export_audio(&t, dir.path(), &m, &opts, &mut |_| {}, &|| true);
    assert!(matches!(r, Err(cm_media::ExportError::Cancelled)));
    let left: Vec<_> = std::fs::read_dir(dir.path().join("exports"))
        .map(|d| d.flatten().collect())
        .unwrap_or_default();
    assert!(left.is_empty(), "{left:?}");
}
