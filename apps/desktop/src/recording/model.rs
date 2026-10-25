//! The HUD's state model: what the recording windows show, derived only from the daemon's
//! `recording.*` events and `get_recording_status` (REC-4, REC-5, AGT-2).
//!
//! The UI never decides anything the engine knows. The model is fed with [`HudState::apply_event`]
//! (every bus event) and [`HudState::apply_status`] (the 10 Hz poll that keeps the timer and the mic
//! meter smooth), and answers with a [`Change`] so the controller knows which windows to open or
//! close.
//!
//! # Timer
//!
//! `recorded_ms` from the engine is *recorded* time: pauses are not in it (REC-4.AC1: 10 s,
//! 20 s paused, 10 s gives 00:20). Between two status answers the timer extrapolates with the local
//! clock, but only while the phase is `Recording`; a pause, a resume countdown or a stop freezes it.
//! Every answer rebases the extrapolation, so it can drift at most one poll interval.

use cm_core::Permission;
use cm_i18n::{tr, tr_ctx};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

use crate::recording::geometry::{AreaKind, BubblePosition, BubbleShape, BubbleSize};

/// How far past the last engine answer the timer may extrapolate (stats arrive every 1 s; this
/// only bounds a stalled engine).
pub const MAX_EXTRAPOLATION: Duration = Duration::from_millis(2500);

/// Where a recording is, as the user sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    /// A start was requested; the engine has not answered yet.
    Starting,
    Countdown,
    Recording,
    Paused,
    /// Paused, with the resume countdown running.
    Resuming,
    /// Restart: the take was discarded and the next one is about to count down.
    Restarting,
    Stopping,
    /// The 60 fps self-test captures the screen (REC-3, REC-5.AC5, NFR-11): the indicator, the
    /// tray and a small HUD show, with Stop only.
    SelfTest,
}

impl Phase {
    /// The phases in which a recording (or a countdown to one) occupies the screen.
    pub fn is_active(self) -> bool {
        self != Phase::Idle
    }

    /// Whether the timer runs.
    pub fn is_running(self) -> bool {
        self == Phase::Recording
    }

    fn from_engine(s: &str) -> Option<Phase> {
        Some(match s {
            "countdown" => Phase::Countdown,
            "starting" => Phase::Starting,
            "recording" => Phase::Recording,
            "paused" => Phase::Paused,
            "resuming" => Phase::Resuming,
            "stopping" => Phase::Stopping,
            "selftest" => Phase::SelfTest,
            _ => return None,
        })
    }
}

/// `indicator.area`.
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub kind: AreaKind,
    pub display_id: Option<u32>,
    pub window_id: Option<u32>,
    /// Physical `[x, y, w, h]`, global desktop space.
    pub bounds: Option<[i64; 4]>,
    pub source_size: [u32; 2],
    pub scale: f64,
}

/// `indicator.camera`: the bubble facts of CAM-1.
#[derive(Debug, Clone, PartialEq)]
pub struct CameraFacts {
    pub shape: BubbleShape,
    pub size: BubbleSize,
    pub position: BubblePosition,
    pub mirror: bool,
}

/// Who started the recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartedBy {
    User,
    Agent { client: String, tool: String },
}

impl StartedBy {
    pub fn agent(&self) -> Option<(&str, &str)> {
        match self {
            StartedBy::Agent { client, tool } => Some((client, tool)),
            StartedBy::User => None,
        }
    }
}

/// The `indicator` object of `recording.started` and `get_recording_status`.
#[derive(Debug, Clone, PartialEq)]
pub struct Indicator {
    pub area: Area,
    pub output: Option<[u32; 2]>,
    pub camera: Option<CameraFacts>,
    pub mic: bool,
    pub system_audio: bool,
    pub cursor: bool,
    pub started_by: StartedBy,
}

/// `(code, message)` of an `error` field that is a string or `{code, message}`.
fn error_parts(e: Option<&Value>) -> (Option<String>, String) {
    match e {
        Some(Value::String(m)) => (None, m.clone()),
        Some(o) if o.is_object() => (
            o.get("code").and_then(Value::as_str).map(str::to_owned),
            o.get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
        ),
        _ => (None, String::new()),
    }
}

fn size2(v: &Value) -> Option<[u32; 2]> {
    let a = v.as_array()?;
    Some([
        u32::try_from(a.first()?.as_u64()?).ok()?,
        u32::try_from(a.get(1)?.as_u64()?).ok()?,
    ])
}

impl Indicator {
    pub fn parse(v: &Value) -> Option<Indicator> {
        let a = v.get("area")?;
        let bounds = a.get("bounds").and_then(Value::as_array).and_then(|b| {
            Some([
                b.first()?.as_i64()?,
                b.get(1)?.as_i64()?,
                b.get(2)?.as_i64()?,
                b.get(3)?.as_i64()?,
            ])
        });
        let area = Area {
            kind: AreaKind::parse(a.get("kind").and_then(Value::as_str).unwrap_or("display")),
            display_id: a
                .get("display_id")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            window_id: a
                .get("window_id")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            bounds,
            source_size: a.get("source_size").and_then(size2).unwrap_or([0, 0]),
            scale: a.get("scale").and_then(Value::as_f64).unwrap_or(1.0),
        };
        let camera = v
            .get("camera")
            .filter(|c| c.is_object())
            .map(|c| CameraFacts {
                shape: c
                    .get("shape")
                    .and_then(Value::as_str)
                    .and_then(BubbleShape::parse)
                    .unwrap_or_default(),
                size: c
                    .get("size")
                    .and_then(Value::as_str)
                    .and_then(BubbleSize::parse)
                    .unwrap_or_default(),
                position: c
                    .get("position")
                    .and_then(BubblePosition::from_json)
                    .unwrap_or_default(),
                mirror: c.get("mirror").and_then(Value::as_bool).unwrap_or(true),
            });
        let by = v.get("started_by");
        let started_by = match by.and_then(|b| b.get("kind")).and_then(Value::as_str) {
            Some("agent") => StartedBy::Agent {
                client: by
                    .and_then(|b| b.get("client"))
                    .and_then(Value::as_str)
                    .unwrap_or("an agent") // i18n-ignore: fallback client name kept as data
                    .to_owned(),
                tool: by
                    .and_then(|b| b.get("tool"))
                    .and_then(Value::as_str)
                    .unwrap_or("start_recording")
                    .to_owned(),
            },
            _ => StartedBy::User,
        };
        Some(Indicator {
            area,
            output: v.get("output").and_then(|o| {
                Some([
                    u32::try_from(o.get("width")?.as_u64()?).ok()?,
                    u32::try_from(o.get("height")?.as_u64()?).ok()?,
                ])
            }),
            camera,
            mic: v.get("mic").and_then(Value::as_bool).unwrap_or(false),
            system_audio: v
                .get("system_audio")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            cursor: v.get("cursor").and_then(Value::as_bool).unwrap_or(true),
            started_by,
        })
    }
}

