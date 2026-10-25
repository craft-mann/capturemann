//! Recorder behaviour over a scripted source: timestamp placement, drift, gaps, pause, device loss.
//! Cites AC IDs from specs/features/05-audio.md.

#![allow(clippy::unwrap_used)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use cm_audio::{
    AudioEvent, DeviceSelector, FakeAudioSource, FakeConfig, FakeHandle, FakeSignal, MicConfig,
    MicRecorder, SourceFactory, SystemAudioConfig, SystemAudioRecorder,
};
use cm_core::{
    CmError, ErrorCode, ManualHostClock, MediaTime, SessionClock, TrackContext, TrackRecorder,
};

const ORIGIN_NS: u64 = 10_000_000_000;

struct Rig {
    clock: SessionClock,
    dir: tempfile::TempDir,
}

impl Rig {
    fn new() -> Rig {
        Rig {
            clock: SessionClock::with_host(Arc::new(ManualHostClock::new(ORIGIN_NS))),
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
    /// Session time of the fake device's next sample.
    fn at(&self, h: &FakeHandle) -> MediaTime {
        self.clock.from_host_ns(h.next_host_ns())
    }
    fn cfg(&self, offset_ms: u64) -> FakeConfig {
        FakeConfig {
            host_start_ns: ORIGIN_NS + offset_ms * 1_000_000,
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
            .map(|s| Box::new(s) as Box<dyn cm_audio::AudioSource>)
            .ok_or_else(|| CmError::not_found("no more devices"))
    })
}

fn no_denoise() -> MicConfig {
    MicConfig {
        denoise: false,
        ..MicConfig::default()
    }
}

fn read(path: &Path) -> (hound::WavSpec, Vec<f32>) {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    (spec, r.samples::<f32>().map(|s| s.unwrap()).collect())
}

fn frames(path: &Path) -> usize {
    let (spec, s) = read(path);
    s.len() / usize::from(spec.channels)
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
}

#[test]
fn records_a_tone_to_48k_float_wav_with_its_capture_start() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(250));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(2.0);
    let info = rec.stop(rig.at(&h)).unwrap();

    let (spec, s) = read(&rig.path("mic.wav"));
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.bits_per_sample),
        (1, 48_000, 32)
    );
    assert_eq!(spec.sample_format, hound::SampleFormat::Float);
    assert_eq!(s.len(), 96_000);
    // First sample sits at the OS capture time of the first buffer (250 ms after the origin).
    assert_eq!(info.start.ticks(), 30_000);
    assert_eq!(info.end.ticks(), 30_000 + 240_000);
    assert_eq!(info.file, "mic.wav");
    assert_eq!(info.details["first_sample_ticks"], 30_000);
    // The tone survived: amplitude 0.5, 1 kHz.
    assert!((s[100..4_900].iter().fold(0.0f32, |m, v| m.max(v.abs())) - 0.5).abs() < 0.01);
    let st = rec.stats();
    assert!(st.delivered >= 200 && st.written == st.delivered && st.dropped == 0);
}

#[test]
fn frames_before_the_session_origin_are_dropped() {
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.host_start_ns = ORIGIN_NS - 100_000_000;
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    assert!(
        info.start.ticks() >= 0 && info.start.ticks() <= 3,
        "{:?}",
        info.start
    );
    assert_eq!(frames(&rig.path("mic.wav")), 43_200);
}

