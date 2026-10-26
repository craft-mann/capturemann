//! The recording HUD's layout decisions (redesign R18), display-free: the order of the controls,
//! which icon each state shows, and how wide the bar is (the window is exactly the bar, so the
//! sizes here are what `ui::recording::hud` draws and what `geometry` snaps and places).
//!
//! Order (kit): Stop (red, `cm-stop`, with its label), the timer, a divider, Pause or Resume, Mic,
//! Camera, Draw (disabled, "Coming soon"), a divider, Restart, Discard, then the agent chip when an
//! agent started the take. Hints, the "Dropping frames" and warning chips and the self-test bar
//! stay in the strip above the bar.
//!
//! An audio recording (VOC-1, board V2) has a compact bar: Stop, the timer, the 14-bar mic meter,
//! a divider, Pause or Resume, Mute mic, Marker, Discard, then the agent chip. No camera, Draw or
//! Restart (there is no picture to redo or draw on).

use cm_i18n::{tr, tr_ctx};

/// A control of the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudControl {
    Stop,
    Timer,
    Pause,
    Mic,
    Camera,
    Draw,
    Restart,
    Discard,
    AgentChip,
    /// The 14-bar mic level next to the timer (audio HUD).
    Meter,
    /// "Marker" with its key hint (audio HUD).
    Marker,
}

/// One slot of the bar, left to right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudItem {
    Control(HudControl),
    Divider,
}

/// The bar's slots, left to right. The agent chip is only there when an agent started the take.
pub fn hud_items(agent: bool) -> Vec<HudItem> {
    use HudControl::*;
    let mut v = vec![
        HudItem::Control(Stop),
        HudItem::Control(Timer),
        HudItem::Divider,
        HudItem::Control(Pause),
        HudItem::Control(Mic),
        HudItem::Control(Camera),
        HudItem::Control(Draw),
        HudItem::Divider,
        HudItem::Control(Restart),
        HudItem::Control(Discard),
    ];
    if agent {
        v.push(HudItem::Control(AgentChip));
    }
    v
}

/// The bar's slots of an audio recording, left to right (board V2).
pub fn hud_items_audio(agent: bool) -> Vec<HudItem> {
    use HudControl::*;
    let mut v = vec![
        HudItem::Control(Stop),
        HudItem::Control(Timer),
        HudItem::Control(Meter),
        HudItem::Divider,
        HudItem::Control(Pause),
        HudItem::Control(Mic),
        HudItem::Control(Marker),
        HudItem::Control(Discard),
    ];
    if agent {
        v.push(HudItem::Control(AgentChip));
    }
    v
}

/// The slots of the bar for a screen (`audio` false) or an audio recording.
pub fn hud_items_for(audio: bool, agent: bool) -> Vec<HudItem> {
    if audio {
        hud_items_audio(agent)
    } else {
        hud_items(agent)
    }
}

/// The bars of the audio HUD's mic meter and their size (board V2: 14 bars of 4 px, 2 px apart,
/// 22 px tall, 6 px padding each side).
pub const METER_BARS: usize = 14;
pub const METER_BAR_W: f64 = 4.0;
pub const METER_GAP: f64 = 2.0;
pub const METER_H: f64 = 22.0;
pub const METER_PAD: f64 = 6.0;
/// The meter's slot: the bars, the gaps between them and the padding.
pub const METER_W: f64 =
    METER_BARS as f64 * METER_BAR_W + (METER_BARS - 1) as f64 * METER_GAP + 2.0 * METER_PAD;
/// The Marker button: 12 px padding, an 18 px icon, 6 px gap, "Marker" at 13 px and the key hint.
pub const MARKER_W: f64 = 124.0;

/// Bar heights (px) of the meter for the level `bars` lit of [`METER_BARS`]: the board's
/// skyline, so a lit meter looks like speech.
pub const METER_SHAPE: [u8; METER_BARS] = [6, 9, 14, 18, 12, 16, 20, 15, 11, 17, 13, 8, 10, 6];

/// How many of the 14 bars are lit for a peak in dBFS: bar `n` (from 1) lights from
/// `-52 + 3.5 (n - 1)` dBFS, so silence lights none and a loud voice around -6 dBFS lights all.
pub fn meter_lit(dbfs: Option<f64>) -> usize {
    let Some(db) = dbfs.filter(|d| d.is_finite()) else {
        return 0;
    };
    (1..=METER_BARS)
        .filter(|n| db >= -52.0 + 3.5 * (*n as f64 - 1.0))
        .count()
}

