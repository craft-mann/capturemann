//! AUD-6: muting the microphone while it records. The track keeps running; the raw and the
//! denoised file hold exact zeros for the frames captured while muted and real samples elsewhere.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use cm_audio::{
    AudioSource, DeviceSelector, FakeAudioSource, FakeConfig, FakeHandle, MicConfig, MicRecorder,
    SourceFactory,
};
use cm_core::{CmError, ManualHostClock, MediaTime, SessionClock, TrackContext, TrackRecorder};

const ORIGIN_NS: u64 = 10_000_000_000;

struct Rig {
    host: Arc<ManualHostClock>,
    clock: SessionClock,
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        let host = Arc::new(ManualHostClock::new(ORIGIN_NS));
        Rig {
            clock: SessionClock::with_host(host.clone()),
            host,
            dir: tempfile::tempdir().unwrap(),
        }
    }
    fn ctx(&self) -> TrackContext {
        TrackContext {
            dir: self.dir.path().to_path_buf(),
            clock: self.clock.clone(),
        }
    }
    fn path(&self, f: &str) -> std::path::PathBuf {
        self.dir.path().join(f)
    }
    /// Moves the session clock to `ns` after the origin.
    fn set(&self, ns: u64) {
        self.host.set_ns(ORIGIN_NS + ns);
    }
    fn cfg(&self) -> FakeConfig {
        FakeConfig {
            host_start_ns: ORIGIN_NS,
            ..FakeConfig::default()
        }
    }
}

fn factory_of(source: FakeAudioSource) -> SourceFactory {
    let cell = Mutex::new(Some(source));
    Box::new(move |_sel: &DeviceSelector| {
        cell.lock()
            .unwrap()
            .take()
            .map(|s| Box::new(s) as Box<dyn AudioSource>)
            .ok_or_else(|| CmError::not_found("no more devices"))
    })
}

fn cfg(denoise: bool) -> MicConfig {
    MicConfig {
        denoise,
        ..MicConfig::default()
    }
}

fn read(path: &Path) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap();
    r.samples::<f32>().map(|s| s.unwrap()).collect()
}

fn peak(x: &[f32]) -> f32 {
    x.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// Delivers `secs`, waits until the worker has processed it, then moves the clock to `now_ns`.
fn play_until(rig: &Rig, h: &FakeHandle, rec: &MicRecorder, secs: f64, now_ns: u64) {
    h.advance(secs);
    rec.sync().unwrap();
    rig.set(now_ns);
}

#[test]
fn muting_before_start_is_refused_and_toggles_are_idempotent() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    assert!(rec.set_muted(true).is_err(), "no worker yet");
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 0.5, 500_000_000);
    assert!(!rec.is_muted());
    assert!(!rec.set_muted(false).unwrap().changed);
    assert!(rec.set_muted(true).unwrap().changed);
    assert!(rec.is_muted());
    let again = rec.set_muted(true).unwrap();
    assert!(!again.changed);
    assert!(rec.set_muted(false).unwrap().changed);
    rec.stop(MediaTime::from_ticks(120_000)).unwrap();
}

#[test]
fn frames_captured_from_the_mute_time_on_are_exact_zeros_and_the_rest_is_untouched() {
    // Boundary: a frame is silenced iff its capture time is >= the mute time (and < the unmute
    // time). 1.0034 s is 48163.2 frames: frame 48164 is the first one at or after it.
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_003_400_000);
    let m = rec.set_muted(true).unwrap();
    assert!(m.changed);
    h.advance(0.5);
    rec.sync().unwrap();
    // Unmute exactly on frame 72000 (1.5 s).
    rig.set(1_500_000_000);
    assert!(rec.set_muted(false).unwrap().changed);
    h.advance(0.5);
    let info = rec.stop(MediaTime::from_ticks(2 * 120_000)).unwrap();

    let s = read(&rig.path("mic.wav"));
    assert_eq!(s.len(), 96_000, "the track was never stopped or restarted");
    assert!(s[..48_164].iter().filter(|v| **v != 0.0).count() > 40_000);
    assert!(
        s[48_163] != 0.0,
        "the frame just before the mute time is real"
    );
    assert!(
        s[48_164..72_000].iter().all(|v| *v == 0.0),
        "digital silence"
    );
    assert!(
        peak(&s[72_000..72_100]) > 0.4,
        "the frame at the unmute time is real"
    );
    assert!(peak(&s[72_000..]) > 0.45);
    // The facts for the manifest, in session ticks (120 000 per second).
    let muted = info.details["muted"].as_array().unwrap();
    assert_eq!(muted.len(), 1);
    assert_eq!(muted[0]["from"], 120_408);
    assert_eq!(muted[0]["to"], 180_000);
}

