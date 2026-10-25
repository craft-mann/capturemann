//! The tray's recording state (REC-5.AC7, CAP-2) and what the global recording hotkeys do.
//!
//! The tray shows the kit's recording icon from the first countdown until the take ends, with a
//! tooltip carrying the elapsed time, and the menu offers Stop and Pause/Resume while recording.
//! Pure decisions; `platform::TrayHandle` applies them.

use std::time::Instant;

use cm_i18n::tr;

use crate::platform::TrayState;
use crate::recording::model::{clock_text, HudState, Phase};

/// What the tray should look like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayPlan {
    pub state: TrayState,
    pub tooltip: String,
    /// "Stop recording" is enabled.
    pub stop_enabled: bool,
    /// "Pause recording" / "Resume recording" is enabled.
    pub pause_enabled: bool,
    pub pause_label: String,
    /// "Record…" is enabled (nothing running).
    pub record_enabled: bool,
}

/// The tray icon for a phase. A take on screen keeps the recording icon while it starts, counts
/// down and stops; Paused and Resuming show the paused icon (the redesign's Paused state). When
/// nothing is running, `attention` (a failed upload, a needed reconnect) shows the Attention icon.
pub fn icon_state(phase: Phase, attention: bool) -> TrayState {
    match phase {
        Phase::Paused | Phase::Resuming => TrayState::Paused,
        p if p.is_active() => TrayState::Recording,
        _ if attention => TrayState::Attention,
        _ => TrayState::Idle,
    }
}

/// The tray for a model state at `now`, with no attention signal.
pub fn plan(state: &HudState, now: Instant) -> TrayPlan {
    plan_with(state, now, false)
}

/// [`plan`] with the attention signal: `UploadTracker::needs_attention` (an upload failed or
/// paused as `auth_expired` and not yet dismissed, retried or reconnected). `ui::upload_ui`
/// re-applies the tray whenever the tracker changes. TODO(design): the library going offline is
/// not part of the signal.
pub fn plan_with(state: &HudState, now: Instant, attention: bool) -> TrayPlan {
    let t = clock_text(state.recorded_ms(now));
    let active = state.phase.is_active();
    let live = matches!(
        state.phase,
        Phase::Recording | Phase::Paused | Phase::Resuming
    );
    TrayPlan {
        state: icon_state(state.phase, attention),
        tooltip: match state.phase {
            Phase::Idle => "CaptureMann".to_owned(), // i18n-ignore: the product name
            Phase::Starting | Phase::Countdown | Phase::Restarting => {
                tr!("CaptureMann \u{2014} Recording starts soon")
            }
            Phase::Recording => tr!("CaptureMann \u{2014} Recording {t}", t = t),
            Phase::Paused | Phase::Resuming => tr!("CaptureMann \u{2014} Paused {t}", t = t),
            Phase::Stopping => tr!("CaptureMann \u{2014} Saving recording"),
            Phase::SelfTest => tr!("CaptureMann \u{2014} Testing 60 fps"),
        },
        stop_enabled: active && state.phase != Phase::Stopping,
        pause_enabled: live,
        pause_label: if state.is_paused() {
            tr!("Resume recording")
        } else {
            tr!("Pause recording")
        },
        record_enabled: !active,
    }
}

/// What the start/stop hotkey (⌥⇧R) does (CAP-2: it toggles).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleAction {
    /// Idle: start with the remembered options.
    Start,
    /// A take is on screen (counting down, recording or paused): stop it. During the countdown
    /// this cancels it.
    Stop,
    /// Already stopping or starting: ignore the repeat.
    Ignore,
}

pub fn toggle_action(phase: Phase) -> ToggleAction {
    match phase {
        Phase::Idle => ToggleAction::Start,
        Phase::Countdown | Phase::Recording | Phase::Paused | Phase::Resuming | Phase::SelfTest => {
            ToggleAction::Stop
        }
        Phase::Starting | Phase::Restarting | Phase::Stopping => ToggleAction::Ignore,
    }
}

/// What the pause hotkey (⌥⇧P) does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseAction {
    Pause,
    Resume,
    /// REC-4.AC4: pausing during the countdown cancels it.
    CancelCountdown,
    Ignore,
}