/// `recording.health`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Health {
    #[default]
    Ok,
    /// Frames are arriving late (REC-3): the HUD shows a warning chip.
    Degraded,
}

/// The permission names the engine puts in events (`permission` of `recording.stopped`).
pub fn permission_from_str(s: &str) -> Option<Permission> {
    Some(match s {
        "screen" => Permission::Screen,
        "camera" => Permission::Camera,
        "microphone" => Permission::Microphone,
        "accessibility" => Permission::Accessibility,
        "input_monitoring" => Permission::InputMonitoring,
        _ => return None,
    })
}

/// One `recording.warning`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub code: String,
    pub message: String,
    pub track: Option<String>,
}

/// The countdown in progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Countdown {
    pub remaining: u32,
    pub total: u32,
    /// `context: "resume"`: the 1 s countdown before a resume.
    pub resume: bool,
    /// When this step started, for the ring.
    pub step_started: Instant,
}

/// How a recording run ended.
#[derive(Debug, Clone, PartialEq)]
pub enum Ended {
    Stopped {
        capture_id: Option<String>,
        duration_s: Option<f64>,
        reason: String,
        /// The stop failed (`recording.stopped` with an `error`).
        error: Option<String>,
        /// `reason: "permission_revoked"`: the permission the OS removed (PRM-1).
        permission: Option<Permission>,
    },
    /// The take was deleted.
    Discarded { reason: String },
    /// The countdown was cancelled: nothing was recorded.
    Cancelled,
    /// The start failed after the countdown began (`recording.discarded` with `start_failed`):
    /// nothing was recorded. `code` is the shared error code when the engine said one.
    StartFailed {
        code: Option<String>,
        message: String,
    },
    /// The engine says it is idle although the model thought a take was on screen, and the
    /// closing event never arrived (R4-3). The windows close; the library may hold the take.
    Lost,
    /// The 60 fps self-test finished (`passed`, `failed`, `cancelled`, `error`).
    SelfTest {
        state: String,
        message: Option<String>,
    },
}

/// What an event or a status answer changed, for the window controller.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    None,
    /// Countdown windows should show (or update).
    Countdown,
    /// The 60 fps self-test went live: open the indicator, the tray state and the small HUD.
    SelfTest,
    /// Recording went live: open the HUD, the indicator and the bubble. `agent` is who started it
    /// when it was an agent (the agent toast).
    Started {
        agent: Option<(String, String)>,
    },
    /// Paused, resumed or numbers changed: repaint (windows are reconciled from the model).
    Updated,
    /// The take was discarded and a restart follows: keep the windows.
    Restarting,
    /// Everything closes.
    Ended(Ended),
}

#[derive(Debug, Clone)]
pub struct HudState {
    pub phase: Phase,
    pub recording_id: Option<String>,
    pub capture_id: Option<String>,
    pub title: Option<String>,
    pub indicator: Option<Indicator>,
    pub countdown: Option<Countdown>,
    recorded_ms: i64,
    anchored_at: Option<Instant>,
    /// Latest mic peak in dBFS.
    pub mic_dbfs: Option<f64>,
    pub health: Health,
    pub warnings: Vec<Warning>,
    pub pauses: u32,
    pub max_duration_s: Option<f64>,
    pub restart_of: Option<String>,
    /// The bubble was hidden with the Webcam button (the camera keeps recording).
    pub bubble_hidden: bool,
    /// Seconds left in the self-test.
    pub selftest_remaining_s: Option<f64>,
    /// Consecutive status answers in which the engine said it is idle while the model is not.
    idle_polls: u8,
}

impl Default for HudState {
    fn default() -> Self {
        HudState {
            phase: Phase::Idle,
            recording_id: None,
            capture_id: None,
            title: None,
            indicator: None,
            countdown: None,
            recorded_ms: 0,
            anchored_at: None,
            mic_dbfs: None,
            health: Health::Ok,
            warnings: Vec::new(),
            pauses: 0,
            max_duration_s: None,
            restart_of: None,
            bubble_hidden: false,
            selftest_remaining_s: None,
            idle_polls: 0,
        }
    }
}

/// Consecutive "engine is idle" answers (250 ms apart while counting down) before the model gives
/// up on a closing event it never got.
const IDLE_POLLS_TO_RESET: u8 = 2;
/// A restart, and a start the engine announced, legitimately pass through an idle engine for a
/// moment (preflight checks, the old take being deleted); give them two seconds.
const IDLE_POLLS_TO_RESET_RESTARTING: u8 = 8;

/// What Esc on the countdown window means (R4-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscAction {
    /// The start countdown: nothing exists yet, cancel the start (`discard_recording`).
    CancelStart,
    /// The resume countdown: only cancel the resume (`pause_recording`); the take stays.
    CancelResume,
    Ignore,
}

const MAX_WARNINGS: usize = 3;

/// MM:SS, or H:MM:SS from an hour on (the engine's `clock_text`).
pub fn clock_text(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{:02}:{:02}", s / 60, s % 60)
    }
}

/// The five bars of the mic meter (REC-5.AC15) from a peak in dBFS: bar `n` lights from
/// `-48 + 9 n` dBFS, so silence shows none and a loud voice around -6 dBFS lights all five.
pub fn mic_bars(dbfs: Option<f64>) -> u8 {
    let Some(db) = dbfs.filter(|d| d.is_finite()) else {
        return 0;
    };
    (1..=5u8)
        .filter(|n| db >= -48.0 + 9.0 * f64::from(*n - 1))
        .count() as u8
}

impl HudState {
    /// A start was requested (the user pressed Start); blocks a second start.
    pub fn begin_start(&mut self) {
        if self.phase == Phase::Idle {
            self.phase = Phase::Starting;
        }
    }

    /// The start failed or was refused before any event: back to idle.
    pub fn abort_start(&mut self) {
        if self.phase == Phase::Starting && self.recording_id.is_none() {
            *self = HudState::default();
        }
    }

    /// `start_recording` or `restart_recording` returned an error. Until `recording.started`
    /// arrives the take does not exist, so whatever the model shows (the sheet's wait, a countdown
    /// the engine abandoned, a restart that never began counting) is reset to idle. Returns whether
    /// the model was reset; the caller then closes every window (R4-1). After `recording.started`
    /// an error is about something else (a refused second start) and the live take is left alone.
    pub fn fail_start(&mut self) -> bool {
        if matches!(
            self.phase,
            Phase::Starting | Phase::Countdown | Phase::Restarting
        ) {
            *self = HudState::default();
            true
        } else {
            false
        }
    }

    /// The resume countdown was cancelled with `pause_recording`: the take is paused again. The
    /// engine publishes no event for this, so the caller settles the model from the call's answer.
    pub fn resume_cancelled(&mut self) {
        if self.phase == Phase::Resuming {
            self.phase = Phase::Paused;
            self.countdown = None;
            self.anchored_at = None;
        }
    }