#[test]
fn a_44k1_device_is_resampled_to_48k_with_the_right_duration() {
    // AUD-1.AC3
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.sample_rate = 44_100;
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(5.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    let (spec, s) = read(&rig.path("mic.wav"));
    assert_eq!(spec.sample_rate, 48_000);
    assert!((s.len() as i64 - 240_000).abs() <= 480, "{}", s.len()); // +-10 ms
    assert_eq!(info.details["source_rate"], 44_100);
}

#[test]
fn stereo_devices_keep_two_channels_and_mono_is_not_duplicated() {
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.channels = 2;
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    rec.stop(rig.at(&h)).unwrap();
    let (spec, s) = read(&rig.path("mic.wav"));
    assert_eq!(spec.channels, 2);
    assert_eq!(s.len(), 96_000);
}

#[test]
fn a_dropout_is_filled_with_silence_and_reported_as_a_gap() {
    // AUD-4: placement by timestamp, never by sample count.
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    h.skip(0.5);
    h.advance(1.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    let (_, s) = read(&rig.path("mic.wav"));
    assert!((s.len() as i64 - 120_000).abs() <= 48, "{}", s.len());
    assert!(
        rms(&s[48_000 + 480..72_000 - 480]) < 1e-6,
        "gap must be silent"
    );
    assert!(
        rms(&s[72_000 + 480..96_000]) > 0.3,
        "audio resumes after the gap"
    );
    let gaps = info.details["gaps"].as_array().unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["reason"], "dropout");
    let (from, to) = (
        gaps[0]["from"].as_i64().unwrap(),
        gaps[0]["to"].as_i64().unwrap(),
    );
    assert!(
        (from - 120_000).abs() <= 120 && (to - 180_000).abs() <= 120,
        "{from} {to}"
    );
    assert_eq!(info.end.ticks(), 300_000);
}

#[test]
fn overlapping_stamps_are_trimmed() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    h.shift_host_ns(-100_000_000); // the OS clock steps back 100 ms
    h.advance(1.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    let n = frames(&rig.path("mic.wav"));
    // 100 ms of overlap: whole buffers are dropped until the residual (here 20 ms) is within the
    // tolerance, so 80..100 ms goes and the track ends within 20 ms of the OS stamps.
    assert!((91_200..=92_200).contains(&n), "{n}");
    let trimmed = info.details["resyncs"]["trimmed_ms"].as_f64().unwrap();
    assert!((80.0..=100.0).contains(&trimmed), "{trimmed}");
    let end_err = info.end.as_secs_f64() - n as f64 / 48_000.0 - 0.0;
    assert!(end_err.abs() < 0.021, "{end_err}");
}

#[test]
fn jitter_below_the_threshold_does_not_touch_the_audio() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(0.5);
    h.shift_host_ns(8_000_000); // 8 ms late: inside the 20 ms tolerance
    h.advance(0.5);
    let info = rec.stop(rig.at(&h)).unwrap();
    assert_eq!(frames(&rig.path("mic.wav")), 48_000);
    assert_eq!(info.details["resyncs"]["count"], 0);
    assert!((info.details["max_offset_ms"].as_f64().unwrap() - 8.0).abs() < 1.0);
}