/// The Stop button's icon.
pub const STOP_ICON: &str = "cm-stop";
/// The label next to it.
pub fn stop_label() -> String {
    tr_ctx!("recording hud", "Stop")
}
/// Pause while recording, Resume (play) while paused. The kit has no plain `pause`: the app's
/// `rec-pause` stays.
pub fn pause_icon(paused: bool) -> &'static str {
    if paused {
        "play"
    } else {
        "rec-pause"
    }
}
/// The mic: the slashed icon means off for this take or muted (AUD-6). The kit has no
/// `mic-off`, so the app's `rec-mic-off` stays.
pub fn mic_icon(on: bool) -> &'static str {
    if on {
        "mic"
    } else {
        "rec-mic-off"
    }
}

/// What the Mic button is and does (AUD-6): a toggle for the mute while the take has a mic
/// track, dead (with the reason as its hint) when it has none or the take is not live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicButton {
    /// Click toggles the mute.
    pub enabled: bool,
    /// The slashed, red-tinted state: muted, or no microphone at all.
    pub off: bool,
    pub muted: bool,
    pub has_mic: bool,
}

impl MicButton {
    pub fn new(has_mic: bool, muted: bool, live: bool) -> Self {
        MicButton {
            enabled: has_mic && live,
            off: !has_mic || muted,
            muted: has_mic && muted,
            has_mic,
        }
    }

    /// The icon for this state.
    pub fn icon(&self) -> &'static str {
        mic_icon(!self.off)
    }

    /// The text that goes in the hint strip.
    pub fn hint_is_unmute(&self) -> bool {
        self.muted
    }

    /// The accessible label: the action a click does, or the reason it does nothing.
    pub fn label(&self) -> String {
        if !self.has_mic {
            tr!("Microphone is off for this recording")
        } else if self.muted {
            tr!("Unmute microphone")
        } else {
            tr!("Mute microphone")
        }
    }
}
/// The Marker button's icon and the hint of its key.
pub const MARKER_ICON: &str = "bookmark";
pub const MARKER_KEY: &str = "M";
pub fn marker_label() -> String {
    tr_ctx!("recording hud", "Marker")
}
/// Camera on, or the slashed icon when the take has no camera.
pub fn camera_icon(on: bool) -> &'static str {
    if on {
        "video"
    } else {
        "video-off"
    }
}
/// Draw is not built (REC-9): disabled, hint "Coming soon".
pub const DRAW_ICON: &str = "pen-line";
pub const RESTART_ICON: &str = "rotate-cw";
pub const DISCARD_ICON: &str = "trash-2";
pub const AGENT_ICON: &str = "bot";

/// The agent chip's text.
pub fn agent_chip_text(client: &str) -> String {
    tr!("Started by {client}", client = client)
}

// ---- sizes (logical px) ----------------------------------------------------------------------

/// Padding inside the bar and the gap between its slots.
pub const PAD: f64 = 6.0;
pub const GAP: f64 = 6.0;
/// The border of the bar (1 px each side).
pub const BORDER: f64 = 2.0;
/// Bar height: 6 + a 44 px button + 6, plus the border.
pub const BAR_H: f64 = 58.0;
/// A 44 px icon button (the hit target).
pub const BUTTON: f64 = 44.0;
/// Stop with its label: 16 px padding, a 16 px icon, 8 px gap and "Stop" at 14/600.
pub const STOP_W: f64 = 88.0;
/// The timer: dot, mono 15 px text and 12 px padding each side.
pub const TIMER_W: f64 = 84.0;
pub const DIVIDER_W: f64 = 1.0;
/// Room of the strip above the bar for hints and chips.
pub const STRIP_H: f64 = 36.0;
/// The agent chip: 10 px padding each side, a 12 px icon and a 6 px gap around the text.
const CHIP_FIXED: f64 = 10.0 + 12.0 + 6.0 + 10.0 + 2.0;
/// Average width of a 12 px character, rounded up so the text is never clipped.
const CHIP_CHAR: f64 = 6.6;
/// The chip never grows beyond this; a longer name is cut with an ellipsis.
pub const CHIP_MAX: f64 = 240.0;