    /// Whether the answer to a Discard or Restart dialog still applies: the take it asked about
    /// is the one on screen (R4-7).
    pub fn dialog_still_applies(&self, asked_about: Option<&str>) -> bool {
        matches!(self.phase, Phase::Recording | Phase::Paused)
            && self.recording_id.as_deref() == asked_about
    }

    /// Whether a countdown window belongs on screen: a countdown is running and the phase says so.
    pub fn countdown_wanted(&self) -> bool {
        self.countdown.is_some() && matches!(self.phase, Phase::Countdown | Phase::Resuming)
    }

    /// What Esc on the countdown does in this phase (R4-2).
    pub fn esc_action(&self) -> EscAction {
        match self.phase {
            Phase::Countdown if self.capture_id.is_none() => EscAction::CancelStart,
            Phase::Resuming => EscAction::CancelResume,
            _ => EscAction::Ignore,
        }
    }

    fn reset_for(&mut self, id: &str) {
        let keep_bubble = self.bubble_hidden;
        *self = HudState::default();
        self.recording_id = Some(id.to_owned());
        self.bubble_hidden = keep_bubble;
    }

    /// Recorded time in ms at `now` (see the module docs).
    pub fn recorded_ms(&self, now: Instant) -> i64 {
        match (self.phase.is_running(), self.anchored_at) {
            (true, Some(at)) => {
                let extra = now
                    .saturating_duration_since(at)
                    .min(MAX_EXTRAPOLATION)
                    .as_millis() as i64;
                self.recorded_ms + extra
            }
            _ => self.recorded_ms,
        }
    }

    /// The timer text (REC-4.AC1: recorded time only).
    pub fn timer_text(&self, now: Instant) -> String {
        clock_text(self.recorded_ms(now))
    }

    fn rebase(&mut self, recorded_ms: i64, now: Instant) {
        // The engine's number can lag the extrapolation by a few ms; never run backwards while
        // recording.
        let shown = self.recorded_ms(now);
        self.recorded_ms =
            if self.phase.is_running() && recorded_ms < shown && shown - recorded_ms < 400 {
                shown
            } else {
                recorded_ms
            };
        self.anchored_at = Some(now);
    }

    fn belongs(&self, data: &Value) -> bool {
        match (
            &self.recording_id,
            data.get("recording_id").and_then(Value::as_str),
        ) {
            (Some(mine), Some(theirs)) => mine == theirs,
            _ => true,
        }
    }