#[test]
fn a_slow_device_clock_is_measured_and_the_track_stays_within_the_sync_budget() {
    // AUD-1.AC2 / NFR-3 at accelerated speed: 1000 ppm of drift over 60 s is 60 ms, past the 50 ms
    // budget unless the pipeline follows the timestamps.
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.drift_ppm = 1000.0;
    cfg.signal = FakeSignal::Beeps {
        period_s: 5.0,
        len_s: 0.05,
        freq_hz: 1000.0,
        amp: 0.5,
    };
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(60.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    let (_, s) = read(&rig.path("mic.wav"));

    // File length matches host elapsed time (60.06 s), not the sample count (60.00 s).
    let host_elapsed = 60.0 * 1.001;
    let file_secs = s.len() as f64 / 48_000.0;
    assert!((file_secs - host_elapsed).abs() < 0.021, "{file_secs}");

    // Every beep lands at its OS capture time within 25 ms (budget 50 ms).
    let mut onsets = Vec::new();
    let mut quiet = true;
    for (i, v) in s.iter().enumerate() {
        if quiet && v.abs() > 0.05 {
            onsets.push(i as f64 / 48_000.0);
            quiet = false;
        } else if !quiet && i > 0 && s[i.saturating_sub(2400)..i].iter().all(|x| x.abs() < 0.01) {
            quiet = true;
        }
    }
    assert!(onsets.len() >= 11, "{onsets:?}");
    for (k, onset) in onsets.iter().enumerate() {
        let want = k as f64 * 5.0 * 1.001; // beep k at device time 5k s = host 5k * 1.001 s
        assert!(
            (onset - want).abs() < 0.025,
            "beep {k}: at {onset}, want {want}"
        );
    }

    let ppm = info.details["drift_ppm"].as_f64().unwrap();
    assert!((ppm - 1000.0).abs() < 100.0, "drift {ppm} ppm");
    assert!(info.details["resyncs"]["count"].as_u64().unwrap() >= 2);
}

#[test]
fn pause_writes_no_audio_and_leaves_no_hole_in_the_file() {
    // REC-4 contract: "continuous timestamps (no gap in the output)"; session gap recorded.
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    rec.pause(rig.at(&h)).unwrap();
    h.advance(2.0);
    rec.resume(rig.at(&h)).unwrap();
    h.advance(1.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    assert!(
        (frames(&rig.path("mic.wav")) as i64 - 96_000).abs() <= 1,
        "file is 2 s"
    );
    assert_eq!(info.start.ticks(), 0);
    assert_eq!(
        info.end.ticks(),
        4 * 120_000,
        "session time still spans the pause"
    );
    let pauses = info.details["pauses"].as_array().unwrap();
    assert_eq!(pauses.len(), 1);
    assert_eq!(pauses[0]["from"], 120_000);
    assert_eq!(pauses[0]["to"], 360_000);
    assert!(
        info.details["gaps"].as_array().unwrap().is_empty(),
        "a pause is not a dropout"
    );
    assert_eq!(info.details["resyncs"]["count"], 0);
}

#[test]
fn pause_inside_a_buffer_splits_it_on_the_timestamp() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    // Pause 3 ms into a 10 ms buffer: only 3 ms of it may be written.
    let pause_at = MediaTime::from_ticks(120_000 + 360); // 1.003 s
    let resume_at = MediaTime::from_ticks(2 * 120_000 + 360); // 2.003 s
    h.advance(1.0);
    rec.pause(pause_at).unwrap();
    rec.resume(resume_at).unwrap();
    h.advance(2.0);
    let info = rec.stop(rig.at(&h)).unwrap();
    // 1.003 s before the pause + (3.0 - 2.003) s after.
    let want = (1.003 + 0.997) * 48_000.0;
    assert!((frames(&rig.path("mic.wav")) as f64 - want).abs() <= 2.0);
    assert_eq!(info.details["resyncs"]["count"], 0);
}

#[test]
fn device_unplugged_switches_to_the_default_and_marks_the_gap() {
    // AUD-1 edge case: "switch to the system default within 1 s, mark the gap, toast".
    let rig = Rig::new();
    let (first, h1) = FakeAudioSource::new(rig.cfg(0));
    let mut cfg2 = rig.cfg(1_300);
    cfg2.device_name = "Fake USB Mic".into();
    cfg2.device_id = "fake:usb".into();
    cfg2.sample_rate = 44_100; // the new device runs at another rate
    let (second, h2) = FakeAudioSource::new(cfg2);
    let queue = Mutex::new(vec![second, first]);
    let factory: SourceFactory = Box::new(move |_s| {
        queue
            .lock()
            .unwrap()
            .pop()
            .map(|s| Box::new(s) as Box<dyn cm_audio::AudioSource>)
            .ok_or_else(|| CmError::not_found("no device"))
    });
    let mut rec = MicRecorder::with_factory(no_denoise(), factory);
    rec.start(rig.ctx()).unwrap();
    h1.advance(1.0);
    h1.disconnect("unplugged");
    rec.sync().unwrap();
    assert_eq!(rec.device_format().unwrap().device_name, "Fake USB Mic");
    assert!(!h1.is_started() && h2.is_started());
    h2.advance(1.0);
    let info = rec.stop(rig.at(&h2)).unwrap();

    let events = rec.drain_events();
    assert!(matches!(&events[0], AudioEvent::DeviceLost { name, .. } if name == "Fake Mic"));
    assert!(
        matches!(&events[1], AudioEvent::DeviceChanged { from, to, .. }
        if from == "Fake Mic" && to == "Fake USB Mic")
    );
    // 1 s + 0.3 s silent gap + 1 s.
    assert!((frames(&rig.path("mic.wav")) as i64 - 110_400).abs() <= 960);
    let gaps = info.details["gaps"].as_array().unwrap();
    assert_eq!(gaps.len(), 1);
    assert_eq!(gaps[0]["reason"], "device_changed");
    assert_eq!(info.details["device_changes"].as_array().unwrap().len(), 1);
    assert_eq!(info.details["device"]["name"], "Fake USB Mic");
}

#[test]
fn no_replacement_device_keeps_recording_silence_until_one_appears() {
    let rig = Rig::new();
    let (first, h1) = FakeAudioSource::new(rig.cfg(0));
    let (second, h2) = {
        let mut c = rig.cfg(2_000);
        c.device_name = "Back Again".into();
        FakeAudioSource::new(c)
    };
    let calls = Arc::new(Mutex::new(0));
    let c2 = calls.clone();
    let queue = Mutex::new(vec![second, first]);
    let factory: SourceFactory = Box::new(move |_s| {
        let mut n = c2.lock().unwrap();
        *n += 1;
        if *n == 2 {
            return Err(CmError::not_found("no microphone"));
        }
        queue
            .lock()
            .unwrap()
            .pop()
            .map(|s| Box::new(s) as Box<dyn cm_audio::AudioSource>)
            .ok_or_else(|| CmError::not_found("no device"))
    });
    let mut rec = MicRecorder::with_factory(no_denoise(), factory);
    rec.start(rig.ctx()).unwrap();
    h1.advance(1.0);
    h1.disconnect("unplugged");
    rec.sync().unwrap();
    assert!(matches!(
        rec.drain_events().as_slice(),
        [AudioEvent::DeviceLost { .. }]
    ));
    // The worker retries every 250 ms; the third call succeeds.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if h2.is_started() {
            break;
        }
    }
    assert!(h2.is_started(), "worker must reopen the default device");
    h2.advance(1.0);
    let info = rec.stop(rig.at(&h2)).unwrap();
    assert!((frames(&rig.path("mic.wav")) as i64 - 144_000).abs() <= 960);
    assert_eq!(info.details["gaps"].as_array().unwrap().len(), 1);
}

