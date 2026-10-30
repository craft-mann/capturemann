//! [`RecordingSession`]: every recorder of M3 wired together the way the daemon's recording
//! actions will drive them (slice 4 adds only the actions, the HUD events and the confirmation on
//! top). No IPC, no daemon state: plain synchronous calls that block while hardware starts and
//! stops, so a daemon calls them from `spawn_blocking`.
//!
//! # Life cycle
//!
//! ```text
//! begin ──► Recording ◄──► Paused ──► stop ──► StoppedSession ──► finalize / finish_into(Library)
//!              │ tick()  (events, disk guard)                  └► discard (HUD "Discard")
//! ```
//!
//! # Clock origin (decision)
//!
//! Every recorder takes the [`SessionClock`] in its `TrackContext` when it starts and drops
//! samples stamped before the origin (`before_origin` counters in the screen, camera and audio
//! pipelines). The REC-1 report measured the first screen frame 170 to 330 ms after the start
//! call, and the camera's `start` blocks until its first frame (up to 3 s). So:
//!
//! 1. The origin is taken **before** the tracks start (one clock, one origin, shared by all).
//!    Nothing a track captures is ever discarded for being "too early".
//! 2. All tracks start **concurrently** (one thread each), so the slowest start (camera, up to
//!    3 s) does not delay the others.
//! 3. Every sample keeps the OS capture stamp it was delivered with, so tracks stay aligned by
//!    timestamp no matter when each one actually began. The first sample of a track is at
//!    `TrackInfo::start`, not at 0.
//! 4. When the last `start` has returned the session notes `live_at`: the session time at which
//!    every track that started is delivering. `live_at` is stored in `session.json`
//!    (`session.live_at`) and in the manifest (`live_at`, `session.live_at`). Material between 0
//!    and `live_at` is pre-roll (the camera warming up while the screen already runs): the HUD
//!    clock and the editor's default in-point should start at `live_at`. Slice 4 should show the
//!    countdown before calling `begin`, so pre-roll is never visible content.
//!
//! Pre-roll (REC-4.AC1, R3-20): the manifest `duration` and the length of the media files include
//! the material before `live_at` (up to about 3 s with the camera or system audio), but the HUD
//! timer starts at `live_at`, so a recording the HUD shows as 10 s has a file of up to 13 s. The
//! files are not trimmed (that would lose the alignment between tracks); `live_at` is in
//! `session.json`, the manifest and the `recording.started` result so exporters and the editor
//! can start there, and the interim exporter (`cm-media`) already trims to the screen's first
//! frame.
//!
//! The alternative (create the clock after the starts) cannot work: recorders hold the clock from
//! `start`, and frames stamped before the origin come out negative and are dropped.
//!
//! # Pause
//!
//! One `at = clock.now()` is passed to every track, so all tracks agree on the gap. Each
//! recorder subtracts the gaps from file time (screen, camera, mic and system audio files have no
//! hole: file time = session time minus the gaps before it, REC-4). The one exception is the
//! cursor log, which stays in session time on purpose (its events are sparse and stamped by event
//! time, so there is nothing to keep continuous). The session's gap list in `manifest.gaps`
//! (session time) is what maps between file time and session time. Gaps are written to the bundle lock on every resume, so a crash
//! after a pause still recovers with the right mapping. Stopping while paused closes the final
//! gap at the stop time.
//!
//! # Partial failure
//!
//! The screen track is required: if it cannot start, nothing is recorded and the bundle is
//! removed. Camera, system audio, mic and cursor are optional: a failure to start, to pause, or
//! to stop is a warning (`manifest.warnings`, [`SessionNote`] to the caller), the track's files
//! are removed, and the recording continues. A recorder that reports it cannot continue (screen
//! window closed, disk full) raises [`NoteLevel::StopRequested`]; the owner then calls `stop` and
//! keeps what was captured.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cm_audio::{Level, LevelHandle};
use cm_capture::disk::{assess_disk, DiskStatus, FreeSpace, SystemFreeSpace};
use cm_core::{
    CmError, HostClock, MediaTime, PauseGap, Permission, SessionClock, SystemHostClock,
    TrackContext, TrackInfo, TrackKind, TrackStats,
};
use cm_library::index::default_title;
use cm_library::manifest::{Marker, RecManifest};
use cm_library::rec::{SessionDisplay, SessionInfo};
use cm_library::{Library, RecBundle};
use serde::Serialize;
use serde_json::{json, Map, Value};
use time::OffsetDateTime;

use super::guard;
use super::tracks::{NoteLevel, SessionTrack, TrackNote};

/// Where and how a session records.
pub struct SessionConfig {
    /// The library root (or any folder): the bundle `<title>.cmrec` is created inside it.
    pub parent: PathBuf,
    /// `None`: `Recording yyyy-MM-dd HH.mm`.
    pub title: Option<String>,
    /// Recording settings as used; stored in `manifest.settings`.
    pub settings: Value,
    pub displays: Vec<SessionDisplay>,
    pub app_version: String,
    /// The host clock the session clock runs on (tests inject a manual one).
    pub host: Arc<dyn HostClock>,
    /// Free-space probe for the disk guard.
    pub free_space: Arc<dyn FreeSpace>,
    /// How often `tick` re-checks the disk (REC-3: 5 s).
    pub disk_check_every: Duration,
    /// Wall time stamped on the bundle; `None` is now.
    pub now: Option<OffsetDateTime>,
    /// An audio-only recording (VOC-1): no screen, camera or cursor track, only the microphone
    /// and/or system audio; the manifest says `"kind": "audio"`.
    pub audio_only: bool,
}

