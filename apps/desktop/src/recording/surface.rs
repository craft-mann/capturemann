//! Where the recording indicator and controls are drawn (REC-5): floating windows over the
//! screen, or a banner inside the Studio window.
//!
//! On macOS and Windows the HUD, the countdown disc and the area brackets are separate windows
//! that the system keeps out of the recording (`sharingType`, `WDA_EXCLUDEFROMCAPTURE`). Linux has
//! neither: the screen is recorded through the desktop's portal, which hands over every pixel, so
//! a floating HUD would be in `screen.mp4`; and on Wayland a client cannot ask for always-on-top
//! or for a place on the screen. So on Linux the controls are a banner in the Studio window
//! ([`Banner`]), next to the tray icon (recording state, Stop and Pause in its menu) and the
//! desktop's own screen-share indicator. X11 can opt back into the floating HUD with
//! `CM_LINUX_FLOATING_HUD=1` (it will then be visible in a recording of that screen).
//!
//! Display-free: `ui::recording::banner` draws what [`banner`] decides.

use std::time::Instant;

use cm_i18n::{tr, tr_ctx};

use crate::permissions::Os;
use crate::recording::model::{HudState, Phase};

/// The variable that lets an X11 session keep the floating HUD on Linux.
pub const FLOATING_ENV: &str = "CM_LINUX_FLOATING_HUD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Separate always-on-top windows (HUD, countdown disc, area brackets).
    Floating,
    /// A banner in the Studio window; no window over the recorded screen.
    InWindow,
}

/// The decision, from the facts.
pub fn surface_for(os: Os, wayland: bool, floating_opt_in: bool) -> Surface {
    match os {
        Os::Linux if wayland || !floating_opt_in => Surface::InWindow,
        _ => Surface::Floating,
    }
}

/// The surface for this process.
pub fn surface() -> Surface {
    let env = |k: &str| std::env::var(k).ok();
    let wayland = crate::platform_window::is_wayland_session(
        env("XDG_SESSION_TYPE").as_deref(),
        env("WAYLAND_DISPLAY").as_deref(),
        env("DISPLAY").as_deref(),
    );
    let opt_in = env(FLOATING_ENV).is_some_and(|v| v.trim() == "1");
    surface_for(Os::current(), wayland, opt_in)
}

/// Whether the full-display indicator (the dimmed scrim, the corner brackets and the `rec`
/// outline) is drawn with the floating windows. Not on Linux: its window covers the whole screen
/// and is transparent, which an X11 session without a compositing manager draws as a black
/// rectangle over everything, and the size it marks is the plan's hint, not the screen the
/// desktop's dialog picked.
pub fn shows_area_indicator(os: Os) -> bool {
    os != Os::Linux
}

/// What the banner offers in one phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    /// Short state word: "Recording", "Paused", "Starting in 3".
    pub label: String,
    /// The recorded time, while there is one.
    pub timer: Option<String>,
    /// The red dot blinks (recording) or rests (everything else).
    pub live: bool,
    /// Pause (`false`) or Resume (`true`) is offered.
    pub pause: Option<bool>,
    /// The microphone is in this take: `Some(muted)`.
    pub mic: Option<bool>,
    /// Stop is offered (the take is not already being saved).
    pub stop: bool,
    /// Skip the countdown and Cancel are offered.
    pub countdown: bool,
    /// Discard this take is offered.
    pub discard: bool,
    /// The line under the controls: what else shows the recording, or (while it starts) what the
    /// desktop may ask.
    pub hint: String,
}