#[test]
fn start_failures_map_to_shared_error_codes() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    h.fail_next_start(CmError::device_busy("Fake Mic is in use"));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    let err = rec.start(rig.ctx()).unwrap_err();
    assert_eq!(err.code(), ErrorCode::DeviceBusy);

    let factory: SourceFactory = Box::new(|_s| {
        Err(CmError::permission_missing(
            cm_core::Permission::Microphone,
            "off",
        ))
    });
    let mut rec = MicRecorder::with_factory(no_denoise(), factory);
    let err = rec.start(rig.ctx()).unwrap_err();
    assert_eq!(err.code(), ErrorCode::PermissionMissing);
    // AUD-1.AC5: no files for a mic that never started... the session omits the track.
    assert!(rec.stop(MediaTime::ZERO).is_err());
}

#[test]
fn a_missing_chosen_device_falls_back_to_the_default() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let cell = Mutex::new(Some(src));
    let factory: SourceFactory = Box::new(move |sel| match sel {
        DeviceSelector::Id(id) => Err(CmError::not_found(format!("{id} gone"))),
        DeviceSelector::Default => Ok(Box::new(cell.lock().unwrap().take().unwrap())),
    });
    let mut rec = MicRecorder::with_factory(
        MicConfig {
            device: DeviceSelector::Id("coreaudio:old".into()),
            denoise: false,
            ..MicConfig::default()
        },
        factory,
    );
    rec.start(rig.ctx()).unwrap();
    h.advance(0.5);
    rec.stop(rig.at(&h)).unwrap();
    assert!(matches!(
        rec.drain_events().as_slice(),
        [AudioEvent::DeviceFallback { requested, using }] if requested == "coreaudio:old" && using == "Fake Mic"
    ));
}

#[test]
fn denoise_on_writes_a_second_file_of_the_same_length_and_off_writes_none() {
    // AUD-3: on by default, beside the raw file; toggle off.
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(FakeConfig {
        signal: FakeSignal::Noise { amp: 0.02 },
        ..rig.cfg(0)
    });
    let mut rec = MicRecorder::with_factory(MicConfig::default(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.5);
    let info = rec.stop(rig.at(&h)).unwrap();
    assert_eq!(info.file, "mic.denoised.wav");
    assert_eq!(info.details["raw_file"], "mic.wav");
    assert_eq!(info.details["denoised_file"], "mic.denoised.wav");
    assert_eq!(frames(&rig.path("mic.wav")), 72_000);
    assert_eq!(frames(&rig.path("mic.denoised.wav")), 72_000);

    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(0.5);
    let info = rec.stop(rig.at(&h)).unwrap();
    assert_eq!(info.file, "mic.wav");
    assert!(!rig.path("mic.denoised.wav").exists());
    assert_eq!(info.details["denoise"], false);
}

#[test]
fn the_level_handle_follows_the_signal_while_recording() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(FakeConfig {
        signal: FakeSignal::Constant(0.5),
        ..rig.cfg(0)
    });
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    let level = rec.level_handle();
    assert_eq!(
        level.read().peak_dbfs,
        cm_audio::FLOOR_DBFS,
        "silent before start"
    );
    rec.start(rig.ctx()).unwrap();
    h.advance(0.2);
    let l = level.read();
    assert!(
        (l.peak_dbfs + 6.02).abs() < 0.1 && (l.rms_dbfs + 6.02).abs() < 0.1,
        "{l:?}"
    );
    rec.stop(rig.at(&h)).unwrap();
}