impl SessionConfig {
    pub fn new(parent: impl Into<PathBuf>) -> Self {
        SessionConfig {
            parent: parent.into(),
            title: None,
            settings: Value::Null,
            displays: Vec::new(),
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            host: Arc::new(SystemHostClock),
            free_space: Arc::new(SystemFreeSpace),
            disk_check_every: Duration::from_secs(5),
            now: None,
            audio_only: false,
        }
    }
}

/// A warning or notice from a track or from the session itself.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionNote {
    /// `None` for the session (disk guard).
    pub track: Option<TrackKind>,
    pub level: NoteLevel,
    pub code: String,
    pub message: String,
    pub at: Option<MediaTime>,
    /// The OS permission a `permission_revoked` note is about.
    pub permission: Option<Permission>,
}

impl SessionNote {
    fn from_track(kind: TrackKind, n: TrackNote) -> Self {
        SessionNote {
            track: Some(kind),
            level: n.level,
            code: n.code,
            message: n.message,
            at: n.at,
            permission: n.permission,
        }
    }

    fn session(level: NoteLevel, code: &str, message: String) -> Self {
        SessionNote {
            track: None,
            level,
            code: code.to_owned(),
            message,
            at: None,
            permission: None,
        }
    }

    fn to_json(&self) -> Value {
        let mut v = json!({
            "track": self.track.map(TrackKind::as_str),
            "code": self.code,
            "message": self.message,
            "at": self.at.map(MediaTime::ticks),
        });
        if let Some(p) = self.permission {
            v["permission"] = json!(p.as_str());
        }
        v
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Recording,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", content = "reason", rename_all = "snake_case")]
pub enum TrackState {
    Running,
    /// Did not start, or was lost at pause/stop; its files were removed.
    Failed(String),
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct LevelReading {
    pub peak_dbfs: f32,
    pub rms_dbfs: f32,
    pub clip_events: u64,
}

impl From<Level> for LevelReading {
    fn from(l: Level) -> Self {
        LevelReading {
            peak_dbfs: l.peak_dbfs,
            rms_dbfs: l.rms_dbfs,
            clip_events: l.clip_events,
        }
    }
}

/// Live numbers of one track for the HUD.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TrackStatus {
    pub kind: TrackKind,
    #[serde(flatten)]
    pub state: TrackState,
    pub stats: TrackStats,
    pub level: Option<LevelReading>,
    /// Recorder specific (screen: late %, fragments, repeated frames).
    pub live: Value,
    /// How long `start` took for this track.
    pub start_ms: f64,
}

/// A span of session time in which the microphone was muted (AUD-6). `to` is `None` while it
/// still is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicMuteSpan {
    pub from: MediaTime,
    pub to: Option<MediaTime>,
}

/// What [`RecordingSession::set_mic_muted`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicMuteChange {
    /// `false` when the microphone already was in the asked state.
    pub changed: bool,
    /// Session time of the change (of the last one when nothing changed).
    pub at: MediaTime,
}

/// The manifest `mic_muted` list: `[{from, to}]` in session ticks; `to` is `null` only for a
/// span still open (a crash recovered the bundle while the mic was muted).
pub fn mic_muted_json(spans: &[MicMuteSpan]) -> Value {
    Value::Array(
        spans
            .iter()
            .map(|s| json!({ "from": s.from.ticks(), "to": s.to.map(MediaTime::ticks) }))
            .collect(),
    )
}

/// What the HUD shows (`get_recording_status`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SessionStatus {
    pub state: SessionState,
    /// Session time now (since the origin).
    pub elapsed: MediaTime,
    /// `elapsed` minus `live_at` minus time spent paused: the clock the HUD shows.
    pub recorded: MediaTime,
    pub live_at: MediaTime,
    pub paused_total: MediaTime,
    pub tracks: Vec<TrackStatus>,
    /// The microphone is muted now (AUD-6).
    pub mic_muted: bool,
    pub warnings: usize,
    pub disk_minutes_left: Option<f64>,
    pub stop_requested: Option<String>,
}

struct Slot {
    track: Box<dyn SessionTrack>,
    state: TrackState,
    start_ms: f64,
    /// The track's recovery hint was copied into the lock (R3-2).
    hint_saved: bool,
}

/// A running recording. See the module docs.
pub struct RecordingSession {
    clock: SessionClock,
    bundle: Option<RecBundle>,
    dir: PathBuf,
    capture_id: String,
    title: String,
    slots: Vec<Slot>,
    state: SessionState,
    paused_at: Option<MediaTime>,
    gaps: Vec<PauseGap>,
    /// Muted spans of the microphone, in session time (AUD-6).
    mic_muted: Vec<MicMuteSpan>,
    live_at: MediaTime,
    requested: Vec<TrackKind>,
    warnings: Vec<SessionNote>,
    stop_requested: Option<String>,
    /// Markers set so far (VOC-1), sorted by time; also kept in the bundle's lock.
    markers: Vec<Marker>,
    /// An audio-only recording: it has no required track, so it fails when none could be kept.
    audio_only: bool,
    free_space: Arc<dyn FreeSpace>,
    disk_every: Duration,
    last_disk_check: Option<Instant>,
    disk_low_sent: bool,
    disk_minutes: Option<f64>,
    bitrate_bps: u64,
    now_wall: OffsetDateTime,
}