pub fn pause_action(phase: Phase) -> PauseAction {
    match phase {
        Phase::Recording => PauseAction::Pause,
        Phase::Paused => PauseAction::Resume,
        // The resume countdown too: it cancels the resume and leaves the take paused (R4-2).
        Phase::Countdown | Phase::Resuming => PauseAction::CancelCountdown,
        _ => PauseAction::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn live(now: Instant) -> HudState {
        let mut s = HudState::default();
        s.apply_event(
            "recording.started",
            &json!({"recording_id": "r", "capture_id": "c", "indicator": {"area": {"kind": "display"}}}),
            now,
        );
        s
    }

    /// REC-5.AC7: the recording icon while a take is on screen, the template icon otherwise.
    #[test]
    fn rec5_ac7_tray_follows_the_phase() {
        let t = Instant::now();
        let idle = plan(&HudState::default(), t);
        assert_eq!(idle.state, TrayState::Idle);
        assert_eq!(idle.tooltip, "CaptureMann");
        assert!(idle.record_enabled && !idle.stop_enabled && !idle.pause_enabled);

        let mut s = HudState::default();
        s.begin_start();
        let p = plan(&s, t);
        assert_eq!(p.state, TrayState::Recording);
        assert!(p.stop_enabled && !p.pause_enabled && !p.record_enabled);

        let mut s = live(t);
        let at = t + std::time::Duration::from_secs(42);
        s.apply_status(&json!({"state": "recording", "recorded_ms": 42_000}), at);
        let p = plan(&s, at);
        assert_eq!(p.state, TrayState::Recording);
        assert_eq!(p.tooltip, "CaptureMann \u{2014} Recording 00:42");
        assert!(p.stop_enabled && p.pause_enabled);
        assert_eq!(p.pause_label, "Pause recording");
    }

    /// Redesign: Paused and Resuming show the paused icon, Starting, Countdown and Stopping keep
    /// the recording icon, Attention only shows while nothing is running.
    #[test]
    fn icon_state_mapping() {
        for p in [Phase::Paused, Phase::Resuming] {
            assert_eq!(icon_state(p, false), TrayState::Paused, "{p:?}");
            assert_eq!(icon_state(p, true), TrayState::Paused, "{p:?}");
        }
        for p in [
            Phase::Starting,
            Phase::Countdown,
            Phase::Restarting,
            Phase::Recording,
            Phase::Stopping,
            Phase::SelfTest,
        ] {
            assert_eq!(icon_state(p, false), TrayState::Recording, "{p:?}");
            assert_eq!(icon_state(p, true), TrayState::Recording, "{p:?}");
        }
        assert_eq!(icon_state(Phase::Idle, false), TrayState::Idle);
        assert_eq!(icon_state(Phase::Idle, true), TrayState::Attention);
        let t = Instant::now();
        assert_eq!(
            plan_with(&HudState::default(), t, true).state,
            TrayState::Attention
        );
        assert_eq!(plan(&HudState::default(), t).state, TrayState::Idle);
        let mut s = live(t);
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "r", "recorded_ms": 1_000}),
            t,
        );
        assert_eq!(plan(&s, t).state, TrayState::Paused);
        assert!(TrayState::Paused.take_on_screen() && !TrayState::Attention.take_on_screen());
    }

    #[test]
    fn paused_tray_offers_resume_and_freezes_the_time() {
        let t = Instant::now();
        let mut s = live(t);
        s.apply_event(
            "recording.paused",
            &json!({"recording_id": "r", "recorded_ms": 12_000}),
            t,
        );
        let p = plan(&s, t + std::time::Duration::from_secs(60));
        assert_eq!(p.tooltip, "CaptureMann \u{2014} Paused 00:12");
        assert_eq!(p.pause_label, "Resume recording");
    }

    #[test]
    fn stopping_cannot_be_stopped_again() {
        let t = Instant::now();
        let mut s = live(t);
        s.phase = Phase::Stopping;
        let p = plan(&s, t);
        assert!(!p.stop_enabled && !p.pause_enabled);
        assert_eq!(p.state, TrayState::Recording);
    }

    /// CAP-2: the shortcut toggles start and stop.
    #[test]
    fn cap2_toggle_hotkey() {
        assert_eq!(toggle_action(Phase::Idle), ToggleAction::Start);
        for p in [
            Phase::Countdown,
            Phase::Recording,
            Phase::Paused,
            Phase::Resuming,
            Phase::SelfTest,
        ] {
            assert_eq!(toggle_action(p), ToggleAction::Stop, "{p:?}");
        }
        for p in [Phase::Starting, Phase::Restarting, Phase::Stopping] {
            assert_eq!(toggle_action(p), ToggleAction::Ignore, "{p:?}");
        }
    }

    /// REC-4.AC4: the pause shortcut cancels a running countdown.
    #[test]
    fn rec4_ac4_pause_hotkey() {
        assert_eq!(pause_action(Phase::Recording), PauseAction::Pause);
        assert_eq!(pause_action(Phase::Paused), PauseAction::Resume);
        assert_eq!(pause_action(Phase::Countdown), PauseAction::CancelCountdown);
        assert_eq!(pause_action(Phase::Idle), PauseAction::Ignore);
        assert_eq!(pause_action(Phase::Resuming), PauseAction::CancelCountdown);
        assert_eq!(pause_action(Phase::SelfTest), PauseAction::Ignore);
    }

    /// I18N P2: the tooltip keeps the clock in every language.
    #[test]
    fn i18n_tooltip_keeps_the_clock() {
        use cm_i18n::{with_language, Lang};
        let now = Instant::now();
        let s = live(now);
        let english = plan(&s, now).tooltip;
        assert!(
            english.starts_with("CaptureMann \u{2014} Recording "),
            "{english}"
        );
        let clock = english.rsplit(' ').next().unwrap().to_owned();
        let pseudo = with_language(Lang::EnXa, || plan(&s, now).tooltip);
        assert!(pseudo.contains(&clock), "{pseudo}");
        assert_ne!(pseudo, english);
    }
}
