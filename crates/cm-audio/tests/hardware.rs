//! Real-hardware checks. All `#[ignore]`: run by hand with
//! `cargo test -p cm-audio --test hardware -- --ignored --nocapture --test-threads=1`
//! (see tests/manual/AUD-1.md at the repository root). They never prompt for permission: if the
//! state is not "authorized" they print it and stop. Recorded audio goes to a temp dir that is
//! deleted when the test ends.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;
use std::time::Duration;

use cm_audio::{
    list_audio_devices, microphone_permission, screen_recording_permission, MicConfig, MicMonitor,
    MicRecorder, PermissionState, SystemAudioConfig, SystemAudioRecorder,
};
use cm_core::{SessionClock, SystemHostClock, TrackContext, TrackRecorder};

#[test]
#[ignore = "touches real audio hardware (lists devices only; no recording)"]
fn lists_devices_and_permission_state() {
    let d = list_audio_devices();
    eprintln!("{}", serde_json::to_string_pretty(&d).unwrap());
    assert!(d.mics.iter().filter(|m| m.default).count() <= 1);
}

#[test]
#[ignore = "records the real microphone for 3 s"]
fn mic_records_three_seconds_and_stamps_with_the_os_clock() {
    let state = microphone_permission();
    if state != PermissionState::Authorized {
        eprintln!("microphone permission is {state:?}; not recording. Grant it, then rerun.");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::start();
    let mut rec = MicRecorder::new(MicConfig::default());
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    let level = rec.level_handle();
    let t_start = clock.now();
    std::thread::sleep(Duration::from_secs(3));
    let live = level.read();
    let info = rec.stop(clock.now()).unwrap();
    eprintln!("device: {:?}", rec.device_format());
    eprintln!(
        "started at session {:?} (call at {:?})",
        info.start, t_start
    );
    eprintln!("level at 3 s: {live:?}");
    eprintln!("{}", serde_json::to_string_pretty(&info).unwrap());
    let raw = hound::WavReader::open(dir.path().join("mic.wav")).unwrap();
    let secs = f64::from(raw.duration()) / 48_000.0;
    assert!((2.7..=3.3).contains(&secs), "{secs} s");
    let den = hound::WavReader::open(dir.path().join("mic.denoised.wav")).unwrap();
    assert_eq!(den.duration(), raw.duration());
    // The first buffer is stamped no earlier than the call and not absurdly late.
    assert!(info.start.as_secs_f64() < 1.0, "{:?}", info.start);
}

#[test]
#[ignore = "opens the real microphone for a meter reading (no files)"]
fn monitor_shows_a_level() {
    if microphone_permission() != PermissionState::Authorized {
        eprintln!(
            "microphone permission is {:?}; skipping",
            microphone_permission()
        );
        return;
    }
    let m = MicMonitor::start(&Default::default()).unwrap();
    std::thread::sleep(Duration::from_millis(600));
    eprintln!("{:?} {:?}", m.format(), m.level_handle().read());
}

#[test]
#[ignore = "plays a 0.6 s chime through the speakers and listens to system audio"]
fn system_audio_hears_a_chime_played_by_another_process() {
    // AUD-2 check, both systems: afplay on macOS, PowerShell on Windows (WASAPI loopback).
    let state = screen_recording_permission();
    if state != PermissionState::Authorized {
        eprintln!("SKIPPED: screen recording permission is {state:?}");
        return;
    }
    let outcome = cm_audio::run_check(
        cm_audio::system_chime_player().as_ref(),
        cm_audio::system_audio_check_factory(),
        &cm_audio::ChimeConfig::default(),
        &mut |peak| eprintln!("peak {peak:.1} dBFS"),
    );
    eprintln!("{outcome:?}");
    assert!(
        matches!(outcome, cm_audio::ChimeOutcome::Heard { .. }),
        "{outcome:?} (unmute the speakers and play nothing else)"
    );
}

#[test]
#[ignore = "records whatever the computer plays for 4 s (play a tone yourself)"]
fn system_audio_records_four_seconds() {
    let state = screen_recording_permission();
    if state != PermissionState::Authorized {
        eprintln!("screen recording permission is {state:?}; not recording.");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(SystemHostClock));
    let mut rec = SystemAudioRecorder::new(SystemAudioConfig::default());
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(4));
    let level = rec.level_handle().read();
    let info = rec.stop(clock.now()).unwrap();
    eprintln!("level: {level:?}");
    eprintln!("{}", serde_json::to_string_pretty(&info).unwrap());
    let r = hound::WavReader::open(dir.path().join("system.wav")).unwrap();
    assert_eq!((r.spec().channels, r.spec().sample_rate), (2, 48_000));
    assert!((3.5..=4.5).contains(&(f64::from(r.duration()) / 48_000.0)));
}

#[test]
#[ignore = "touches real audio hardware (opens devices, starts no stream, records nothing)"]
fn devices_open_without_streaming() {
    let mic = cm_audio::CpalSource::open_input(&cm_audio::DeviceSelector::Default);
    match &mic {
        Ok(m) => eprintln!("default mic: {:?}", cm_audio::AudioSource::format(m)),
        Err(e) => eprintln!("default mic: {e}"),
    }
    #[cfg(target_os = "macos")]
    {
        let factory = cm_audio::system_audio_check_factory();
        let mut factory = factory;
        match factory(&cm_audio::DeviceSelector::Default) {
            Ok(s) => eprintln!("system audio source: {:?}", s.format()),
            Err(e) => eprintln!("system audio source: {e}"),
        }
    }
}

/// Windows: the process loopback (system audio without CaptureMann's own sound) opens, runs for 3 s and
/// stops. Plays nothing and needs no sound: a silent computer gives an empty or short track, which is
/// printed, not asserted. Not named after the other system-audio tests so that a filter for them never
/// picks this one up.
#[cfg(windows)]
#[test]
#[ignore = "opens the Windows process loopback for 3 s (no sound is played)"]
fn windows_process_loopback_opens_and_stops() {
    let dir = tempfile::tempdir().unwrap();
    let clock = SessionClock::with_host(Arc::new(SystemHostClock));
    let mut rec = SystemAudioRecorder::new(SystemAudioConfig::default());
    rec.start(TrackContext {
        dir: dir.path().to_path_buf(),
        clock: clock.clone(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let info = rec.stop(clock.now()).unwrap();
    eprintln!("{}", serde_json::to_string_pretty(&info).unwrap());
    let r = hound::WavReader::open(dir.path().join("system.wav")).unwrap();
    eprintln!(
        "system.wav: {} ch, {} Hz, {:.2} s",
        r.spec().channels,
        r.spec().sample_rate,
        f64::from(r.duration()) / f64::from(r.spec().sample_rate)
    );
}
