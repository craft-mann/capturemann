//! System audio check: plays a short chime from another process and reports whether system audio
//! capture heard it.
//!
//! CaptureMann's own process audio is always excluded from system capture (see `sys::mac_sck`), so
//! the chime has to come from a separate process: `/usr/bin/afplay` on a temporary WAV. The check
//! starts the real system capture, measures the level for a moment (the baseline), plays the chime
//! and watches the peak. Everything it creates (the WAV, the recorder's scratch files) lives in one
//! temporary folder that is removed in every outcome; the player is killed if still running and the
//! capture is stopped.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cm_core::{CmError, SessionClock, TrackContext, TrackRecorder};

use crate::level::{LevelHandle, FLOOR_DBFS};
use crate::source::SourceFactory;
use crate::system::{SystemAudioConfig, SystemAudioRecorder};

const CHIME_RATE: u32 = 48_000;
/// Peak of the chime in dBFS.
const CHIME_PEAK_DBFS: f64 = -12.0;
/// The macOS command line player.
/// The baseline takes at least this many level readings.
const MIN_BASELINE_READS: u32 = 4;

const AFPLAY: &str = "/usr/bin/afplay";

/// A 0.6 s two-note sine chime (A5 then E6), 48 kHz mono 16-bit PCM WAV, peak about -12 dBFS, each
/// note faded in and out so it does not click.
pub fn chime_wav() -> Vec<u8> {
    let notes = [(880.0f64, 0.3f64), (1318.51, 0.3)];
    let fade_s = 0.02;
    let amp = 10f64.powf(CHIME_PEAK_DBFS / 20.0) * f64::from(i16::MAX);
    let mut pcm: Vec<i16> = Vec::new();
    for (freq, len) in notes {
        let n = (len * f64::from(CHIME_RATE)).round() as usize;
        let fade = (fade_s * f64::from(CHIME_RATE)) as usize;
        for i in 0..n {
            let t = i as f64 / f64::from(CHIME_RATE);
            let edge = i.min(n - 1 - i);
            let env = if edge < fade {
                let x = edge as f64 / fade as f64;
                (x * std::f64::consts::FRAC_PI_2).sin().powi(2)
            } else {
                1.0
            };
            let v = amp * env * (2.0 * std::f64::consts::PI * freq * t).sin();
            pcm.push(v.round() as i16);
        }
    }
    let data_len = (pcm.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&CHIME_RATE.to_le_bytes());
    out.extend_from_slice(&(CHIME_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in pcm {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// A running (or finished) chime process.
pub trait ChimeProcess: Send {
    /// `Some(success)` once the process has ended, `None` while it runs.
    fn try_wait(&mut self) -> std::io::Result<Option<bool>>;
    /// Blocks until the process ends; `true` on success.
    fn wait(&mut self) -> std::io::Result<bool>;
    fn kill(&mut self);
}

/// Handle to a started chime. Dropping it kills a process that is still running.
pub struct ChimeHandle(Box<dyn ChimeProcess>);

impl ChimeHandle {
    pub fn new(p: Box<dyn ChimeProcess>) -> Self {
        ChimeHandle(p)
    }
    pub fn try_wait(&mut self) -> std::io::Result<Option<bool>> {
        self.0.try_wait()
    }
    pub fn wait(&mut self) -> std::io::Result<bool> {
        self.0.wait()
    }
    pub fn kill(&mut self) {
        self.0.kill();
    }
}

impl Drop for ChimeHandle {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            self.0.kill();
        }
    }
}

/// Plays a WAV file from a process other than this one.
pub trait ChimePlayer {
    fn play(&self, wav_path: &Path) -> Result<ChimeHandle, CmError>;
}

/// The real player: `/usr/bin/afplay <path>` (no shell, the path is its own argument).
#[derive(Debug, Clone, Copy, Default)]
pub struct AfplayPlayer;

struct AfplayProcess(Child);

impl ChimeProcess for AfplayProcess {
    fn try_wait(&mut self) -> std::io::Result<Option<bool>> {
        Ok(self.0.try_wait()?.map(|s| s.success()))
    }
    fn wait(&mut self) -> std::io::Result<bool> {
        Ok(self.0.wait()?.success())
    }
    fn kill(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

impl ChimePlayer for AfplayPlayer {
    fn play(&self, wav_path: &Path) -> Result<ChimeHandle, CmError> {
        let child = Command::new(AFPLAY)
            .arg(wav_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| CmError::internal(format!("cannot start {AFPLAY}: {e}")))?;
        Ok(ChimeHandle::new(Box::new(AfplayProcess(child))))
    }
}

/// What a [`FakeChimePlayer`] does.
pub enum FakeChimeScript {
    /// The process runs for `duration`, then exits with success. `on_play` runs when it starts (a
    /// test makes its fake audio device hear something).
    Plays {
        duration: Duration,
        on_play: Option<Box<dyn Fn() + Send + Sync>>,
    },
    /// `play` fails (the player is missing).
    SpawnFails(String),
    /// The process starts and ends at once with a failure status.
    ExitsFailing,
}

/// Scripted [`ChimePlayer`] for tests; it records what happened.
pub struct FakeChimePlayer {
    script: FakeChimeScript,
    /// Set when the process was killed while running.
    pub killed: Arc<AtomicBool>,
    /// Paths `play` was called with, and whether the file existed then.
    pub played: Arc<Mutex<Vec<(PathBuf, bool)>>>,
}

impl FakeChimePlayer {
    pub fn new(script: FakeChimeScript) -> Self {
        FakeChimePlayer {
            script,
            killed: Arc::new(AtomicBool::new(false)),
            played: Arc::default(),
        }
    }
}

struct FakeProcess {
    ends_at: Instant,
    success: bool,
    killed: Arc<AtomicBool>,
    done: bool,
}

impl ChimeProcess for FakeProcess {
    fn try_wait(&mut self) -> std::io::Result<Option<bool>> {
        if self.done || Instant::now() >= self.ends_at {
            Ok(Some(self.success))
        } else {
            Ok(None)
        }
    }
    fn wait(&mut self) -> std::io::Result<bool> {
        let left = self.ends_at.saturating_duration_since(Instant::now());
        std::thread::sleep(left);
        Ok(self.success)
    }
    fn kill(&mut self) {
        if !self.done {
            self.done = true;
            self.killed.store(true, Ordering::SeqCst);
        }
    }
}

impl ChimePlayer for FakeChimePlayer {
    fn play(&self, wav_path: &Path) -> Result<ChimeHandle, CmError> {
        if let Ok(mut p) = self.played.lock() {
            p.push((wav_path.to_path_buf(), wav_path.exists()));
        }
        let (duration, success) = match &self.script {
            FakeChimeScript::SpawnFails(m) => return Err(CmError::internal(m.clone())),
            FakeChimeScript::Plays { duration, on_play } => {
                if let Some(f) = on_play {
                    f();
                }
                (*duration, true)
            }
            FakeChimeScript::ExitsFailing => (Duration::ZERO, false),
        };
        Ok(ChimeHandle::new(Box::new(FakeProcess {
            ends_at: Instant::now() + duration,
            success,
            killed: self.killed.clone(),
            done: false,
        })))
    }
}

/// Timing and thresholds of the check.
#[derive(Debug, Clone)]
pub struct ChimeConfig {
    /// Whole check, from the start of the baseline to the verdict.
    pub total: Duration,
    /// Level is measured this long before the chime starts.
    pub baseline: Duration,
    /// The chime counts as heard above this peak (dBFS) ...
    pub floor_dbfs: f32,
    /// ... and this many dB above the baseline peak (music already playing).
    pub margin_db: f32,
    /// Time between level readings (about 30 Hz).
    pub poll: Duration,
}

impl Default for ChimeConfig {
    fn default() -> Self {
        ChimeConfig {
            total: Duration::from_millis(2500),
            baseline: Duration::from_millis(300),
            floor_dbfs: -50.0,
            margin_db: 6.0,
            poll: Duration::from_millis(33),
        }
    }
}

/// Result of [`run_check`].
#[derive(Debug, Clone, PartialEq)]
pub enum ChimeOutcome {
    /// Capture heard the chime; the loudest peak seen while it played.
    Heard { peak_dbfs: f32 },
    /// The chime played and capture stayed quiet.
    NotHeard,
    /// Screen Recording permission (system audio) is missing.
    NoPermission,
    /// The player or the capture failed for another reason.
    Failed(String),
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A private temp folder, removed on drop.
struct ScratchDir(PathBuf);

impl ScratchDir {
    fn create() -> std::io::Result<ScratchDir> {
        let base = std::env::temp_dir();
        for _ in 0..16 {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = base.join(format!(
                "capturemann-chime-{}-{nanos:x}-{n}",
                std::process::id()
            ));
            match std::fs::create_dir(&dir) {
                Ok(()) => return Ok(ScratchDir(dir)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("no free temp name"))
    }
}

impl Drop for ScratchDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sleep_until(t: Instant) {
    let now = Instant::now();
    if t > now {
        std::thread::sleep(t - now);
    }
}

/// Runs the check once. `factory` opens the system audio source (use
/// [`crate::system_audio_factory`]); `on_level` gets the current peak in dBFS about 30 times a
/// second, from the start of the baseline on. The chime WAV is written to a unique temp folder
/// that is deleted before this returns.
pub fn run_check(
    player: &dyn ChimePlayer,
    factory: SourceFactory,
    cfg: &ChimeConfig,
    on_level: &mut dyn FnMut(f32),
) -> ChimeOutcome {
    let scratch = match ScratchDir::create() {
        Ok(s) => s,
        Err(e) => return ChimeOutcome::Failed(format!("cannot create a temp folder: {e}")),
    };
    let wav_path = scratch.0.join("chime.wav");
    let written = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&wav_path)
        .and_then(|mut f| f.write_all(&chime_wav()));
    if let Err(e) = written {
        return ChimeOutcome::Failed(format!("cannot write the chime: {e}"));
    }

    let mut rec = SystemAudioRecorder::with_factory(SystemAudioConfig::default(), factory);
    let level = rec.level_handle();
    let clock = SessionClock::start();
    let ctx = TrackContext {
        dir: scratch.0.clone(),
        clock: clock.clone(),
    };
    if let Err(e) = rec.start(ctx) {
        return match e {
            CmError::PermissionMissing { .. } => ChimeOutcome::NoPermission,
            other => ChimeOutcome::Failed(other.to_string()),
        };
    }
    let outcome = watch(player, &wav_path, &level, cfg, on_level);
    let _ = rec.stop(clock.now());
    drop(scratch);
    outcome
}

fn watch(
    player: &dyn ChimePlayer,
    wav_path: &Path,
    level: &LevelHandle,
    cfg: &ChimeConfig,
    on_level: &mut dyn FnMut(f32),
) -> ChimeOutcome {
    let start = Instant::now();
    let deadline = start + cfg.total;
    let mut next = start;

    // Baseline: whatever the computer is already playing. At least `MIN_BASELINE_READS` readings,
    // even when the time is up first (a stalled thread or a slow machine must not leave the
    // baseline empty: steady music would then pass for the chime).
    let mut baseline = FLOOR_DBFS;
    let mut reads = 0;
    while (Instant::now() < start + cfg.baseline.min(cfg.total) || reads < MIN_BASELINE_READS)
        && Instant::now() < deadline
    {
        let p = level.read().peak_dbfs;
        baseline = baseline.max(p);
        on_level(p);
        reads += 1;
        next += cfg.poll;
        sleep_until(next);
    }
    let threshold = cfg.floor_dbfs.max(baseline + cfg.margin_db);

    let mut handle = match player.play(wav_path) {
        Ok(h) => h,
        Err(e) => return ChimeOutcome::Failed(e.to_string()),
    };
    let mut peak = FLOOR_DBFS;
    loop {
        let p = level.read().peak_dbfs;
        peak = peak.max(p);
        on_level(p);
        match handle.try_wait() {
            Ok(Some(false)) => return ChimeOutcome::Failed("the chime player failed".into()),
            // Heard and the chime is over: no need to wait for the deadline.
            Ok(Some(true)) if peak > threshold => break,
            Ok(_) => {}
            Err(e) => return ChimeOutcome::Failed(format!("the chime player: {e}")),
        }
        if Instant::now() >= deadline {
            break;
        }
        next += cfg.poll;
        sleep_until(next.min(deadline));
    }
    handle.kill();
    if peak > threshold {
        ChimeOutcome::Heard { peak_dbfs: peak }
    } else {
        ChimeOutcome::NotHeard
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeAudioSource, FakeConfig, FakeHandle, FakeSignal};
    use crate::source::AudioSource;

    fn chime_test_cfg() -> ChimeConfig {
        ChimeConfig {
            total: Duration::from_millis(900),
            baseline: Duration::from_millis(200),
            ..ChimeConfig::default()
        }
    }

    /// A fake system device plus a thread that delivers its audio in real time.
    struct ChimeTestDevice {
        handle: FakeHandle,
        stop: Arc<AtomicBool>,
        feeder: Option<std::thread::JoinHandle<()>>,
    }

    fn chime_test_device(signal: FakeSignal) -> (SourceFactory, ChimeTestDevice) {
        let (src, handle) = FakeAudioSource::new(FakeConfig {
            channels: 2,
            signal,
            ..FakeConfig::default()
        });
        let mut src = Some(src);
        let factory: SourceFactory = Box::new(move |_| {
            src.take()
                .map(|s| Box::new(s) as Box<dyn AudioSource>)
                .ok_or_else(|| CmError::internal("opened twice"))
        });
        let stop = Arc::new(AtomicBool::new(false));
        let (h, s) = (handle.clone(), stop.clone());
        let feeder = std::thread::spawn(move || {
            while !s.load(Ordering::SeqCst) {
                h.advance(0.02);
                std::thread::sleep(Duration::from_millis(20));
            }
        });
        (
            factory,
            ChimeTestDevice {
                handle,
                stop,
                feeder: Some(feeder),
            },
        )
    }

    impl Drop for ChimeTestDevice {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            if let Some(f) = self.feeder.take() {
                let _ = f.join();
            }
        }
    }

    fn chime_test_loud() -> FakeSignal {
        FakeSignal::Sine {
            freq_hz: 880.0,
            amp: 0.25,
        }
    }

    fn chime_test_check(
        player: &FakeChimePlayer,
        factory: SourceFactory,
    ) -> (ChimeOutcome, Vec<f32>) {
        let mut seen = Vec::new();
        let out = run_check(player, factory, &chime_test_cfg(), &mut |p| seen.push(p));
        (out, seen)
    }

    fn chime_test_gone(player: &FakeChimePlayer) {
        for (path, _) in player.played.lock().unwrap().iter() {
            assert!(!path.exists(), "wav left behind");
            assert!(!path.parent().unwrap().exists(), "temp folder left behind");
        }
    }

    #[test]
    fn chime_wav_header_length_and_peak() {
        let w = chime_wav();
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u16::from_le_bytes([w[20], w[21]]), 1, "PCM");
        assert_eq!(u16::from_le_bytes([w[22], w[23]]), 1, "mono");
        assert_eq!(u32::from_le_bytes([w[24], w[25], w[26], w[27]]), 48_000);
        assert_eq!(u16::from_le_bytes([w[34], w[35]]), 16);
        assert_eq!(&w[36..40], b"data");
        let data_len = u32::from_le_bytes([w[40], w[41], w[42], w[43]]) as usize;
        assert_eq!(data_len, 28_800 * 2, "0.6 s");
        assert_eq!(w.len(), 44 + data_len);
        assert_eq!(
            u32::from_le_bytes([w[4], w[5], w[6], w[7]]) as usize,
            w.len() - 8
        );
        let pcm: Vec<i16> = w[44..]
            .chunks(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect();
        let peak = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap();
        let db = 20.0 * (f64::from(peak) / 32767.0).log10();
        assert!((db + 12.0).abs() < 0.5, "peak {db} dBFS");
        // Faded: starts and ends silent.
        assert!(pcm[0].abs() < 100 && pcm[pcm.len() - 1].abs() < 100);
    }

    #[test]
    fn heard_when_the_device_gets_loud_after_the_chime_starts() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        let h = dev.handle.clone();
        let player = FakeChimePlayer::new(FakeChimeScript::Plays {
            duration: Duration::from_millis(300),
            on_play: Some(Box::new(move || h.set_signal(chime_test_loud()))),
        });
        let (out, seen) = chime_test_check(&player, factory);
        match out {
            ChimeOutcome::Heard { peak_dbfs } => assert!(peak_dbfs > -20.0, "{peak_dbfs}"),
            other => panic!("{other:?}"),
        }
        assert!(seen.len() > 5, "progress callback ran: {}", seen.len());
        assert!(!dev.handle.is_started(), "capture stopped");
        assert_eq!(player.played.lock().unwrap().len(), 1);
        assert!(player.played.lock().unwrap()[0].1, "wav existed at play");
        chime_test_gone(&player);
    }

    #[test]
    fn not_heard_on_silence_and_the_player_is_killed_at_timeout() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        let player = FakeChimePlayer::new(FakeChimeScript::Plays {
            duration: Duration::from_secs(60),
            on_play: None,
        });
        let t = Instant::now();
        let (out, _) = chime_test_check(&player, factory);
        assert_eq!(out, ChimeOutcome::NotHeard);
        assert!(t.elapsed() < Duration::from_millis(2500), "timeout kept");
        assert!(player.killed.load(Ordering::SeqCst));
        assert!(!dev.handle.is_started());
        chime_test_gone(&player);
    }

    /// Steady loud audio (music already playing) is part of the baseline, not the chime. The device
    /// is driven from the level callback, one poll of audio per reading, so the result does not
    /// depend on how a loaded machine schedules a feeder thread.
    #[test]
    fn steady_loud_audio_is_not_mistaken_for_the_chime() {
        let (factory, mut dev) = chime_test_device(chime_test_loud());
        dev.stop.store(true, Ordering::SeqCst);
        dev.feeder.take().unwrap().join().unwrap();
        let player = FakeChimePlayer::new(FakeChimeScript::Plays {
            duration: Duration::from_millis(200),
            on_play: None,
        });
        let handle = dev.handle.clone();
        let out = run_check(&player, factory, &chime_test_cfg(), &mut |_| {
            handle.advance(0.033);
            // The recorder's worker turns the chunk into a level reading.
            std::thread::sleep(Duration::from_millis(10));
        });
        assert_eq!(out, ChimeOutcome::NotHeard);
        chime_test_gone(&player);
    }

    /// The baseline takes at least `MIN_BASELINE_READS` readings even when its time is already up.
    #[test]
    fn the_baseline_never_ends_before_a_few_readings() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        let player = FakeChimePlayer::new(FakeChimeScript::Plays {
            duration: Duration::from_millis(50),
            on_play: None,
        });
        let cfg = ChimeConfig {
            baseline: Duration::ZERO,
            total: Duration::from_millis(600),
            ..chime_test_cfg()
        };
        let mut before_play = 0;
        let _ = run_check(&player, factory, &cfg, &mut |_| {
            if player.played.lock().unwrap().is_empty() {
                before_play += 1;
            }
        });
        assert!(before_play >= MIN_BASELINE_READS as usize, "{before_play}");
        drop(dev);
    }

    #[test]
    fn permission_missing_maps_to_no_permission() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        dev.handle.fail_next_start(CmError::permission_missing(
            cm_core::Permission::Screen,
            "need screen recording",
        ));
        let player = FakeChimePlayer::new(FakeChimeScript::Plays {
            duration: Duration::from_millis(100),
            on_play: None,
        });
        let (out, _) = chime_test_check(&player, factory);
        assert_eq!(out, ChimeOutcome::NoPermission);
        assert!(player.played.lock().unwrap().is_empty(), "nothing played");
    }

    #[test]
    fn other_capture_errors_fail() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        dev.handle
            .fail_next_start(CmError::device_busy("audio device busy"));
        let player = FakeChimePlayer::new(FakeChimeScript::ExitsFailing);
        let (out, _) = chime_test_check(&player, factory);
        assert!(matches!(out, ChimeOutcome::Failed(m) if m.contains("busy")));
    }

    #[test]
    fn missing_afplay_fails_and_cleans_up() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        let player = FakeChimePlayer::new(FakeChimeScript::SpawnFails("no afplay".into()));
        let (out, _) = chime_test_check(&player, factory);
        assert!(matches!(out, ChimeOutcome::Failed(m) if m.contains("no afplay")));
        assert!(!dev.handle.is_started(), "capture stopped");
        chime_test_gone(&player);
    }

    #[test]
    fn failing_afplay_fails_and_cleans_up() {
        let (factory, dev) = chime_test_device(FakeSignal::Silence);
        let player = FakeChimePlayer::new(FakeChimeScript::ExitsFailing);
        let (out, _) = chime_test_check(&player, factory);
        assert!(matches!(out, ChimeOutcome::Failed(_)), "{out:?}");
        assert!(!dev.handle.is_started());
        chime_test_gone(&player);
    }
}