fn slot_width(item: HudItem, agent: Option<&str>) -> f64 {
    match item {
        HudItem::Divider => DIVIDER_W,
        HudItem::Control(c) => match c {
            HudControl::Stop => STOP_W,
            HudControl::Timer => TIMER_W,
            HudControl::Meter => METER_W,
            HudControl::Marker => MARKER_W,
            HudControl::AgentChip => agent.map_or(0.0, agent_chip_width),
            _ => BUTTON,
        },
    }
}

/// The width of the agent chip for a client name.
pub fn agent_chip_width(client: &str) -> f64 {
    let chars = agent_chip_text(client).chars().count() as f64;
    (CHIP_FIXED + chars * CHIP_CHAR).min(CHIP_MAX).ceil()
}

/// The bar's width: every slot, the gaps between them, the padding and the border.
pub fn bar_width(agent: Option<&str>) -> f64 {
    bar_width_for(false, agent)
}

/// [`bar_width`] of a screen or an audio recording.
pub fn bar_width_for(audio: bool, agent: Option<&str>) -> f64 {
    let items = hud_items_for(audio, agent.is_some());
    let slots: f64 = items.iter().map(|i| slot_width(*i, agent)).sum();
    slots + GAP * (items.len() - 1) as f64 + 2.0 * PAD + BORDER
}

/// The bar without an agent chip: what `HUD_BAR` is.
pub fn bar_size(agent: Option<&str>) -> (f64, f64) {
    (bar_width(agent), BAR_H)
}

/// The window: the bar under the strip.
pub fn window_size(agent: Option<&str>) -> (f64, f64) {
    window_size_for(false, agent)
}