    /// Applies one daemon event (`name` is the event name, `data` its payload).
    pub fn apply_event(&mut self, name: &str, data: &Value, now: Instant) -> Change {
        let id = data.get("recording_id").and_then(Value::as_str);
        match name {
            "recording.countdown" => {
                // A countdown of a new run (start, or the one after a restart) resets the model; a
                // resume countdown belongs to the current run.
                let resume = data.get("context").and_then(Value::as_str) == Some("resume");
                if !resume {
                    if let Some(id) = id {
                        if self.recording_id.as_deref() != Some(id) {
                            let restarting = self.phase == Phase::Restarting;
                            let indicator = self.indicator.take();
                            self.reset_for(id);
                            if restarting {
                                self.indicator = indicator;
                            }
                        }
                    }
                } else if self.phase == Phase::Idle || !self.belongs(data) {
                    // A resume countdown of a run this model does not know (a lost event).
                    return Change::None;
                }
                let n = |k: &str| {
                    data.get(k)
                        .and_then(Value::as_u64)
                        .and_then(|v| u32::try_from(v).ok())
                        .unwrap_or(0)
                };
                self.countdown = Some(Countdown {
                    remaining: n("remaining"),
                    total: n("total"),
                    resume,
                    step_started: now,
                });
                self.phase = if resume {
                    Phase::Resuming
                } else {
                    Phase::Countdown
                };
                Change::Countdown
            }
            "recording.starting" => {
                // Published before the countdown and before any track starts: the area is known
                // now, so the countdown can sit on the right display and a recording an agent
                // starts is on screen from its first moment (NFR-11).
                if let Some(id) = id {
                    if self.recording_id.as_deref() != Some(id) {
                        let restarting = self.phase == Phase::Restarting;
                        let keep = self.indicator.take();
                        self.reset_for(id);
                        if restarting {
                            self.phase = Phase::Restarting;
                            self.indicator = keep;
                        }
                    }
                }
                if let Some(i) = data.get("indicator").and_then(Indicator::parse) {
                    self.indicator = Some(i);
                }
                if self.phase == Phase::Idle {
                    self.phase = Phase::Starting;
                }
                Change::Updated
            }
            "recording.started" => {
                if let Some(id) = id {
                    if self.recording_id.as_deref() != Some(id) {
                        let keep = self.indicator.take();
                        self.reset_for(id);
                        self.indicator = keep;
                    }
                }
                self.countdown = None;
                self.phase = Phase::Recording;
                self.capture_id = data
                    .get("capture_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.title = data.get("title").and_then(Value::as_str).map(str::to_owned);
                self.restart_of = data
                    .get("restart_of")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.max_duration_s = data.get("max_duration_s").and_then(Value::as_f64);
                if let Some(i) = data.get("indicator").and_then(Indicator::parse) {
                    self.indicator = Some(i);
                }
                self.recorded_ms = 0;
                self.anchored_at = Some(now);
                self.mic_dbfs = None;
                self.health = Health::Ok;
                for w in data
                    .get("warnings")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    self.push_warning(w);
                }
                let agent = self
                    .indicator
                    .as_ref()
                    .and_then(|i| i.started_by.agent())
                    .map(|(c, t)| (c.to_owned(), t.to_owned()));
                Change::Started { agent }
            }
            "recording.paused" => {
                if !self.belongs(data) {
                    return Change::None;
                }
                if let Some(ms) = data.get("recorded_ms").and_then(Value::as_i64) {
                    self.recorded_ms = ms;
                } else {
                    self.recorded_ms = self.recorded_ms(now);
                }
                self.anchored_at = None;
                self.countdown = None;
                self.phase = Phase::Paused;
                self.pauses += 1;
                Change::Updated
            }
            "recording.resumed" => {
                if !self.belongs(data) {
                    return Change::None;
                }
                self.countdown = None;
                self.phase = Phase::Recording;
                self.anchored_at = Some(now);
                Change::Updated
            }
            "recording.stats" => {
                // A stats event that trails `recording.stopped` must not revive an idle model.
                if self.phase == Phase::Idle || !self.belongs(data) {
                    return Change::None;
                }
                self.apply_status(data, now)
            }
            "recording.health" => {
                if !self.belongs(data) {
                    return Change::None;
                }
                let h = match data.get("status").and_then(Value::as_str) {
                    Some("degraded") => Health::Degraded,
                    _ => Health::Ok,
                };
                if h == self.health {
                    Change::None
                } else {
                    self.health = h;
                    Change::Updated
                }
            }
            "recording.warning" => {
                if !self.belongs(data) {
                    return Change::None;
                }
                self.push_warning(data);
                Change::Updated
            }
            "recording.stopped" => {
                if !self.belongs(data) {
                    return Change::None;
                }
                let ended = Ended::Stopped {
                    capture_id: data
                        .get("capture_id")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    duration_s: data.get("duration_s").and_then(Value::as_f64),
                    reason: data
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("user")
                        .to_owned(),
                    error: data
                        .get("error")
                        .and_then(|e| e.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    permission: data
                        .get("permission")
                        .and_then(Value::as_str)
                        .and_then(permission_from_str),
                };
                *self = HudState::default();
                Change::Ended(ended)
            }
            "recording.discarded" => {
                if self.phase == Phase::Idle || !self.belongs(data) {
                    return Change::None;
                }
                let reason = data
                    .get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("user")
                    .to_owned();
                if reason == "restart" {
                    self.phase = Phase::Restarting;
                    self.anchored_at = None;
                    self.recorded_ms = 0;
                    self.countdown = None;
                    self.warnings.clear();
                    self.health = Health::Ok;
                    return Change::Restarting;
                }
                if reason == "start_failed" {
                    let (code, message) = error_parts(data.get("error"));
                    *self = HudState::default();
                    return Change::Ended(Ended::StartFailed { code, message });
                }
                let cancelled = data.get("capture_id").is_none_or(Value::is_null);
                *self = HudState::default();
                Change::Ended(if cancelled {
                    Ended::Cancelled
                } else {
                    Ended::Discarded { reason }
                })
            }
            "recording.selftest" => self.apply_selftest(data, now),
            _ => Change::None,
        }
    }

    /// One `recording.selftest` event (REC-3): the screen is being captured, so the indicator
    /// shows (REC-5.AC5, NFR-11).
    fn apply_selftest(&mut self, data: &Value, _now: Instant) -> Change {
        let state = data.get("state").and_then(Value::as_str).unwrap_or("");
        let remaining = data.get("remaining_s").and_then(Value::as_f64);
        match state {
            "started" => {
                let id = data
                    .get("selftest_id")
                    .and_then(Value::as_str)
                    .unwrap_or("selftest");
                self.reset_for(id);
                self.phase = Phase::SelfTest;
                self.indicator = data.get("indicator").and_then(Indicator::parse);
                self.selftest_remaining_s = remaining;
                Change::SelfTest
            }
            "running" if self.phase == Phase::SelfTest => {
                self.selftest_remaining_s = remaining.or(self.selftest_remaining_s);
                Change::Updated
            }
            "passed" | "failed" | "cancelled" | "error" if self.phase == Phase::SelfTest => {
                let message = data
                    .get("error")
                    .map(|e| error_parts(Some(e)).1)
                    .filter(|m| !m.is_empty());
                *self = HudState::default();
                Change::Ended(Ended::SelfTest {
                    state: state.to_owned(),
                    message,
                })
            }
            _ => Change::None,
        }
    }

    /// The engine says nothing is running. One answer proves little (a restart passes through
    /// idle, a start has not begun), so the model resets only after consecutive answers while it
    /// believes a take is on screen: a lost closing event must not strand the HUD, the tray and
    /// the bubble (R4-3).
    fn engine_idle(&mut self) -> Change {
        // A start of our own that the engine has not announced yet (`Starting`, no id) is not
        // stranded: the engine is simply not there yet.
        let waiting_for_engine = self.phase == Phase::Starting && self.recording_id.is_none();
        if self.phase == Phase::Idle || waiting_for_engine {
            self.idle_polls = 0;
            return Change::None;
        }
        self.idle_polls = self.idle_polls.saturating_add(1);
        let limit = if matches!(self.phase, Phase::Restarting | Phase::Starting) {
            IDLE_POLLS_TO_RESET_RESTARTING
        } else {
            IDLE_POLLS_TO_RESET
        };
        if self.idle_polls < limit {
            return Change::None;
        }
        let was_selftest = self.phase == Phase::SelfTest;
        *self = HudState::default();
        Change::Ended(if was_selftest {
            Ended::SelfTest {
                state: "cancelled".to_owned(),
                message: None,
            }
        } else {
            Ended::Lost
        })
    }

    /// Picks up a run the model missed (the event stream lagged and dropped `recording.started`
    /// or `recording.selftest`) from a `get_recording_status` answer. Only an idle model adopts.
    pub fn adopt_status(&mut self, v: &Value, now: Instant) -> Change {
        if self.phase != Phase::Idle {
            return Change::None;
        }
        match v.get("state").and_then(Value::as_str) {
            Some("selftest") => {
                let st = v.get("selftest").unwrap_or(&Value::Null);
                self.apply_selftest(
                    &json!({
                        "state": "started",
                        "selftest_id": st.get("selftest_id"),
                        "remaining_s": st.get("remaining_s"),
                        "indicator": st.get("indicator"),
                    }),
                    now,
                )
            }
            Some(s @ ("recording" | "paused")) if v.get("indicator").is_some() => {
                let mut start = v.clone();
                start["state"] = json!("recording");
                let c = self.apply_event("recording.started", &start, now);
                if s == "paused" {
                    self.phase = Phase::Paused;
                    self.anchored_at = None;
                }
                if let Some(ms) = v.get("recorded_ms").and_then(Value::as_i64) {
                    self.recorded_ms = ms;
                }
                c
            }
            _ => Change::None,
        }
    }

    /// Applies a `get_recording_status` answer (or a `recording.stats` payload, which is the same
    /// object).
    pub fn apply_status(&mut self, v: &Value, now: Instant) -> Change {
        let state = v.get("state").and_then(Value::as_str).unwrap_or("");
        if matches!(state, "idle" | "stopped" | "failed") {
            return self.engine_idle();
        }
        let Some(phase) = Phase::from_engine(state) else {
            return Change::None;
        };
        // An idle model is never revived by a trailing answer; a run the model missed is adopted
        // explicitly ([`HudState::adopt_status`]).
        if self.phase == Phase::Idle || !self.belongs(v) {
            return Change::None;
        }
        // A self-test and a recording never turn into each other.
        if (phase == Phase::SelfTest) != (self.phase == Phase::SelfTest) {
            return Change::None;
        }
        self.idle_polls = 0;
        let before = (
            self.phase,
            self.mic_dbfs,
            self.countdown.is_some(),
            self.selftest_remaining_s.map(|s| s.ceil() as i64),
        );
        // A status answer that races a restart must not pull the model out of `Restarting`.
        if self.phase != Phase::Restarting || phase != Phase::Stopping {
            self.phase = phase;
        }
        if !matches!(self.phase, Phase::Countdown | Phase::Resuming) {
            self.countdown = None;
        }
        if phase == Phase::SelfTest {
            let remaining = v
                .pointer("/selftest/remaining_s")
                .and_then(Value::as_f64)
                .or(self.selftest_remaining_s);
            self.selftest_remaining_s = remaining;
        } else {
            if let Some(ms) = v.get("recorded_ms").and_then(Value::as_i64) {
                self.rebase(ms, now);
            }
            self.mic_dbfs = v.get("mic_level_dbfs").and_then(Value::as_f64);
            self.pauses = v
                .get("pauses")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(self.pauses);
            if self.indicator.is_none() {
                self.indicator = v.get("indicator").and_then(Indicator::parse);
            }
        }
        let after = (
            self.phase,
            self.mic_dbfs,
            self.countdown.is_some(),
            self.selftest_remaining_s.map(|s| s.ceil() as i64),
        );
        if before == after {
            Change::None
        } else {
            Change::Updated
        }
    }

    fn push_warning(&mut self, w: &Value) {
        let code = w.get("code").and_then(Value::as_str).unwrap_or("warning");
        let message = w.get("message").and_then(Value::as_str).unwrap_or(code);
        if self
            .warnings
            .iter()
            .any(|x| x.code == code && x.message == message)
        {
            return;
        }
        self.warnings.push(Warning {
            code: code.to_owned(),
            message: message.to_owned(),
            track: w.get("track").and_then(Value::as_str).map(str::to_owned),
        });
        if self.warnings.len() > MAX_WARNINGS {
            self.warnings.remove(0);
        }
    }

    /// The countdown digit to show, if a countdown is on screen.
    pub fn countdown_digit(&self) -> Option<u32> {
        self.countdown.map(|c| c.remaining)
    }

    pub fn is_paused(&self) -> bool {
        matches!(self.phase, Phase::Paused | Phase::Resuming)
    }

    pub fn camera_on(&self) -> bool {
        self.indicator.as_ref().is_some_and(|i| i.camera.is_some())
    }

    pub fn mic_on(&self) -> bool {
        self.indicator.as_ref().is_some_and(|i| i.mic)
    }

    pub fn started_by_agent(&self) -> Option<(&str, &str)> {
        self.indicator.as_ref().and_then(|i| i.started_by.agent())
    }

    /// Screen-reader summary of the recording state (NFR-13).
    pub fn status_label(&self, now: Instant) -> String {
        let t = self.timer_text(now);
        match self.phase {
            Phase::Recording => tr!("Recording, {t}", t = t),
            Phase::Paused => tr!("Paused, {t}", t = t),
            Phase::Resuming => tr!("Resuming, {t}", t = t),
            Phase::Countdown => tr!("Starting soon"),
            Phase::Restarting => tr!("Restarting"),
            Phase::Stopping => tr_ctx!("recording status", "Saving"),
            Phase::Starting => tr_ctx!("recording status", "Starting"),
            Phase::SelfTest => tr!("Testing 60 fps"),
            Phase::Idle => tr!("Not recording"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn started(id: &str, by: Value) -> Value {
        json!({
            "recording_id": id, "capture_id": "rec_abc", "title": "Recording 10.30",
            "state": "recording", "live_at_ms": 0, "by": "user",
            "indicator": {
                "area": { "kind": "region", "display_id": 1, "window_id": null,
                          "bounds": [2560, 400, 1920, 1080], "source_size": [1920, 1080],
                          "scale": 2.0 },
                "output": { "width": 1920, "height": 1080, "fps": 30, "codec": "hevc" },
                "camera": { "shape": "circle", "size": "m", "position": "bottom-right", "mirror": true },
                "mic": true, "system_audio": false, "cursor": true, "started_by": by,
            },
            "warnings": [],
        })
    }

    fn user() -> Value {
        json!({ "kind": "user", "client": null, "tool": null })
    }

    fn status(state: &str, recorded: i64) -> Value {
        json!({ "recording_id": "rec_1", "state": state, "recorded_ms": recorded,
                "mic_level_dbfs": -20.0, "pauses": 0 })
    }

    #[test]
    fn indicator_facts_parse() {
        let i = Indicator::parse(&started("rec_1", user())["indicator"]).unwrap();
        assert_eq!(i.area.kind, AreaKind::Region);
        assert_eq!(i.area.bounds, Some([2560, 400, 1920, 1080]));
        assert_eq!(i.area.scale, 2.0);
        assert_eq!(i.output, Some([1920, 1080]));
        assert_eq!(i.camera.as_ref().unwrap().size, BubbleSize::M);
        assert!(i.mic && !i.system_audio);
        assert_eq!(i.started_by, StartedBy::User);
        // No camera: the field is null.
        let mut j = started("rec_1", user());
        j["indicator"]["camera"] = Value::Null;
        assert!(Indicator::parse(&j["indicator"]).unwrap().camera.is_none());
    }

    /// REC-1.AC5 / REC-4: the countdown digits come from `recording.countdown` and the phase
    /// follows.
    #[test]
    fn countdown_then_started() {
        let t0 = Instant::now();
        let mut s = HudState::default();
        s.begin_start();
        assert_eq!(s.phase, Phase::Starting);
        let c = s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            t0,
        );
        assert_eq!(
            (c, s.phase, s.countdown_digit()),
            (Change::Countdown, Phase::Countdown, Some(3))
        );
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 2, "total": 3, "context": "start"}),
            t0 + ms(1000),
        );
        assert_eq!(s.countdown_digit(), Some(2));
        let c = s.apply_event(
            "recording.started",
            &started("rec_1", user()),
            t0 + ms(3000),
        );
        assert_eq!(c, Change::Started { agent: None });
        assert_eq!((s.phase, s.countdown_digit()), (Phase::Recording, None));
        assert_eq!(s.capture_id.as_deref(), Some("rec_abc"));
    }