#[test]
fn a_recorder_dropped_without_stop_leaves_readable_files() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    {
        let mut rec = MicRecorder::with_factory(MicConfig::default(), factory_of(src));
        rec.start(rig.ctx()).unwrap();
        h.advance(1.0);
        rec.sync().unwrap();
    }
    assert!(frames(&rig.path("mic.wav")) >= 47_000);
    assert!(frames(&rig.path("mic.denoised.wav")) >= 47_000);
}

#[test]
fn a_source_on_another_timebase_is_shifted_onto_the_session_clock() {
    // Defensive: a host whose capture stamps are not on the session clock must not put audio
    // minutes away from the video.
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.host_start_ns = 5_000_000_000_000; // 5000 s on some other base
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(1.0);
    let info = rec.stop(MediaTime::from_ticks(240_000)).unwrap();
    assert_eq!(info.details["timestamp_shifted"], true);
    assert!(info.start.ticks().abs() < 12_000, "{:?}", info.start);
}

#[test]
fn system_audio_is_stereo_with_no_denoised_copy_and_follows_a_default_change() {
    // AUD-2: system.wav 48 kHz stereo; AUD-2.AC3 (Windows device change) via needs_reopen.
    let rig = Rig::new();
    let (first, h1) = FakeAudioSource::new(FakeConfig {
        device_name: "Speakers".into(),
        ..rig.cfg(0)
    });
    let (second, h2) = FakeAudioSource::new(FakeConfig {
        device_name: "Headphones".into(),
        ..rig.cfg(1_500)
    });
    let queue = Mutex::new(vec![second, first]);
    let factory: SourceFactory = Box::new(move |_s| {
        queue
            .lock()
            .unwrap()
            .pop()
            .map(|s| Box::new(s) as Box<dyn cm_audio::AudioSource>)
            .ok_or_else(|| CmError::not_found("no device"))
    });
    let mut rec = SystemAudioRecorder::with_factory(SystemAudioConfig::default(), factory);
    rec.start(rig.ctx()).unwrap();
    h1.advance(1.0);
    h1.request_reopen();
    // The worker polls ~4 times a second.
    for _ in 0..40 {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if h2.is_started() {
            break;
        }
    }
    assert!(h2.is_started());
    h2.advance(1.0);
    let info = rec.stop(rig.at(&h2)).unwrap();
    let (spec, s) = read(&rig.path("system.wav"));
    assert_eq!((spec.channels, spec.sample_rate), (2, 48_000));
    assert!((s.len() as i64 / 2 - 120_000).abs() <= 960);
    assert!(!rig.path("system.denoised.wav").exists());
    assert_eq!(info.file, "system.wav");
    assert_eq!(info.kind, cm_core::TrackKind::System);
    let events = rec.drain_events();
    assert!(
        matches!(events.as_slice(), [AudioEvent::DeviceChanged { to, .. }] if to == "Headphones")
    );
}