/// [`window_size`] of a screen or an audio recording.
pub fn window_size_for(audio: bool, agent: Option<&str>) -> (f64, f64) {
    (bar_width_for(audio, agent), BAR_H + STRIP_H)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Redesign R18: Stop, timer | Pause, Mic, Camera, Draw | Restart, Discard, then the chip.
    #[test]
    fn the_bar_follows_the_board_order() {
        use HudControl::*;
        use HudItem::*;
        assert_eq!(
            hud_items(false),
            [
                Control(Stop),
                Control(Timer),
                Divider,
                Control(Pause),
                Control(Mic),
                Control(Camera),
                Control(Draw),
                Divider,
                Control(Restart),
                Control(Discard),
            ]
        );
        let with = hud_items(true);
        assert_eq!(with.len(), 11);
        assert_eq!(with[10], Control(AgentChip));
        assert_eq!(&with[..10], hud_items(false).as_slice());
    }

    /// The icon table: the kit's names, the slashed ones when off, the app's own where the kit
    /// has none (`rec-pause`, `rec-mic-off`).
    #[test]
    fn icons_per_state() {
        assert_eq!(STOP_ICON, "cm-stop");
        assert_eq!((pause_icon(false), pause_icon(true)), ("rec-pause", "play"));
        assert_eq!((mic_icon(true), mic_icon(false)), ("mic", "rec-mic-off"));
        assert_eq!(MicButton::new(true, false, true).icon(), "mic");
        assert_eq!(MicButton::new(true, true, true).icon(), "rec-mic-off");
        assert_eq!(
            (camera_icon(true), camera_icon(false)),
            ("video", "video-off")
        );
        assert_eq!(
            (DRAW_ICON, RESTART_ICON, DISCARD_ICON, AGENT_ICON),
            ("pen-line", "rotate-cw", "trash-2", "bot")
        );
    }

    /// The bar is 506 wide without the chip and grows by a gap and the chip with it; the height
    /// and the strip are unchanged.
    #[test]
    fn bar_widths() {
        assert_eq!(bar_width(None), 506.0);
        assert_eq!(BAR_H, 58.0);
        let w = bar_width(Some("Claude Code"));
        assert_eq!(w, 506.0 + GAP + agent_chip_width("Claude Code"));
        assert!(agent_chip_width("a") < agent_chip_width("Claude Code"));
        // A very long name is capped.
        assert_eq!(agent_chip_width(&"x".repeat(200)), CHIP_MAX);
        assert_eq!(window_size(None), (506.0, 58.0 + 36.0));
        assert_eq!(bar_size(None), (506.0, 58.0));
    }

    /// The window constants of `ui::recording` are this module's sizes.
    #[test]
    fn window_constants_match_the_layout() {
        use crate::ui::recording::{HUD_BAR, HUD_STRIP, HUD_WINDOW};
        assert_eq!(HUD_BAR, bar_size(None));
        assert_eq!(HUD_WINDOW, window_size(None));
        assert_eq!(HUD_STRIP, STRIP_H);
    }

    /// AUD-6: the Mic button toggles the mute only on a live take with a mic track; its label is
    /// the action, or the reason it is dead.
    #[test]
    fn mic_button_states_and_labels() {
        let on = MicButton::new(true, false, true);
        assert!(on.enabled && !on.off && !on.muted);
        assert_eq!(on.label(), "Mute microphone");
        let muted = MicButton::new(true, true, true);
        assert!(muted.enabled && muted.off && muted.muted && muted.hint_is_unmute());
        assert_eq!(muted.label(), "Unmute microphone");
        let none = MicButton::new(false, false, true);
        assert!(!none.enabled && none.off && !none.muted);
        assert_eq!(none.label(), "Microphone is off for this recording");
        // A mute flag with no mic track is ignored; a take that is not live cannot toggle.
        assert!(!MicButton::new(false, true, true).muted);
        assert!(!MicButton::new(true, false, false).enabled);
    }

    /// VOC-1 / board V2: the audio bar has Stop, timer, meter | Pause, Mic, Marker, Discard and
    /// nothing of a screen recording (no camera, Draw or Restart); the agent chip comes last.
    #[test]
    fn the_audio_bar_follows_board_v2() {
        use HudControl::*;
        use HudItem::*;
        assert_eq!(
            hud_items_audio(false),
            [
                Control(Stop),
                Control(Timer),
                Control(Meter),
                Divider,
                Control(Pause),
                Control(Mic),
                Control(Marker),
                Control(Discard),
            ]
        );
        let with = hud_items_audio(true);
        assert_eq!(with.last(), Some(&Control(AgentChip)));
        for banned in [Camera, Draw, Restart] {
            assert!(!with.contains(&Control(banned)));
        }
        assert_eq!(hud_items_for(false, true), hud_items(true));
        // 14 bars of 4 px with 2 px gaps and 6 px padding.
        assert_eq!(METER_W, 94.0);
        // The chip adds a gap and its own width.
        assert_eq!(bar_width_for(true, None), 579.0);
        let w = bar_width_for(true, Some("Claude Code"));
        assert_eq!(
            w,
            bar_width_for(true, None) + GAP + agent_chip_width("Claude Code")
        );
        assert_eq!(window_size_for(true, None).1, BAR_H + STRIP_H);
        assert_eq!(bar_width_for(false, None), bar_width(None));
    }

    /// The meter lights from silence (none) to a loud voice (all 14); a missing or NaN level is
    /// silence.
    #[test]
    fn the_audio_meter_lights_by_level() {
        assert_eq!(meter_lit(None), 0);
        assert_eq!(meter_lit(Some(f64::NAN)), 0);
        assert_eq!(meter_lit(Some(-90.0)), 0);
        assert_eq!(meter_lit(Some(-52.0)), 1);
        assert_eq!(meter_lit(Some(-30.0)), 7);
        assert_eq!(meter_lit(Some(-6.0)), METER_BARS);
        assert_eq!(meter_lit(Some(0.0)), METER_BARS);
        assert!(meter_lit(Some(-40.0)) < meter_lit(Some(-20.0)));
        assert_eq!(MARKER_ICON, "bookmark");
        assert_eq!(marker_label(), "Marker");
    }

    #[test]
    fn chip_text_names_the_client() {
        assert_eq!(agent_chip_text("Claude Code"), "Started by Claude Code");
    }

    /// I18N P2: the chip text names the client in every language, and the width follows the text.
    #[test]
    fn i18n_chip_text_keeps_the_client() {
        use cm_i18n::{with_language, Lang};
        let pseudo = with_language(Lang::EnXa, || agent_chip_text("Claude Code"));
        assert!(pseudo.contains("Claude Code"), "{pseudo}");
        assert_ne!(pseudo, "Started by Claude Code");
        assert_eq!(stop_label(), "Stop");
    }
}