#[test]
fn the_denoised_file_is_silent_in_the_same_span_and_has_the_same_length() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(true), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_000_000_000);
    rec.set_muted(true).unwrap();
    play_until(&rig, &h, &rec, 1.0, 2_000_000_000);
    rec.set_muted(false).unwrap();
    h.advance(1.0);
    rec.stop(MediaTime::from_ticks(3 * 120_000)).unwrap();

    let raw = read(&rig.path("mic.wav"));
    let den = read(&rig.path("mic.denoised.wav"));
    assert_eq!(raw.len(), 144_000);
    assert_eq!(den.len(), raw.len());
    assert!(raw[48_000..96_000].iter().all(|v| *v == 0.0));
    assert!(
        den[48_000..96_000].iter().all(|v| *v == 0.0),
        "the denoiser must not leak its tail into the muted span"
    );
    assert!(peak(&raw[..48_000]) > 0.45 && peak(&raw[96_000..]) > 0.45);
}

#[test]
fn a_mic_muted_before_its_first_sample_records_silence_from_the_start() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    rec.set_muted(true).unwrap();
    play_until(&rig, &h, &rec, 0.5, 500_000_000);
    rec.set_muted(false).unwrap();
    h.advance(0.5);
    rec.stop(MediaTime::from_ticks(120_000)).unwrap();
    let s = read(&rig.path("mic.wav"));
    assert_eq!(s.len(), 48_000);
    assert!(s[..24_000].iter().all(|v| *v == 0.0));
    assert!(peak(&s[24_000..]) > 0.45);
}

#[test]
fn a_44k1_device_gets_the_same_span_within_one_output_frame() {
    let rig = Rig::new();
    let mut c = rig.cfg();
    c.sample_rate = 44_100;
    c.chunk_frames = 441;
    let (src, h) = FakeAudioSource::new(c);
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_000_000_000);
    rec.set_muted(true).unwrap();
    play_until(&rig, &h, &rec, 1.0, 2_000_000_000);
    rec.set_muted(false).unwrap();
    h.advance(1.0);
    rec.stop(MediaTime::from_ticks(3 * 120_000)).unwrap();
    let s = read(&rig.path("mic.wav"));
    assert!((s.len() as i64 - 144_000).abs() <= 2, "{}", s.len());
    assert!(s[48_001..95_999].iter().all(|v| *v == 0.0));
    assert!(peak(&s[..47_900]) > 0.45);
    assert!(peak(&s[96_100..]) > 0.45);
}

#[test]
fn mute_spans_that_cross_a_pause_map_to_the_gapless_file() {
    // Session: record 0..1, mute at 1, pause 1.5..2.5 (muted all along), unmute at 3, record to 4.
    // The file has no hole: session [1, 1.5) and [2.5, 3) are file [1, 2).
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_000_000_000);
    rec.set_muted(true).unwrap();
    h.advance(0.5);
    rec.pause(MediaTime::from_ticks(180_000)).unwrap();
    h.advance(1.0);
    rec.resume(MediaTime::from_ticks(300_000)).unwrap();
    play_until(&rig, &h, &rec, 0.5, 3_000_000_000);
    rec.set_muted(false).unwrap();
    h.advance(1.0);
    let info = rec.stop(MediaTime::from_ticks(4 * 120_000)).unwrap();
    let s = read(&rig.path("mic.wav"));
    assert_eq!(s.len(), 144_000);
    assert!(peak(&s[..48_000]) > 0.45);
    assert!(s[48_000..96_000].iter().all(|v| *v == 0.0));
    assert!(peak(&s[96_000..]) > 0.45);
    let muted = info.details["muted"].as_array().unwrap();
    assert_eq!(
        (muted[0]["from"].as_i64(), muted[0]["to"].as_i64()),
        (Some(120_000), Some(360_000))
    );
}

#[test]
fn muting_during_a_pause_silences_what_follows_the_resume() {
    // Pause 1..2, mute at 1.5 (while paused), resume at 2, unmute at 3 (session).
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_000_000_000);
    rec.pause(MediaTime::from_ticks(120_000)).unwrap();
    h.advance(0.5);
    rec.sync().unwrap();
    rig.set(1_500_000_000);
    rec.set_muted(true).unwrap();
    h.advance(0.5);
    rec.resume(MediaTime::from_ticks(240_000)).unwrap();
    play_until(&rig, &h, &rec, 1.0, 3_000_000_000);
    rec.set_muted(false).unwrap();
    h.advance(1.0);
    rec.stop(MediaTime::from_ticks(4 * 120_000)).unwrap();
    let s = read(&rig.path("mic.wav"));
    assert_eq!(s.len(), 144_000);
    assert!(peak(&s[..48_000]) > 0.45);
    assert!(s[48_000..96_000].iter().all(|v| *v == 0.0));
    assert!(peak(&s[96_000..]) > 0.45);
}

#[test]
fn the_digital_silence_alert_is_not_raised_by_a_deliberate_mute() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg());
    let mut rec = MicRecorder::with_factory(cfg(false), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    play_until(&rig, &h, &rec, 1.0, 1_000_000_000);
    rec.set_muted(true).unwrap();
    h.advance(20.0);
    rec.sync().unwrap();
    assert!(
        rec.drain_events().is_empty(),
        "the live signal was never silent"
    );
    rec.stop(MediaTime::from_ticks(21 * 120_000)).unwrap();
}