/// The banner for a model state, or `None` when nothing is running (the banner is not drawn).
pub fn banner(hud: &HudState, now: Instant) -> Option<Banner> {
    let phase = hud.phase;
    if phase == Phase::Idle {
        return None;
    }
    let timer = matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming)
        .then(|| hud.timer_text(now));
    let running = matches!(phase, Phase::Recording | Phase::Paused);
    let digit = hud.countdown_digit();
    let label = match phase {
        Phase::Recording => tr_ctx!("recording banner", "Recording"),
        Phase::Paused => tr_ctx!("recording banner", "Paused"),
        Phase::Countdown | Phase::Resuming | Phase::Restarting => match digit {
            Some(n) => tr!("Starting in {n}", n = n),
            None => tr!("Starting soon"),
        },
        Phase::Starting => tr_ctx!("recording status", "Starting"),
        Phase::Stopping => tr_ctx!("recording status", "Saving"),
        Phase::SelfTest => tr!("Testing 60 fps"),
        Phase::Idle => return None,
    };
    Some(Banner {
        label,
        timer,
        live: phase == Phase::Recording,
        pause: running.then(|| hud.is_paused()),
        mic: (running && hud.mic_on()).then(|| hud.mic_is_muted()),
        stop: matches!(
            phase,
            Phase::Recording | Phase::Paused | Phase::Resuming | Phase::SelfTest
        ),
        countdown: matches!(phase, Phase::Countdown | Phase::Resuming) && digit.is_some(),
        discard: running,
        hint: if phase == Phase::Starting {
            tr!("Your desktop may ask which screen to share. Choose the screen and press Share.")
        } else {
            tr!("Your desktop and the tray icon also show that the screen is being recorded. Stop from either one if this window is hidden.")
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn state(phase: &str) -> HudState {
        let mut h = HudState::default();
        let now = Instant::now();
        match phase {
            "starting" => h.begin_start(),
            _ => {
                h.apply_event(
                    "recording.started",
                    &json!({"recording_id": "rec_1", "capture_id": "rec_abc", "live_at_ms": 0,
                            "indicator": {"area": {"kind": "display"}, "mic": true}}),
                    now,
                );
                h.apply_status(
                    &json!({"state": "recording", "recording_id": "rec_1", "recorded_ms": 65000}),
                    now,
                );
                match phase {
                    "paused" => {
                        h.apply_event(
                            "recording.paused",
                            &json!({"recording_id": "rec_1", "recorded_ms": 65000}),
                            now,
                        );
                    }
                    "stopping" => {
                        h.apply_status(&json!({"state": "stopping", "recording_id": "rec_1"}), now);
                    }
                    _ => {}
                }
            }
        }
        h
    }

    /// Linux never floats the HUD over the recorded screen, except an X11 session that asks.
    #[test]
    fn linux_uses_the_banner_unless_x11_opts_in() {
        use Surface::*;
        assert_eq!(surface_for(Os::Mac, false, false), Floating);
        assert_eq!(surface_for(Os::Windows, false, true), Floating);
        assert_eq!(
            surface_for(Os::Linux, false, false),
            InWindow,
            "X11 default"
        );
        assert_eq!(surface_for(Os::Linux, true, false), InWindow, "Wayland");
        assert_eq!(surface_for(Os::Linux, false, true), Floating, "X11 opt-in");
        assert_eq!(
            surface_for(Os::Linux, true, true),
            InWindow,
            "Wayland cannot"
        );
    }

    #[test]
    fn only_linux_leaves_out_the_full_display_indicator() {
        assert!(shows_area_indicator(Os::Mac) && shows_area_indicator(Os::Windows));
        assert!(!shows_area_indicator(Os::Linux));
    }

    #[test]
    fn nothing_running_draws_no_banner() {
        assert!(banner(&HudState::default(), Instant::now()).is_none());
    }

    #[test]
    fn recording_and_paused_offer_pause_stop_and_discard() {
        let now = Instant::now();
        let rec = banner(&state("recording"), now).unwrap();
        assert_eq!(rec.label, "Recording");
        assert!(rec.timer.as_deref().unwrap().starts_with("01:0"), "{rec:?}");
        assert!(rec.live && rec.stop && rec.discard);
        assert_eq!(rec.pause, Some(false));
        assert_eq!(rec.mic, Some(false));
        let paused = banner(&state("paused"), now + Duration::from_secs(1)).unwrap();
        assert_eq!(paused.label, "Paused");
        assert_eq!(paused.pause, Some(true));
        assert!(!paused.live);
    }

    #[test]
    fn starting_and_saving_have_no_pause_and_saving_has_no_stop() {
        let now = Instant::now();
        let starting = banner(&state("starting"), now).unwrap();
        assert_eq!(starting.label, "Starting");
        assert!(starting.pause.is_none() && starting.timer.is_none() && !starting.stop);
        assert!(starting.hint.contains("may ask which screen to share"));
        assert!(rec_hint_is_the_indicator_line(
            &banner(&state("recording"), now).unwrap()
        ));
        let saving = banner(&state("stopping"), now).unwrap();
        assert_eq!(saving.label, "Saving");
        assert!(!saving.stop && !saving.discard);
    }

    fn rec_hint_is_the_indicator_line(b: &Banner) -> bool {
        b.hint.contains("tray icon also show")
    }

    #[test]
    fn the_countdown_shows_its_digit_with_skip_and_cancel() {
        let mut h = HudState::default();
        let now = Instant::now();
        h.begin_start();
        h.apply_event(
            "recording.countdown",
            &json!({"recording_id": "rec_1", "remaining": 3, "total": 3}),
            now,
        );
        let b = banner(&h, now).unwrap();
        assert_eq!(b.label, "Starting in 3");
        assert!(b.countdown && b.pause.is_none() && !b.stop);
    }
}