fn track_err(kind: TrackKind, what: &str, e: &CmError) -> String {
    format!("{} track {what}: {e}", kind.as_str())
}

fn remove_files(dir: &std::path::Path, files: &[&str]) {
    for f in files {
        let p = dir.join(f);
        let _ = if p.is_dir() {
            fs::remove_dir_all(&p)
        } else {
            fs::remove_file(&p)
        };
    }
}

impl RecordingSession {
    /// Creates the bundle and starts every track. Returns when all starts have returned (the
    /// slowest, normally the camera, decides). See the module docs for the clock decision.
    pub fn begin(
        config: SessionConfig,
        tracks: Vec<Box<dyn SessionTrack>>,
    ) -> Result<Self, CmError> {
        let mut seen = std::collections::HashSet::new();
        for t in &tracks {
            if !seen.insert(t.kind()) {
                return Err(CmError::invalid_input(format!(
                    "Two {} tracks in one recording",
                    t.kind().as_str()
                )));
            }
        }
        if config.audio_only {
            if tracks
                .iter()
                .any(|t| !matches!(t.kind(), TrackKind::Mic | TrackKind::System))
            {
                return Err(CmError::invalid_input(
                    "An audio recording has no screen, camera or cursor track",
                ));
            }
            if tracks.is_empty() {
                return Err(CmError::invalid_input(
                    "An audio recording needs a microphone or system audio track",
                ));
            }
        } else if tracks
            .iter()
            .filter(|t| t.kind() == TrackKind::Screen)
            .count()
            != 1
        {
            return Err(CmError::invalid_input(
                "A recording needs exactly one screen track",
            ));
        }
        let bitrate_bps: u64 = tracks.iter().map(|t| t.bitrate_bps()).sum();
        if let Some(free) = config.free_space.free_bytes(&config.parent) {
            if let DiskStatus::Full { free_bytes } = assess_disk(free, bitrate_bps) {
                return Err(CmError::disk_full(format!(
                    "Only {} MB free; recording needs more than 1 GB free to start",
                    free_bytes / 1_000_000
                )));
            }
        }

        // The origin comes first: see the module docs.
        let clock = SessionClock::with_host(Arc::clone(&config.host));
        let now_wall = config.now.unwrap_or_else(OffsetDateTime::now_utc);
        let title = match &config.title {
            Some(t) => t.clone(),
            None => default_title(
                if config.audio_only {
                    "Audio recording"
                } else {
                    "Recording"
                },
                now_wall,
            )?,
        };
        let requested: Vec<TrackKind> = tracks.iter().map(|t| t.kind()).collect();
        let mut info = SessionInfo {
            clock_origin_ns: clock.origin_ns(),
            timebase: timebase_name().into(),
            displays: config.displays.clone(),
            devices: json!({}),
            app_version: config.app_version.clone(),
            extra: Map::new(),
        };
        let mut bundle = RecBundle::create(
            &config.parent,
            &title,
            now_wall,
            config.settings.clone(),
            &info,
        )?;
        let dir = bundle.dir().to_owned();
        let capture_id = bundle.id().to_owned();
        if config.audio_only {
            // In the lock from the first moment, so a recovered bundle is still an audio one.
            if let Err(e) = bundle.set_extra("kind", json!("audio")) {
                drop(bundle);
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        }

        let mut slots: Vec<Slot> = tracks
            .into_iter()
            .map(|track| Slot {
                track,
                state: TrackState::Running,
                start_ms: 0.0,
                hint_saved: false,
            })
            .collect();
        let ctx = TrackContext {
            dir: dir.clone(),
            clock: clock.clone(),
        };
        let results: Vec<(Result<(), CmError>, f64)> = std::thread::scope(|s| {
            let handles: Vec<_> = slots
                .iter_mut()
                .map(|slot| {
                    let ctx = ctx.clone();
                    s.spawn(move || {
                        let t0 = Instant::now();
                        let kind = slot.track.kind();
                        let r = guard::call(&format!("{kind:?} start"), || slot.track.start(ctx));
                        (r, t0.elapsed().as_secs_f64() * 1e3)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        (
                            Err(CmError::internal("a track panicked while starting")),
                            0.0,
                        )
                    })
                })
                .collect()
        });
        let live_at = clock.now();

        // A required track that failed: undo everything. An audio recording has no screen track
        // to be required: it fails when none of its tracks started.
        let failed_required = if config.audio_only {
            results
                .iter()
                .all(|(r, _)| r.is_err())
                .then_some(0)
                .filter(|_| !results.is_empty())
        } else {
            results
                .iter()
                .enumerate()
                .position(|(i, (r, _))| r.is_err() && slots[i].track.required())
        };
        if let Some(pos) = failed_required {
            for (i, slot) in slots.iter_mut().enumerate() {
                if results[i].0.is_ok() {
                    let _ = guard::call("stop", || slot.track.stop(live_at));
                }
            }
            drop(bundle);
            let _ = fs::remove_dir_all(&dir);
            return Err(results
                .into_iter()
                .nth(pos)
                .and_then(|(r, _)| r.err())
                .unwrap_or_else(|| CmError::internal("screen track failed to start")));
        }

        let mut warnings = Vec::new();
        let mut devices = Map::new();
        for (slot, (res, ms)) in slots.iter_mut().zip(results) {
            let kind = slot.track.kind();
            slot.start_ms = ms;
            match res {
                Ok(()) => {
                    let d = slot.track.device_info();
                    if !d.is_null() {
                        devices.insert(kind.as_str().to_owned(), d);
                    }
                    if let Some((k, v)) = slot.track.session_extra() {
                        info.extra.insert(k, v);
                    }
                    for n in slot.track.start_notes() {
                        warnings.push(SessionNote::from_track(kind, n));
                    }
                }
                Err(e) => {
                    remove_files(&dir, slot.track.files());
                    slot.state = TrackState::Failed(e.to_string());
                    warnings.push(SessionNote {
                        track: Some(kind),
                        level: NoteLevel::Warning,
                        code: format!("{}_failed", kind.as_str()),
                        message: track_err(kind, "could not start", &e),
                        at: Some(live_at),
                        permission: None,
                    });
                }
            }
        }
        let started: Vec<&str> = slots
            .iter()
            .filter(|s| s.state == TrackState::Running)
            .map(|s| s.track.kind().as_str())
            .collect();
        let start_ms: Map<String, Value> = slots
            .iter()
            .map(|s| (s.track.kind().as_str().to_owned(), json!(s.start_ms)))
            .collect();
        let session_json = json!({
            "origin_ns": clock.origin_ns(),
            "live_at": live_at.ticks(),
            "start_ms": start_ms,
            "requested_tracks": requested.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
            "started_tracks": started,
            "clock_policy": "origin before concurrent track starts; samples keep OS capture stamps; \
                             live_at is when the last start returned",
        });
        info.devices = Value::Object(devices);
        info.extra.insert("session".into(), session_json.clone());
        // R3-14: the tracks are running; if the bundle cannot take its metadata they must not be
        // left running with no owner.
        let meta: Result<(), CmError> = (|| {
            bundle.write_session(&info)?;
            bundle.set_extra("live_at", json!(live_at.ticks()))?;
            bundle.set_extra("session", session_json)?;
            for w in &warnings {
                bundle.add_warning(w.to_json())?;
            }
            Ok(())
        })();
        if let Err(e) = meta {
            let at = clock.now();
            for slot in slots.iter_mut().filter(|s| s.state == TrackState::Running) {
                let _ = guard::call("stop", || slot.track.stop(at));
            }
            drop(bundle);
            let _ = fs::remove_dir_all(&dir);
            return Err(e);
        }

        Ok(RecordingSession {
            clock,
            bundle: Some(bundle),
            dir,
            capture_id,
            title,
            slots,
            state: SessionState::Recording,
            paused_at: None,
            gaps: Vec::new(),
            mic_muted: Vec::new(),
            live_at,
            requested,
            warnings,
            stop_requested: None,
            markers: Vec::new(),
            audio_only: config.audio_only,
            free_space: config.free_space,
            disk_every: config.disk_check_every,
            last_disk_check: None,
            disk_low_sent: false,
            disk_minutes: None,
            bitrate_bps,
            now_wall,
        })
    }

    pub fn dir(&self) -> &std::path::Path {
        &self.dir
    }

    /// The id the recording will have in the library (`rec_...`), known from the moment the bundle
    /// is created.
    pub fn capture_id(&self) -> &str {
        &self.capture_id
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn clock(&self) -> &SessionClock {
        &self.clock
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Session time at which every started track was delivering.
    pub fn live_at(&self) -> MediaTime {
        self.live_at
    }

    pub fn gaps(&self) -> &[PauseGap] {
        &self.gaps
    }

    /// Everything noted so far (also stored in the manifest).
    pub fn warnings(&self) -> &[SessionNote] {
        &self.warnings
    }

    /// Set when a track or the disk guard asked for the recording to end.
    pub fn stop_requested(&self) -> Option<&str> {
        self.stop_requested.as_deref()
    }

    /// The live meter of a track (mic, system audio); valid for the whole recording.
    pub fn level_handle(&self, kind: TrackKind) -> Option<LevelHandle> {
        self.slots
            .iter()
            .find(|s| s.track.kind() == kind)
            .and_then(|s| s.track.level())
    }

    fn note(&mut self, n: SessionNote) {
        if n.level != NoteLevel::Info {
            if let Some(b) = self.bundle.as_mut() {
                let _ = b.add_warning(n.to_json());
            }
            self.warnings.push(n);
        }
    }

    fn paused_total(&self) -> MediaTime {
        let closed: i64 = self
            .gaps
            .iter()
            .map(|g| g.to.ticks() - g.from.ticks())
            .sum();
        let open = self
            .paused_at
            .map_or(0, |from| (self.clock.now() - from).ticks().max(0));
        MediaTime::from_ticks(closed + open)
    }

    /// Live status for the HUD. Cheap; call as often as the HUD refreshes.
    pub fn status(&self) -> SessionStatus {
        let elapsed = self.clock.now();
        let paused_total = self.paused_total();
        SessionStatus {
            state: self.state,
            elapsed,
            recorded: MediaTime::from_ticks(
                (elapsed.ticks() - self.live_at.ticks() - paused_total.ticks()).max(0),
            ),
            live_at: self.live_at,
            paused_total,
            tracks: self
                .slots
                .iter()
                .map(|s| TrackStatus {
                    kind: s.track.kind(),
                    state: s.state.clone(),
                    stats: s.track.stats(),
                    level: s.track.level().map(|l| l.read().into()),
                    live: s.track.live(),
                    start_ms: s.start_ms,
                })
                .collect(),
            mic_muted: self.is_mic_muted(),
            warnings: self.warnings.len(),
            disk_minutes_left: self.disk_minutes,
            stop_requested: self.stop_requested.clone(),
        }
    }

    /// Housekeeping the owner calls every 100 to 500 ms while recording: collects recorder
    /// events, runs the disk guard (every `disk_check_every`), and returns what is new. Warnings
    /// are also stored in the manifest. A [`NoteLevel::StopRequested`] note means the owner
    /// should call [`RecordingSession::stop`].
    pub fn tick(&mut self) -> Vec<SessionNote> {
        let mut out = Vec::new();
        if self.state == SessionState::Stopped {
            return out;
        }
        for slot in &mut self.slots {
            if slot.state != TrackState::Running {
                continue;
            }
            let kind = slot.track.kind();
            for n in slot.track.poll() {
                out.push(SessionNote::from_track(kind, n));
            }
            if !slot.hint_saved {
                if let Some(hint) = slot.track.recovery_hint() {
                    slot.hint_saved = true;
                    if let Some(b) = self.bundle.as_mut() {
                        let _ = b.set_track_hint(kind.as_str(), hint);
                    }
                }
            }
        }
        if self
            .last_disk_check
            .is_none_or(|t| t.elapsed() >= self.disk_every)
        {
            self.last_disk_check = Some(Instant::now());
            if let Some(free) = self.free_space.free_bytes(&self.dir) {
                match assess_disk(free, self.bitrate_bps) {
                    DiskStatus::Full { free_bytes } => out.push(SessionNote::session(
                        NoteLevel::StopRequested,
                        "disk_full",
                        format!("Recording stopped: only {} MB free", free_bytes / 1_000_000),
                    )),
                    DiskStatus::Low { minutes_left } => {
                        self.disk_minutes = Some(minutes_left);
                        if !self.disk_low_sent {
                            self.disk_low_sent = true;
                            out.push(SessionNote::session(
                                NoteLevel::Warning,
                                "disk_low",
                                format!("About {minutes_left:.0} minutes of disk space left"),
                            ));
                        }
                    }
                    DiskStatus::Ok { minutes_left } => {
                        self.disk_minutes = Some(minutes_left);
                        self.disk_low_sent = false;
                    }
                }
            }
        }
        // The screen recorder runs its own disk check too: report each code once. The disk
        // codes dedupe by code alone (the screen's and the session's carry different tracks, R3-15).
        out.retain(|n| {
            !(n.level != NoteLevel::Info
                && self.warnings.iter().any(|w| {
                    w.code == n.code
                        && (w.track == n.track
                            || matches!(n.code.as_str(), "disk_low" | "disk_full"))
                }))
        });
        let mut seen_disk: Vec<String> = Vec::new();
        out.retain(|n| {
            if matches!(n.code.as_str(), "disk_low" | "disk_full") {
                if seen_disk.contains(&n.code) {
                    return false;
                }
                seen_disk.push(n.code.clone());
            }
            true
        });
        for n in &out {
            if n.level == NoteLevel::StopRequested && self.stop_requested.is_none() {
                self.stop_requested = Some(n.code.clone());
            }
            self.note(n.clone());
        }
        out
    }

    /// The kinds of the tracks that are still running (what a permission poll must cover).
    pub fn running_kinds(&self) -> Vec<TrackKind> {
        self.slots
            .iter()
            .filter(|s| s.state == TrackState::Running)
            .map(|s| s.track.kind())
            .collect()
    }

    /// The OS withdrew `permission` (PRM-1), as seen by a poll of the permission state rather
    /// than by the track itself. Raises the same `permission_revoked` note a track would, once
    /// per track: Screen Recording asks the owner to stop (the required track cannot go on),
    /// Camera and Microphone only warn. `None` when no running track needs that permission or
    /// it was already reported.
    pub fn report_permission_revoked(&mut self, permission: Permission) -> Option<SessionNote> {
        let kind = match permission {
            Permission::Screen => TrackKind::Screen,
            Permission::Camera => TrackKind::Camera,
            Permission::Microphone => TrackKind::Mic,
            // Accessibility and Input Monitoring degrade the cursor track; they never stop a
            // recording (PRM-1) and are not polled.
            Permission::Accessibility | Permission::InputMonitoring => return None,
        };
        if self.state == SessionState::Stopped
            || !self
                .slots
                .iter()
                .any(|s| s.state == TrackState::Running && s.track.kind() == kind)
            || self
                .warnings
                .iter()
                .any(|w| w.code == "permission_revoked" && w.permission == Some(permission))
        {
            return None;
        }
        let at = self.clock.now();
        let note = SessionNote::from_track(kind, TrackNote::permission_revoked(permission).at(at));
        if note.level == NoteLevel::StopRequested && self.stop_requested.is_none() {
            self.stop_requested = Some(note.code.clone());
        }
        self.note(note.clone());
        Some(note)
    }

    /// The microphone is muted now.
    /// Sets a marker at the current recorded time (the HUD timer: pauses removed) and keeps it in
    /// the bundle's lock, so a crash does not lose it (VOC-1.AC4, AC5). `by` is the actor tag
    /// (`user` or `agent:<name>`).
    pub fn add_marker(&mut self, label: Option<&str>, by: &str) -> Result<Marker, CmError> {
        if self.state == SessionState::Stopped {
            return Err(CmError::invalid_input("The recording is already stopped"));
        }
        let label = cm_library::rec_edit::normalize_marker_label(label)?;
        let marker = Marker {
            at: self.status().recorded,
            label,
            by: Some(by.to_owned()),
        };
        self.markers.push(marker.clone());
        self.markers.sort_by_key(|m| m.at);
        if let Some(b) = self.bundle.as_mut() {
            b.set_extra(
                "markers",
                serde_json::to_value(&self.markers)
                    .map_err(|e| CmError::internal(format!("markers: {e}")))?,
            )?;
        }
        Ok(marker)
    }

    /// Gives the marker set at `at` (within 1 ms) its label: the HUD's "Add a label" after a
    /// marker. `not_found` when no marker is there.
    pub fn relabel_marker(
        &mut self,
        at: cm_core::MediaTime,
        label: Option<&str>,
    ) -> Result<Marker, CmError> {
        if self.state == SessionState::Stopped {
            return Err(CmError::invalid_input("The recording is already stopped"));
        }
        let label = cm_library::rec_edit::normalize_marker_label(label)?;
        let target = at.as_millis_floor();
        let Some(m) = self
            .markers
            .iter_mut()
            .find(|m| (m.at.as_millis_floor() - target).abs() <= 1)
        else {
            return Err(CmError::not_found("There is no marker at that time"));
        };
        m.label = label;
        let marker = m.clone();
        if let Some(b) = self.bundle.as_mut() {
            b.set_extra(
                "markers",
                serde_json::to_value(&self.markers)
                    .map_err(|e| CmError::internal(format!("markers: {e}")))?,
            )?;
        }
        Ok(marker)
    }

    /// The markers set so far.
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }

    pub fn is_mic_muted(&self) -> bool {
        self.mic_muted.last().is_some_and(|s| s.to.is_none())
    }

    /// The muted spans so far, in session time.
    pub fn mic_muted_spans(&self) -> &[MicMuteSpan] {
        &self.mic_muted
    }

    /// Mutes or unmutes the microphone (AUD-6) while recording or paused. The track keeps
    /// running; its files hold digital silence for the muted frames. Idempotent. Without a
    /// running microphone track: `invalid_input`.
    pub fn set_mic_muted(&mut self, muted: bool) -> Result<MicMuteChange, CmError> {
        if !matches!(self.state, SessionState::Recording | SessionState::Paused) {
            return Err(CmError::invalid_input("The recording is not running"));
        }
        let slot = self
            .slots
            .iter_mut()
            .find(|s| s.track.kind() == TrackKind::Mic && s.state == TrackState::Running)
            .ok_or_else(|| CmError::invalid_input("This recording has no microphone track"))?;
        if self.mic_muted.last().is_some_and(|s| s.to.is_none()) == muted {
            let last = self
                .mic_muted
                .last()
                .map(|s| s.to.unwrap_or(s.from))
                .unwrap_or_else(|| self.clock.now());
            return Ok(MicMuteChange {
                changed: false,
                at: last,
            });
        }
        let change = slot.track.set_muted(muted)?;
        if muted {
            self.mic_muted.push(MicMuteSpan {
                from: change.at,
                to: None,
            });
        } else if let Some(last) = self.mic_muted.last_mut() {
            last.to = Some(change.at.max(last.from));
        }
        // Best effort: the lock keeps the spans for crash recovery; the final list is written at
        // stop and a failure here must not undo a mute that already is in force.
        if let Some(b) = self.bundle.as_mut() {
            let _ = b.set_extra("mic_muted", mic_muted_json(&self.mic_muted));
        }
        Ok(MicMuteChange {
            changed: true,
            at: change.at,
        })
    }

    /// Pauses every track at one instant (REC-4).
    pub fn pause(&mut self) -> Result<(), CmError> {
        if self.state != SessionState::Recording {
            return Err(CmError::invalid_input("The recording is not running"));
        }
        let at = self.clock.now();
        let mut fatal = None;
        let mut notes = Vec::new();
        for slot in self
            .slots
            .iter_mut()
            .filter(|s| s.state == TrackState::Running)
        {
            if let Err(e) = guard::call("pause", || slot.track.pause(at)) {
                let kind = slot.track.kind();
                if slot.track.required() {
                    fatal.get_or_insert(e);
                } else {
                    notes.push(SessionNote {
                        track: Some(kind),
                        level: NoteLevel::Warning,
                        code: format!("{}_pause_failed", kind.as_str()),
                        message: track_err(kind, "could not pause", &e),
                        at: Some(at),
                        permission: None,
                    });
                }
            }
        }
        for n in notes {
            self.note(n);
        }
        if let Some(e) = fatal {
            return Err(e);
        }
        self.paused_at = Some(at);
        self.state = SessionState::Paused;
        Ok(())
    }

    /// Resumes every track at one instant and records the gap in the bundle.
    pub fn resume(&mut self) -> Result<(), CmError> {
        let (SessionState::Paused, Some(from)) = (self.state, self.paused_at) else {
            return Err(CmError::invalid_input("The recording is not paused"));
        };
        let at = self.clock.now().max(from);
        let mut fatal = None;
        let mut notes = Vec::new();
        for slot in self
            .slots
            .iter_mut()
            .filter(|s| s.state == TrackState::Running)
        {
            if let Err(e) = guard::call("resume", || slot.track.resume(at)) {
                let kind = slot.track.kind();
                if slot.track.required() {
                    fatal.get_or_insert(e);
                } else {
                    notes.push(SessionNote {
                        track: Some(kind),
                        level: NoteLevel::Warning,
                        code: format!("{}_resume_failed", kind.as_str()),
                        message: track_err(kind, "could not resume", &e),
                        at: Some(at),
                        permission: None,
                    });
                }
            }
        }
        for n in notes {
            self.note(n);
        }
        if let Some(e) = fatal {
            return Err(e);
        }
        let gap = PauseGap { from, to: at };
        self.gaps.push(gap);
        if let Some(b) = self.bundle.as_mut() {
            b.add_gap(gap)?;
        }
        self.paused_at = None;
        self.state = SessionState::Recording;
        Ok(())
    }

    /// Stops every track at one instant. The bundle is not finalized yet: call
    /// [`StoppedSession::finalize`] or [`StoppedSession::finish_into`].
    pub fn stop(&mut self) -> Result<StoppedSession, CmError> {
        if self.state == SessionState::Stopped {
            return Err(CmError::invalid_input("The recording is already stopped"));
        }
        let at = self.clock.now();
        // Anything the recorders reported since the last tick still goes into the manifest.
        let _ = self.tick();
        if let Some(from) = self.paused_at.take() {
            let gap = PauseGap {
                from,
                to: at.max(from),
            };
            self.gaps.push(gap);
            if let Some(b) = self.bundle.as_mut() {
                b.add_gap(gap)?;
            }
        }
        if let Some(open) = self.mic_muted.last_mut().filter(|s| s.to.is_none()) {
            open.to = Some(at.max(open.from));
        }
        let t0 = Instant::now();
        let results: Vec<Option<Result<TrackInfo, CmError>>> = std::thread::scope(|s| {
            let handles: Vec<_> = self
                .slots
                .iter_mut()
                .map(|slot| {
                    let running = slot.state == TrackState::Running;
                    s.spawn(move || running.then(|| guard::call("stop", || slot.track.stop(at))))
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|_| {
                        Some(Err(CmError::internal("a track panicked while stopping")))
                    })
                })
                .collect()
        });
        let stop_ms = t0.elapsed().as_secs_f64() * 1e3;
        self.state = SessionState::Stopped;

        let already_failed: Vec<TrackKind> = self
            .warnings
            .iter()
            .filter(|w| w.code == "track_failed")
            .filter_map(|w| w.track)
            .collect();
        let mut infos = Vec::new();
        let mut required_err = None;
        let mut first_err: Option<CmError> = None;
        let mut notes = Vec::new();
        for (slot, res) in self.slots.iter_mut().zip(results) {
            let kind = slot.track.kind();
            match res {
                None => {}
                Some(Ok(info)) => {
                    slot.state = TrackState::Stopped;
                    // A track that stopped growing before the end (R3-1) says so in its details.
                    if let Some(t) = info.details["truncated_at"]
                        .as_i64()
                        .filter(|_| !already_failed.contains(&kind))
                    {
                        notes.push(SessionNote {
                            track: Some(kind),
                            level: NoteLevel::Warning,
                            code: "track_failed".to_owned(),
                            message: format!(
                                "The {} track stopped early ({}); what was written is kept",
                                kind.as_str(),
                                info.details["write_error"]
                                    .as_str()
                                    .unwrap_or("write error")
                            ),
                            at: Some(MediaTime::from_ticks(t)),
                            permission: None,
                        });
                    }
                    // The file ended before the recording did (disk full or too slow, the encoder
                    // failed at the end): kept up to its last fragment; say so (R3-5).
                    if let Some(h) = info.details.get("writer_halt").filter(|h| h.is_object()) {
                        if !self.warnings.iter().any(|w| {
                            w.track == Some(kind)
                                && matches!(
                                    w.code.as_str(),
                                    "disk_full" | "screen_halted" | "screen_encoder_failed"
                                )
                        }) {
                            notes.push(SessionNote {
                                track: Some(kind),
                                level: NoteLevel::Warning,
                                code: format!("{}_stopped_early", kind.as_str()),
                                message: format!(
                                    "The {} file ended early ({}); what was written is kept",
                                    kind.as_str(),
                                    h["message"].as_str().unwrap_or("disk error")
                                ),
                                at: Some(at),
                                permission: None,
                            });
                        }
                    }
                    infos.push(info);
                }
                Some(Err(e)) => {
                    slot.state = TrackState::Failed(e.to_string());
                    if slot.track.required() {
                        required_err.get_or_insert(e);
                    } else {
                        if first_err.is_none() {
                            first_err = Some(e.clone());
                        }
                        // R3-1: files are removed only when a track fails to START. At stop
                        // whatever the track wrote is kept: hours of audio must not vanish
                        // because the last flush failed.
                        notes.push(SessionNote {
                            track: Some(kind),
                            level: NoteLevel::Warning,
                            code: format!("{}_failed", kind.as_str()),
                            message: format!(
                                "{}; its files were kept in the recording folder",
                                track_err(kind, "could not be finished", &e)
                            ),
                            at: Some(at),
                            permission: None,
                        });
                    }
                }
            }
        }
        // R3-6: one `pauses` key per track, the session's own list (closed gaps plus the final
        // one when the recording was stopped while paused), whatever the recorder wrote.
        for info in &mut infos {
            unify_pauses(info, &self.gaps);
        }
        for n in notes {
            self.note(n);
        }
        if self.audio_only && infos.is_empty() && required_err.is_none() {
            required_err = first_err;
        }
        if let Some(e) = required_err {
            if cm_encode::is_no_frames(&e) {
                // R3-5: stopped before the first frame. There is nothing to keep and nothing to
                // recover: delete the empty bundle instead of leaving a lock that every launch
                // would try again.
                self.bundle = None;
                let _ = fs::remove_dir_all(&self.dir);
                return Err(e);
            }
            // The lock stays: the next launch's recovery rebuilds what was written.
            return Err(e);
        }
        let paused = self
            .gaps
            .iter()
            .map(|g| g.to.ticks() - g.from.ticks())
            .sum::<i64>();
        let mut bundle = self
            .bundle
            .take()
            .ok_or_else(|| CmError::internal("the recording bundle is gone"))?;
        if !self.mic_muted.is_empty() {
            bundle.set_extra("mic_muted", mic_muted_json(&self.mic_muted))?;
        }
        bundle.set_extra(
            "session",
            json!({
                "mic_muted": mic_muted_json(&self.mic_muted),
                "live_at": self.live_at.ticks(),
                "wall_end": at.ticks(),
                "paused": paused,
                "requested_tracks": self.requested.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
                "recorded_tracks": infos.iter().map(|i| i.kind.as_str()).collect::<Vec<_>>(),
                "stop_ms": stop_ms,
                "start_ms": self.slots.iter().map(|s| (s.track.kind().as_str().to_owned(), json!(s.start_ms))).collect::<Map<_, _>>(),
            }),
        )?;
        Ok(StoppedSession {
            capture_id: self.capture_id.clone(),
            bundle,
            tracks: infos,
            warnings: self.warnings.clone(),
            wall_end: at,
            paused: MediaTime::from_ticks(paused),
            live_at: self.live_at,
            gaps: self.gaps.clone(),
            stop_ms,
            now: self.now_wall,
        })
    }

    /// Stops everything and deletes the bundle (HUD Discard, and the first half of Restart).
    pub fn discard(mut self) {
        let at = self.clock.now();
        for slot in &mut self.slots {
            if slot.state == TrackState::Running {
                let _ = guard::call("stop", || slot.track.stop(at));
            }
        }
        self.state = SessionState::Stopped;
        self.bundle = None;
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A stopped session whose bundle still has its lock.
#[derive(Debug)]
pub struct StoppedSession {
    capture_id: String,
    bundle: RecBundle,
    pub tracks: Vec<TrackInfo>,
    pub warnings: Vec<SessionNote>,
    /// Session time of the stop.
    pub wall_end: MediaTime,
    /// Total time spent paused.
    pub paused: MediaTime,
    pub live_at: MediaTime,
    pub gaps: Vec<PauseGap>,
    /// Time `stop` took on all tracks (REC-5.AC2: within 2 s).
    pub stop_ms: f64,
    now: OffsetDateTime,
}

impl StoppedSession {
    pub fn dir(&self) -> &std::path::Path {
        self.bundle.dir()
    }

    /// The recording's capture id.
    pub fn capture_id(&self) -> &str {
        &self.capture_id
    }

    /// Adds a top-level manifest field before finalizing (the engine records `stop_reason` and who
    /// started the recording this way).
    pub fn set_extra(&mut self, key: &str, value: Value) -> Result<(), CmError> {
        self.bundle.set_extra(key, value)
    }

    /// Writes `manifest.json` and removes the lock; the caller indexes the bundle (or not).
    pub fn finalize(self) -> Result<(RecManifest, PathBuf), CmError> {
        let dir = self.bundle.dir().to_owned();
        let m = self
            .bundle
            .finalize(&self.tracks, OffsetDateTime::now_utc())?;
        Ok((m, dir))
    }

    /// Finalizes and indexes the recording in `library`.
    pub fn finish_into(
        self,
        library: &mut Library,
    ) -> Result<cm_library::index::CaptureRecord, CmError> {
        library.finish_recording(self.bundle, &self.tracks, OffsetDateTime::now_utc())
    }

    /// Wall time the recording was started.
    pub fn started_at(&self) -> OffsetDateTime {
        self.now
    }
}

/// Sets `details.pauses` of a finished track to the session's pause list, `[{from, to}]` in
/// session ticks (R3-6, REC-4.AC5). The screen recorder used to call them `gaps` and the cursor
/// `pause_gaps`; `gaps` now means audio dropouts only, so the old pause keys are dropped here
/// (readers accept both, see `cm_media::spec`).
fn unify_pauses(info: &mut TrackInfo, gaps: &[PauseGap]) {
    let pauses: Vec<Value> = gaps
        .iter()
        .filter(|g| g.to > info.start || g.from >= info.start)
        .map(|g| json!({ "from": g.from.ticks(), "to": g.to.ticks() }))
        .collect();
    if let Some(d) = info.details.as_object_mut() {
        match info.kind {
            TrackKind::Screen => {
                d.remove("gaps");
            }
            TrackKind::Cursor => {
                d.remove("pause_gaps");
            }
            _ => {}
        }
        d.insert("pauses".into(), Value::Array(pauses));
    }
}

fn timebase_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "uptime_raw_ns"
    } else {
        "host_ns"
    }
}