    /// REC-5 agent toast: the agent that started it and the tool come from `started_by`.
    #[test]
    fn agent_start_is_reported_once() {
        let mut s = HudState::default();
        let by = json!({"kind": "agent", "client": "claude-desktop", "tool": "start_recording"});
        let c = s.apply_event("recording.started", &started("rec_1", by), Instant::now());
        assert_eq!(
            c,
            Change::Started {
                agent: Some(("claude-desktop".into(), "start_recording".into()))
            }
        );
        assert_eq!(
            s.started_by_agent(),
            Some(("claude-desktop", "start_recording"))
        );
    }

    /// REC-4.AC1: 10 s recorded, 20 s paused, 10 s recorded shows 00:20.
    #[test]
    fn rec4_ac1_timer_counts_recorded_time_only() {
        let t0 = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t0);
        // 10 s later the stats say 10 s; the user pauses.
        for i in 1..=10u64 {
            s.apply_status(&status("recording", i as i64 * 1000), t0 + ms(i * 1000));
        }
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 10_000}),
            t0 + ms(10_050),
        );
        assert_eq!(s.phase, Phase::Paused);
        // 20 s of pause: the timer stays at 00:10.
        assert_eq!(s.timer_text(t0 + ms(30_050)), "00:10");
        s.apply_status(&status("paused", 10_000), t0 + ms(30_000));
        assert_eq!(s.timer_text(t0 + ms(30_100)), "00:10");
        // Resume and record 10 s more.
        s.apply_event(
            "recording.resumed",
            &json!({"recording_id": "rec_1"}),
            t0 + ms(30_200),
        );
        assert_eq!(s.timer_text(t0 + ms(30_200)), "00:10");
        // The engine answers every second; the timer follows it.
        for i in 1..=10u64 {
            s.apply_status(
                &status("recording", 10_000 + i as i64 * 1000),
                t0 + ms(30_200 + i * 1000),
            );
        }
        assert_eq!(s.timer_text(t0 + ms(40_300)), "00:20");
    }

    #[test]
    fn the_timer_freezes_during_the_resume_countdown_and_a_stalled_engine() {
        let t0 = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t0);
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 5000}),
            t0,
        );
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 1, "total": 1, "context": "resume"}),
            t0 + ms(100),
        );
        assert_eq!(s.phase, Phase::Resuming);
        assert!(s.is_paused());
        assert_eq!(s.timer_text(t0 + ms(900)), "00:05");
        // A resumed recording whose stats stopped arriving does not run away.
        s.apply_event(
            "recording.resumed",
            &json!({"recording_id": "rec_1"}),
            t0 + ms(1100),
        );
        assert_eq!(s.timer_text(t0 + Duration::from_secs(3600)), "00:07");
    }

    #[test]
    fn the_timer_never_runs_backwards_between_answers() {
        let t0 = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t0);
        let before = s.recorded_ms(t0 + ms(1000));
        s.apply_status(&status("recording", 950), t0 + ms(1000));
        assert!(s.recorded_ms(t0 + ms(1000)) >= before);
        // A real correction (the engine says much less) is taken.
        s.apply_status(&status("recording", 100), t0 + ms(1100));
        assert_eq!(s.recorded_ms(t0 + ms(1100)), 100);
    }

    #[test]
    fn clock_formatting() {
        assert_eq!(clock_text(0), "00:00");
        assert_eq!(clock_text(42_900), "00:42");
        assert_eq!(clock_text(61_000), "01:01");
        assert_eq!(clock_text(3_723_000), "1:02:03");
        assert_eq!(clock_text(-5), "00:00");
    }

    /// REC-5.AC15: the meter follows the mic level, five bars.
    #[test]
    fn rec5_ac15_mic_meter_bars() {
        assert_eq!(mic_bars(None), 0);
        assert_eq!(mic_bars(Some(-90.0)), 0);
        assert_eq!(mic_bars(Some(-45.0)), 1);
        assert_eq!(mic_bars(Some(-30.0)), 3);
        assert_eq!(mic_bars(Some(-6.0)), 5);
        assert_eq!(mic_bars(Some(0.0)), 5);
        assert_eq!(mic_bars(Some(f64::NEG_INFINITY)), 0);
        let mut last = 0;
        for db in -60..=0 {
            let n = mic_bars(Some(f64::from(db)));
            assert!(n >= last, "monotonic at {db}");
            last = n;
        }
    }

    /// REC-5.AC10: pause then resume swaps the phase back and forth.
    #[test]
    fn rec5_ac10_pause_and_resume_phases() {
        let t0 = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t0);
        assert!(!s.is_paused());
        assert_eq!(
            s.apply_event(
                "recording.paused",
                &json!({"recording_id": "rec_1", "recorded_ms": 1000}),
                t0
            ),
            Change::Updated
        );
        assert!(s.is_paused() && s.pauses == 1);
        s.apply_event("recording.resumed", &json!({"recording_id": "rec_1"}), t0);
        assert!(!s.is_paused() && s.phase == Phase::Recording);
    }

    #[test]
    fn health_and_warnings() {
        let mut s = HudState::default();
        let t = Instant::now();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        assert_eq!(
            s.apply_event(
                "recording.health",
                &json!({"recording_id": "rec_1", "status": "degraded"}),
                t
            ),
            Change::Updated
        );
        assert_eq!(s.health, Health::Degraded);
        assert_eq!(
            s.apply_event(
                "recording.health",
                &json!({"recording_id": "rec_1", "status": "degraded"}),
                t
            ),
            Change::None
        );
        s.apply_event(
            "recording.health",
            &json!({"recording_id": "rec_1", "status": "ok"}),
            t,
        );
        assert_eq!(s.health, Health::Ok);
        for i in 0..5 {
            s.apply_event(
                "recording.warning",
                &json!({"recording_id": "rec_1", "code": format!("c{i}"), "message": "m"}),
                t,
            );
        }
        assert_eq!(s.warnings.len(), MAX_WARNINGS);
        assert_eq!(s.warnings.last().map(|w| w.code.as_str()), Some("c4"));
    }

    /// REC-5.AC2 / AC3: stopped and discarded end everything and reset the model.
    #[test]
    fn stop_discard_and_cancel_end_the_run() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        let c = s.apply_event(
            "recording.stopped",
            &json!({"recording_id": "rec_1", "capture_id": "rec_abc", "duration_s": 42.5, "reason": "user"}),
            t,
        );
        assert_eq!(
            c,
            Change::Ended(Ended::Stopped {
                capture_id: Some("rec_abc".into()),
                duration_s: Some(42.5),
                reason: "user".into(),
                error: None,
                permission: None,
            })
        );
        assert_eq!(s.phase, Phase::Idle);
        // Discard of a running take.
        s.apply_event("recording.started", &started("rec_2", user()), t);
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_2", "capture_id": "rec_x", "reason": "user"}),
            t,
        );
        assert_eq!(
            c,
            Change::Ended(Ended::Discarded {
                reason: "user".into()
            })
        );
        // Esc during the countdown: no capture existed.
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_3", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_3", "capture_id": null, "reason": "countdown_cancelled"}),
            t,
        );
        assert_eq!(c, Change::Ended(Ended::Cancelled));
        assert_eq!(s.phase, Phase::Idle);
    }

    /// A stop that failed says why.
    #[test]
    fn a_failed_stop_carries_the_error() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        let c = s.apply_event(
            "recording.stopped",
            &json!({"recording_id": "rec_1", "reason": "error", "error": {"message": "disk full"}}),
            t,
        );
        assert!(
            matches!(c, Change::Ended(Ended::Stopped { error: Some(e), .. }) if e == "disk full")
        );
    }

    /// REC-5.AC12: Restart keeps the windows, then a new countdown and a new id follow.
    #[test]
    fn rec5_ac12_restart_keeps_the_windows_and_resets_the_take() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        s.apply_status(&status("recording", 8000), t);
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "capture_id": "rec_x", "reason": "restart"}),
            t,
        );
        assert_eq!(c, Change::Restarting);
        assert_eq!(s.phase, Phase::Restarting);
        assert!(s.indicator.is_some());
        // A late status answer does not undo the restart.
        s.apply_status(&status("stopping", 8000), t);
        assert_eq!(s.phase, Phase::Restarting);
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_2", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        assert_eq!(s.recording_id.as_deref(), Some("rec_2"));
        assert!(
            s.indicator.is_some(),
            "the area stays while counting down again"
        );
        let c = s.apply_event("recording.started", &started("rec_2", user()), t);
        assert_eq!(c, Change::Started { agent: None });
        assert_eq!(s.recorded_ms(t), 0);
    }

    #[test]
    fn events_of_another_run_are_ignored() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        assert_eq!(
            s.apply_event(
                "recording.stopped",
                &json!({"recording_id": "rec_other"}),
                t
            ),
            Change::None
        );
        assert_eq!(s.phase, Phase::Recording);
    }

    /// `recording.starting` carries the indicator before the countdown: the model knows the area
    /// and counts the run as on screen, for an agent's start too.
    #[test]
    fn recording_starting_brings_the_indicator_early() {
        let t = Instant::now();
        let mut s = HudState::default();
        let mut e = started("rec_1", user());
        e["state"] = json!("starting");
        let c = s.apply_event("recording.starting", &e, t);
        assert_eq!(c, Change::Updated);
        assert_eq!(s.phase, Phase::Starting);
        assert!(s.indicator.is_some() && s.phase.is_active());
        assert_eq!(s.recording_id.as_deref(), Some("rec_1"));
        // The countdown that follows keeps the indicator and moves on.
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        assert_eq!(s.phase, Phase::Countdown);
        assert!(s.indicator.is_some());
        // A start that then fails (the engine says so) resets everything.
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "reason": "start_failed", "error": "no disk"}),
            t,
        );
        assert!(matches!(c, Change::Ended(Ended::StartFailed { .. })));
        assert_eq!(s.phase, Phase::Idle);
        // During a restart the area stays.
        let mut r = HudState::default();
        r.apply_event("recording.started", &started("rec_1", user()), t);
        r.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "capture_id": "c", "reason": "restart"}),
            t,
        );
        r.apply_event("recording.starting", &started("rec_2", user()), t);
        assert_eq!(
            (r.phase, r.recording_id.as_deref()),
            (Phase::Restarting, Some("rec_2"))
        );
        assert!(r.indicator.is_some());
    }

    #[test]
    fn a_refused_start_returns_to_idle() {
        let mut s = HudState::default();
        s.begin_start();
        s.begin_start();
        s.abort_start();
        assert_eq!(s.phase, Phase::Idle);
        // After events arrived the run is real and abort_start leaves it alone.
        s.begin_start();
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            Instant::now(),
        );
        s.abort_start();
        assert_eq!(s.phase, Phase::Countdown);
    }

    /// R4-1: a start that fails after the countdown began (disk check, encoder, display gone)
    /// resets the model, so the countdown window, the tray and the Record button recover.
    #[test]
    fn r4_1_a_start_that_fails_during_the_countdown_resets_everything() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.begin_start();
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        assert_eq!(s.phase, Phase::Countdown);
        assert!(s.fail_start());
        assert_eq!(s.phase, Phase::Idle);
        assert!(s.countdown.is_none() && s.recording_id.is_none());
        assert!(!s.countdown_wanted());
        // A refused start before any event too.
        s.begin_start();
        assert!(s.fail_start());
        assert_eq!(s.phase, Phase::Idle);
    }

    /// R4-1: a Restart that fails (the take is already gone) resets as well.
    #[test]
    fn r4_1_a_failed_restart_resets() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "capture_id": "rec_x", "reason": "restart"}),
            t,
        );
        assert_eq!(s.phase, Phase::Restarting);
        assert!(s.fail_start());
        assert_eq!(s.phase, Phase::Idle);
        assert!(s.indicator.is_none());
    }

    /// R4-1: once `recording.started` arrived an error is about something else.
    #[test]
    fn r4_1_a_live_take_survives_a_late_error() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        assert!(!s.fail_start());
        assert_eq!(s.phase, Phase::Recording);
    }

    /// R4-1: the engine's own `recording.discarded { reason: start_failed }`.
    #[test]
    fn r4_1_start_failed_event_ends_the_run_once() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.begin_start();
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "capture_id": null, "reason": "start_failed",
                    "error": {"code": "disk_full", "message": "Not enough space"}}),
            t,
        );
        assert_eq!(
            c,
            Change::Ended(Ended::StartFailed {
                code: Some("disk_full".into()),
                message: "Not enough space".into()
            })
        );
        assert_eq!(s.phase, Phase::Idle);
        // The same event again (or after the call's error already reset the model) is ignored.
        let again = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_1", "reason": "start_failed"}),
            t,
        );
        assert_eq!(again, Change::None);
        // A plain string error works too.
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_2", "remaining": 1, "total": 1, "context": "start"}),
            t,
        );
        let c = s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_2", "reason": "start_failed", "error": "boom"}),
            t,
        );
        assert!(
            matches!(c, Change::Ended(Ended::StartFailed { message, .. }) if message == "boom")
        );
    }

    /// R4-2: the resume countdown closes when the resume happens or the pause returns, and Esc on it
    /// cancels the resume instead of deleting the take.
    #[test]
    fn r4_2_resume_countdown_lifecycle_and_esc() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 5000}),
            t,
        );
        assert!(!s.countdown_wanted());
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 1, "total": 1, "context": "resume"}),
            t,
        );
        assert!(s.countdown_wanted());
        assert_eq!(s.esc_action(), EscAction::CancelResume);
        s.apply_event("recording.resumed", &json!({"recording_id": "rec_1"}), t);
        assert!(!s.countdown_wanted(), "the window must close on resume");
        assert_eq!(s.esc_action(), EscAction::Ignore);
        // The user cancelled the resume: paused again, window closes.
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 6000}),
            t,
        );
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 1, "total": 1, "context": "resume"}),
            t,
        );
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 6000}),
            t,
        );
        assert!(!s.countdown_wanted());
        // A lost `resumed` event: the poll says recording, the countdown is dropped.
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 1, "total": 1, "context": "resume"}),
            t,
        );
        s.apply_status(&status("recording", 6100), t);
        assert!(!s.countdown_wanted());
    }

    /// R4-2: Esc discards only a start countdown with no capture yet.
    #[test]
    fn r4_2_esc_cancels_only_a_start_countdown() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3, "context": "start"}),
            t,
        );
        assert_eq!(s.esc_action(), EscAction::CancelStart);
        // A countdown while a capture exists (never the start one) is not a start cancel.
        s.capture_id = Some("rec_abc".into());
        assert_eq!(s.esc_action(), EscAction::Ignore);
        let mut r = HudState::default();
        r.apply_event("recording.started", &started("rec_1", user()), t);
        assert_eq!(r.esc_action(), EscAction::Ignore);
        assert_eq!(HudState::default().esc_action(), EscAction::Ignore);
    }

    /// R4-3: an engine that says idle while the model shows a take, twice in a row, resets the
    /// model (a lost `recording.stopped`); one answer does not; a restart gets longer.
    #[test]
    fn r4_3_idle_engine_resets_a_stranded_model() {
        let t = Instant::now();
        let idle = json!({"state": "idle", "recording_id": null, "last": null});
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        assert_eq!(s.apply_status(&idle, t), Change::None);
        // A real answer in between starts the count over.
        s.apply_status(&status("recording", 1000), t);
        assert_eq!(s.apply_status(&idle, t), Change::None);
        assert_eq!(s.apply_status(&idle, t), Change::Ended(Ended::Lost));
        assert_eq!(s.phase, Phase::Idle);
        // Idle stays idle, and a Starting model is left alone.
        assert_eq!(s.apply_status(&idle, t), Change::None);
        s.begin_start();
        for _ in 0..5 {
            assert_eq!(s.apply_status(&idle, t), Change::None);
        }
        assert_eq!(s.phase, Phase::Starting);
        // `stopped` and `failed` count as idle.
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_2", user()), t);
        s.apply_status(&json!({"state": "stopped"}), t);
        assert!(matches!(
            s.apply_status(&json!({"state": "failed"}), t),
            Change::Ended(Ended::Lost)
        ));
        // A run the engine announced (`recording.starting`) that then vanished without a word.
        let mut a = HudState::default();
        a.apply_event("recording.starting", &started("rec_9", user()), t);
        for _ in 0..IDLE_POLLS_TO_RESET_RESTARTING - 1 {
            assert_eq!(a.apply_status(&idle, t), Change::None);
        }
        assert_eq!(a.apply_status(&idle, t), Change::Ended(Ended::Lost));
        // A restart waits longer.
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_3", user()), t);
        s.apply_event(
            "recording.discarded",
            &json!({"recording_id": "rec_3", "capture_id": "c", "reason": "restart"}),
            t,
        );
        for _ in 0..IDLE_POLLS_TO_RESET_RESTARTING - 1 {
            assert_eq!(s.apply_status(&idle, t), Change::None);
        }
        assert_eq!(s.apply_status(&idle, t), Change::Ended(Ended::Lost));
    }

    /// R4-3: stats that trail `recording.stopped` do not flip an idle model back to Recording.
    #[test]
    fn r4_3_late_stats_do_not_revive_an_idle_model() {
        let t = Instant::now();
        let mut s = HudState::default();
        s.apply_event("recording.started", &started("rec_1", user()), t);
        s.apply_event(
            "recording.stopped",
            &json!({"recording_id": "rec_1", "capture_id": "c", "duration_s": 3.0}),
            t,
        );
        let late = json!({"recording_id": "rec_1", "state": "recording", "recorded_ms": 3000});
        assert_eq!(s.apply_event("recording.stats", &late, t), Change::None);
        assert_eq!(s.apply_status(&late, t), Change::None);
        assert_eq!(s.phase, Phase::Idle);
        // A late discard or resume countdown of the finished run is ignored as well.
        assert_eq!(
            s.apply_event(
                "recording.discarded",
                &json!({"recording_id": "rec_1", "reason": "user", "capture_id": "c"}),
                t
            ),
            Change::None
        );
    }

    /// R4-3: after a lagged event stream a status answer adopts a run the model never saw.
    #[test]
    fn r4_3_resync_adopts_a_missed_run() {
        let t = Instant::now();
        let mut s = HudState::default();
        let mut v = started("rec_1", user());
        v["recorded_ms"] = json!(4000);
        let c = s.adopt_status(&v, t);
        assert_eq!(c, Change::Started { agent: None });
        assert_eq!(s.phase, Phase::Recording);
        assert_eq!(s.recorded_ms(t), 4000);
        // A model that already shows a run keeps it.
        assert_eq!(s.adopt_status(&v, t), Change::None);
        // Idle engine: nothing to adopt.
        let mut s = HudState::default();
        assert_eq!(s.adopt_status(&json!({"state": "idle"}), t), Change::None);
    }

    /// R4-9 / REC-5.AC5 / NFR-11: the 60 fps self-test shows the indicator and the tray, and
    /// ends with its verdict.
    #[test]
    fn r4_9_selftest_has_an_indicator_and_ends() {
        let t = Instant::now();
        let mut s = HudState::default();
        let ind = started("x", user())["indicator"].clone();
        let c = s.apply_event(
            "recording.selftest",
            &json!({"selftest_id": "st_1", "state": "started", "total_s": 10.0,
                    "remaining_s": 10.0, "indicator": ind}),
            t,
        );
        assert_eq!(c, Change::SelfTest);
        assert_eq!(s.phase, Phase::SelfTest);
        assert!(s.indicator.is_some() && s.phase.is_active());
        assert_eq!(s.status_label(t), "Testing 60 fps");
        s.apply_event(
            "recording.selftest",
            &json!({"selftest_id": "st_1", "state": "running", "remaining_s": 7.2}),
            t,
        );
        assert_eq!(s.selftest_remaining_s, Some(7.2));
        // The poll agrees.
        let st = json!({"state": "selftest", "recording_id": null,
                        "selftest": {"selftest_id": "st_1", "remaining_s": 6.0}});
        s.apply_status(&st, t);
        assert_eq!(
            (s.phase, s.selftest_remaining_s),
            (Phase::SelfTest, Some(6.0))
        );
        // A recording status never turns it into a recording.
        assert_eq!(s.apply_status(&status("recording", 10), t), Change::None);
        assert_eq!(s.phase, Phase::SelfTest);
        let c = s.apply_event(
            "recording.selftest",
            &json!({"selftest_id": "st_1", "state": "passed"}),
            t,
        );
        assert_eq!(
            c,
            Change::Ended(Ended::SelfTest {
                state: "passed".into(),
                message: None
            })
        );
        assert_eq!(s.phase, Phase::Idle);
        // A lost end event: the engine goes idle, the model follows.
        s.apply_event(
            "recording.selftest",
            &json!({"selftest_id": "st_2", "state": "started", "indicator": ind}),
            t,
        );
        let idle = json!({"state": "idle"});
        s.apply_status(&idle, t);
        assert!(matches!(
            s.apply_status(&idle, t),
            Change::Ended(Ended::SelfTest { .. })
        ));
        // Adoption from a status answer after a lagged stream.
        let c = s.adopt_status(
            &json!({"state": "selftest", "selftest": {"selftest_id": "st_3", "remaining_s": 5.0,
                    "indicator": ind}}),
            t,
        );
        assert_eq!(c, Change::SelfTest);
    }

    #[test]
    fn status_labels_for_screen_readers() {
        let t = Instant::now();
        let mut s = HudState::default();
        assert_eq!(s.status_label(t), "Not recording");
        s.apply_event("recording.started", &started("rec_1", user()), t);
        assert_eq!(s.status_label(t), "Recording, 00:00");
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "rec_1", "recorded_ms": 42_000}),
            t,
        );
        assert_eq!(s.status_label(t), "Paused, 00:42");
    }
}