/// R3-1: a write error (disk full, I/O error) keeps the audio written so far: the track ends where
/// its file ends, `stop` returns the track with `truncated_at`, and the recorder reports a
/// `WriteFailed` event while it runs. Nothing is deleted.
#[test]
fn r3_1_a_write_error_keeps_the_files_and_marks_the_track_truncated() {
    let rig = Rig::new();
    let (src, h) = FakeAudioSource::new(rig.cfg(0));
    // 1 s of mono f32 is 192_000 bytes: fail after 1.5 s.
    let mut rec = MicRecorder::with_factory(MicConfig::default(), factory_of(src))
        .with_write_fault_after(288_000);
    rec.start(rig.ctx()).unwrap();
    h.advance(3.0);
    rec.sync().unwrap();
    let events = rec.drain_events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, AudioEvent::WriteFailed { .. })),
        "{events:?}"
    );
    let info = rec.stop(rig.at(&h)).unwrap();
    assert!(rig.path("mic.wav").exists());
    let n = frames(&rig.path("mic.wav"));
    assert!((71_000..=72_100).contains(&n), "{n}");
    let t = info.details["truncated_at"].as_i64().unwrap();
    assert!((t - 180_000).abs() < 1_500, "{t}");
    assert!(info.end.ticks() <= t);
    assert!(info.details["write_error"]
        .as_str()
        .unwrap()
        .contains("mic.wav"));
    // The denoised copy is kept too and is readable.
    assert!(frames(&rig.path("mic.denoised.wav")) > 0);
}

/// R3-13: a start that times out stops the worker, closes the device and deletes the partial
/// files.
#[test]
fn r3_13_a_start_timeout_stops_the_worker_and_cleans_up() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let rig = Rig::new();
    let (src, _h) = FakeAudioSource::new(rig.cfg(0));
    let opened = Arc::new(AtomicBool::new(false));
    let o = opened.clone();
    let cell = Mutex::new(Some(src));
    let factory: SourceFactory = Box::new(move |_sel: &DeviceSelector| {
        std::thread::sleep(std::time::Duration::from_millis(400));
        o.store(true, Ordering::SeqCst);
        cell.lock()
            .unwrap()
            .take()
            .map(|s| Box::new(s) as Box<dyn cm_audio::AudioSource>)
            .ok_or_else(|| CmError::not_found("no more devices"))
    });
    let mut rec = MicRecorder::with_factory(MicConfig::default(), factory)
        .with_start_timeout(std::time::Duration::from_millis(100));
    let e = rec.start(rig.ctx()).unwrap_err();
    assert!(e.to_string().contains("timed out"), "{e}");
    // The device opens late; the worker then stops by itself and the files are removed.
    for _ in 0..100 {
        if opened.load(Ordering::SeqCst)
            && !rig.path("mic.wav").exists()
            && !rig.path("mic.denoised.wav").exists()
        {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("the partial files were not cleaned up");
}

/// R3-21: a microphone that delivers only digital silence for 15 s (muted, or access revoked
/// while the stream stays open) raises one `DigitalSilence` event; recording goes on; sound
/// returning re-arms it; system audio (silent when nothing plays) never warns.
#[test]
fn r3_21_a_mic_stuck_on_digital_silence_is_reported_once() {
    let rig = Rig::new();
    let mut cfg = rig.cfg(0);
    cfg.signal = FakeSignal::Silence;
    let (src, h) = FakeAudioSource::new(cfg);
    let mut rec = MicRecorder::with_factory(no_denoise(), factory_of(src));
    rec.start(rig.ctx()).unwrap();
    h.advance(10.0);
    rec.sync().unwrap();
    assert!(rec.drain_events().is_empty(), "not yet");
    h.advance(10.0);
    rec.sync().unwrap();
    let ev = rec.drain_events();
    assert_eq!(
        ev.iter()
            .filter(|e| matches!(e, AudioEvent::DigitalSilence { .. }))
            .count(),
        1,
        "{ev:?}"
    );
    h.advance(10.0);
    rec.sync().unwrap();
    assert!(rec.drain_events().is_empty(), "once per episode");
    let info = rec.stop(rig.at(&h)).unwrap();
    assert!(info.details["truncated_at"].is_null());

    let rig2 = Rig::new();
    let mut cfg = rig2.cfg(0);
    cfg.signal = FakeSignal::Silence;
    let (src, h2) = FakeAudioSource::new(cfg);
    let mut sys = SystemAudioRecorder::with_factory(SystemAudioConfig::default(), factory_of(src));
    sys.start(rig2.ctx()).unwrap();
    h2.advance(20.0);
    sys.sync().unwrap();
    assert!(
        sys.drain_events().is_empty(),
        "silent system audio is normal"
    );
}
