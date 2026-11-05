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

/// Writes a mono 48 kHz 16-bit sine of `secs` seconds at half scale.
#[cfg(windows)]
fn write_tone(path: &std::path::Path, hz: f64, secs: f64) {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: 48_000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    for i in 0..(48_000.0 * secs) as usize {
        let s = (2.0 * std::f64::consts::PI * hz * i as f64 / 48_000.0).sin() * 0.5;
        w.write_sample((s * f64::from(i16::MAX)) as i16).unwrap();
    }
    w.finalize().unwrap();
}

/// Level of one frequency in a recording, in dBFS of a full-scale sine (Goertzel over the left channel,
/// the whole file).
#[cfg(windows)]
fn tone_level_dbfs(path: &std::path::Path, hz: f64) -> f64 {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let ch = usize::from(spec.channels);
    let samples: Vec<f64> = match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .step_by(ch)
            .map(|s| f64::from(s.unwrap()))
            .collect(),
        hound::SampleFormat::Int => r
            .samples::<i32>()
            .step_by(ch)
            .map(|s| f64::from(s.unwrap()) / f64::from(1i32 << (spec.bits_per_sample - 1)))
            .collect(),
    };
    let w = 2.0 * std::f64::consts::PI * hz / f64::from(spec.sample_rate);
    let coeff = 2.0 * w.cos();
    let (mut s1, mut s2) = (0.0f64, 0.0f64);
    for x in &samples {
        let s0 = x + coeff * s1 - s2;
        s2 = s1;
        s1 = s0;
    }
    let power = s1 * s1 + s2 * s2 - coeff * s1 * s2;
    // Amplitude of the component: 2 * |X| / N; a full-scale sine is amplitude 1.
    let amp = 2.0 * power.max(0.0).sqrt() / samples.len().max(1) as f64;
    20.0 * amp.max(1e-9).log10()
}

/// First and last time (seconds from the start of the file) at which the left channel is above 5 % of full
/// scale, and the length between them.
#[cfg(windows)]
fn audible_span(path: &std::path::Path) -> Option<(f64, f64, f64)> {
    let mut r = hound::WavReader::open(path).unwrap();
    let spec = r.spec();
    let ch = usize::from(spec.channels);
    let rate = f64::from(spec.sample_rate);
    let loud: Vec<usize> = match spec.sample_format {
        hound::SampleFormat::Float => r
            .samples::<f32>()
            .step_by(ch)
            .enumerate()
            .filter(|(_, s)| s.as_ref().is_ok_and(|v| v.abs() > 0.05))
            .map(|(i, _)| i)
            .collect(),
        hound::SampleFormat::Int => {
            let full = f64::from(1i32 << (spec.bits_per_sample - 1));
            r.samples::<i32>()
                .step_by(ch)
                .enumerate()
                .filter(|(_, s)| s.as_ref().is_ok_and(|v| f64::from(*v).abs() / full > 0.05))
                .map(|(i, _)| i)
                .collect()
        }
    };
    let (first, last) = (*loud.first()?, *loud.last()?);
    Some((
        first as f64 / rate,
        last as f64 / rate,
        (last - first) as f64 / rate,
    ))
}

/// Windows: the process loopback leaves out the sound of CaptureMann's own process tree and keeps
/// everything else. The test records with the product's system-audio source while (a) a CHILD of this
/// process (PowerShell `SoundPlayer`) plays a 440 Hz tone, which must be absent, and (b) an UNRELATED
/// process (a Task Scheduler task, not a descendant) plays a 1000 Hz tone, which must be present. The
/// two tones are told apart by frequency, so no timing is needed. Needs audible speakers: the output must
/// not be muted (set the volume low).
#[cfg(windows)]
#[test]
#[ignore = "plays two 2 s tones through the speakers (one from a child process, one from a scheduled task)"]
fn windows_process_loopback_excludes_own_children() {
    use std::process::Command;

    struct TaskGuard(&'static str);
    impl Drop for TaskGuard {
        fn drop(&mut self) {
            let _ = Command::new("schtasks")
                .args(["/delete", "/tn", self.0, "/f"])
                .output();
        }
    }

    let dir = tempfile::tempdir().unwrap();
    let rec_dir = dir.path().join("rec");
    std::fs::create_dir_all(&rec_dir).unwrap();
    let tone_a = dir.path().join("own-440.wav");
    let tone_b = dir.path().join("other-1000.wav");
    write_tone(&tone_a, 440.0, 2.0);
    write_tone(&tone_b, 1000.0, 2.0);
    let done = dir.path().join("other-done.txt");
    let script = dir.path().join("other.ps1");
    std::fs::write(
        &script,
        format!(
            "(New-Object Media.SoundPlayer '{}').PlaySync(); Set-Content -Path '{}' -Value done",
            tone_b.display(),
            done.display()
        ),
    )
    .unwrap();

    let clock = SessionClock::with_host(Arc::new(SystemHostClock));
    let mut rec = SystemAudioRecorder::new(SystemAudioConfig::default());
    rec.start(TrackContext {
        dir: rec_dir.clone(),
        clock: clock.clone(),
    })
    .unwrap();
    std::thread::sleep(Duration::from_secs(1));

    // (a) a child of this process plays tone A.
    let st = Command::new("powershell")
        .args(["-NoProfile", "-Command"])
        .arg(format!(
            "(New-Object Media.SoundPlayer '{}').PlaySync()",
            tone_a.display()
        ))
        .status()
        .unwrap();
    assert!(st.success(), "the child player failed: {st}");
    std::thread::sleep(Duration::from_millis(500));

    // (b) an unrelated process (Task Scheduler, interactive for this user) plays tone B.
    let name = "cm-own-sound-test";
    let _guard = TaskGuard(name);
    let tr = format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    );
    let created = Command::new("schtasks")
        .args([
            "/create", "/tn", name, "/tr", &tr, "/sc", "once", "/st", "00:00", "/it", "/f",
        ])
        .output()
        .unwrap();
    assert!(
        created.status.success(),
        "schtasks /create: {}",
        String::from_utf8_lossy(&created.stderr)
    );
    let run = Command::new("schtasks")
        .args(["/run", "/tn", name])
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "schtasks /run: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let started = std::time::Instant::now();
    while !done.exists() && started.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(250));
    }
    assert!(done.exists(), "the unrelated player did not finish in 30 s");
    std::thread::sleep(Duration::from_millis(500));

    let info = rec.stop(clock.now()).unwrap();
    let info_text = serde_json::to_string_pretty(&info).unwrap();
    eprintln!("{info_text}");
    assert!(
        info_text.contains("process-loopback"),
        "the recorder did not use the process loopback (so there is nothing to check)"
    );
    let wav = rec_dir.join("system.wav");
    let own = tone_level_dbfs(&wav, 440.0);
    let other = tone_level_dbfs(&wav, 1000.0);
    if let Some((first, last, span)) = audible_span(&wav) {
        eprintln!(
            "audible from {first:.3} s to {last:.3} s = {:.0} ms (tone B is 2000 ms long; a shorter span means its start or end was lost)",
            span * 1000.0
        );
    }
    eprintln!("440 Hz (child of this process): {own:.1} dBFS; 1000 Hz (unrelated process): {other:.1} dBFS");
    assert!(
        other > -45.0,
        "the unrelated process's tone is missing: {other:.1} dBFS (is the output muted?)"
    );
    assert!(
        own < other - 40.0,
        "the child's tone is in the recording: {own:.1} dBFS against {other:.1} dBFS"
    );
}
