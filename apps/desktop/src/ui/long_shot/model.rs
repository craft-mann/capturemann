//! What the Long Shot HUD and side preview show, and the toast that ends a Long Shot (design
//! S2). Plain data and text: no window, so it is unit tested. The numbers come from the daemon's
//! `long_shot_state`; nothing here sees a pixel except the already scaled preview picture.

use std::sync::Arc;

use cm_i18n::tr;
use gpui::RenderImage;
use serde_json::Value;

use crate::toast::{ToastAction, ToastKind, ToastSpec};

/// What the daemon last said about the running Long Shot.
#[derive(Default)]
pub struct LongState {
    pub frames: u32,
    pub len_px: u32,
    pub max_len: u32,
    pub header_px: u32,
    pub footer_px: u32,
    pub paused: bool,
    /// `seq` of the picture shown; asking with it again returns no picture until it changes.
    pub seq: u64,
    pub preview: Option<Arc<RenderImage>>,
    pub preview_px: (u32, u32),
    /// The daemon no longer knows the run (it ended).
    pub finished: bool,
    /// `auto` or `hand`.
    pub scroll: String,
    /// `vertical` or `horizontal`.
    pub direction: String,
}

fn u32_of(v: &Value, k: &str) -> u32 {
    v.get(k)
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0)
}

impl LongState {
    pub fn new(scroll: &str, direction: &str) -> Self {
        LongState {
            scroll: scroll.to_owned(),
            direction: direction.to_owned(),
            ..Default::default()
        }
    }

    /// Takes a `long_shot_state` answer. Returns the PNG bytes of a new picture, if it carries
    /// one (decoded by the caller, off the UI thread).
    pub fn apply(&mut self, v: &Value) -> Option<Vec<u8>> {
        if v.get("running").and_then(Value::as_bool) != Some(true) {
            self.finished = true;
            return None;
        }
        self.frames = u32_of(v, "frames");
        self.len_px = u32_of(v, "len_px");
        self.max_len = u32_of(v, "max_len");
        self.header_px = u32_of(v, "header_px");
        self.footer_px = u32_of(v, "footer_px");
        self.paused = v.get("paused").and_then(Value::as_bool) == Some(true);
        let seq = v.get("seq").and_then(Value::as_u64).unwrap_or(0);
        let png = v
            .get("preview")
            .and_then(|p| p.get("png_base64"))
            .and_then(Value::as_str)
            .and_then(|s| {
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s).ok()
            });
        if png.is_some() {
            self.seq = seq;
        }
        png
    }

    pub fn is_auto(&self) -> bool {
        self.scroll == "auto"
    }

    pub fn is_horizontal(&self) -> bool {
        self.direction == "horizontal"
    }

    /// "7 frames", "1 frame".
    pub fn frames_text(&self) -> String {
        if self.frames == 1 {
            tr!("1 frame")
        } else {
            tr!("{n} frames", n = self.frames)
        }
    }

    /// "6,840 / 16,384 px".
    pub fn length_text(&self) -> String {
        format!(
            "{} / {} px",
            thousands(u64::from(self.len_px)),
            thousands(u64::from(self.max_len.max(self.len_px)))
        )
    }

    /// Share of the limit used, 0.0 to 1.0.
    pub fn fraction(&self) -> f32 {
        if self.max_len == 0 {
            0.0
        } else {
            (self.len_px as f32 / self.max_len as f32).clamp(0.0, 1.0)
        }
    }

    /// The line under the preview: "Adding frame 8..." while running, "Paused" while paused.
    pub fn status_text(&self) -> String {
        if self.paused {
            tr!("Paused")
        } else if self.is_auto() {
            tr!("Adding frame {n}\u{2026}", n = self.frames + 1)
        } else {
            tr!("Scroll at your own pace\u{2026}")
        }
    }

    /// The tags of the fixed bands found ("Header kept once", "Footer kept once").
    pub fn band_tags(&self) -> Vec<String> {
        let mut v = Vec::new();
        if self.header_px > 0 {
            v.push(tr!("Header kept once"));
        }
        if self.footer_px > 0 {
            v.push(tr!("Footer kept once"));
        }
        v
    }
}

/// `6840` as `6,840`.
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The toast that ends a Long Shot (S2): "Long Shot saved" with the size and the frame count and
/// **Open** and **Copy**; a warning with the reason when it stopped early. Every ending keeps the
/// image. `result` is the `capture_screenshot` answer (`width`, `height`, `capture_id`,
/// `long_shot.end`, `long_shot.frames`).
pub fn done_toast(result: &Value) -> Option<ToastSpec> {
    let id = result.get("capture_id")?.as_str()?.to_owned();
    let (w, h) = (u32_of(result, "width"), u32_of(result, "height"));
    let ls = result.get("long_shot");
    let frames = ls.map_or(0, |l| u32_of(l, "frames"));
    let end = ls
        .and_then(|l| l.get("end"))
        .and_then(Value::as_str)
        .unwrap_or("reached_end");
    let (kind, title) = match end {
        "no_overlap" => (
            ToastKind::Warning,
            tr!("Stopped: the next part did not line up"),
        ),
        "nothing_scrolled" => (
            ToastKind::Warning,
            tr!("Nothing scrolled: check that the window can scroll"),
        ),
        "limit_reached" => (
            ToastKind::Warning,
            tr!("Reached the longest image CaptureMann can edit"),
        ),
        "stopped_by_user" => (ToastKind::Info, tr!("Long Shot saved")),
        _ => (ToastKind::Success, tr!("Long Shot saved")),
    };
    let frames_text = if frames == 1 {
        tr!("1 frame")
    } else {
        tr!("{n} frames", n = frames)
    };
    let body = format!(
        "{} \u{d7} {} px \u{b7} {}",
        thousands(u64::from(w)),
        thousands(u64::from(h)),
        frames_text
    );
    Some(
        ToastSpec::new(kind, title)
            .body(body)
            .button(tr!("Open"), ToastAction::OpenCapture(id.clone()))
            .button(tr!("Copy"), ToastAction::CopyCapture(id)),
    )
}

/// The `--auto-scroll` value the overlay takes for what the system can do.
pub fn overlay_flag(s: cm_capture::long_shot::AutoScrollSupport) -> &'static str {
    use cm_capture::long_shot::AutoScrollSupport as S;
    match s {
        S::Available => "available",
        S::NeedsAccessibility => "needs_accessibility",
        // Wayland cannot inject scrolling, and a system without a scroller offers Hand only.
        S::NoInjection | S::NotBuilt => "unavailable",
    }
}

/// What a daemon event means for the Esc key (CAP-6: "Esc or Stop keeps what has been captured").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeKey {
    /// A Long Shot started (from the app, the CLI or an agent): Esc is a global key until it ends.
    Grab,
    /// The run ended: Esc goes back to the other apps.
    Release,
}

/// The daemon's `capture.long_shot.started` and `.ended` events, as the Esc key sees them.
pub fn escape_for_event(name: &str) -> Option<EscapeKey> {
    match name {
        cm_daemon::long_shot_ops::STARTED_EVENT => Some(EscapeKey::Grab),
        cm_daemon::long_shot_ops::ENDED_EVENT => Some(EscapeKey::Release),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Esc is registered while any Long Shot runs, the app's or the CLI's, and let go after.
    #[test]
    fn esc_follows_the_daemons_long_shot_events() {
        assert_eq!(
            escape_for_event("capture.long_shot.started"),
            Some(EscapeKey::Grab)
        );
        assert_eq!(
            escape_for_event("capture.long_shot.ended"),
            Some(EscapeKey::Release)
        );
        // The progress event arrives many times a run and changes nothing.
        assert_eq!(escape_for_event("capture.long_shot.progress"), None);
        assert_eq!(escape_for_event("capture.created"), None);
    }

    /// The key is the bare Escape, the same id the delay pill's handler already routes, so no
    /// shortcut chord can collide with it.
    #[test]
    fn esc_is_the_bare_escape_key() {
        assert_eq!(
            super::super::escape_hotkey_id(),
            crate::ui::delay_pill::escape_hotkey_id()
        );
        let map = crate::shortcuts::ShortcutMap::defaults(crate::shortcuts::Platform::Mac);
        for (_, chord) in map.iter().filter(|(_, c)| !c.is_unset()) {
            let id = crate::platform::chord_to_hotkey(chord).unwrap().id();
            assert_ne!(id, super::super::escape_hotkey_id());
        }
        assert_eq!(
            crate::delay::key_action("escape"),
            crate::delay::KeyAction::Cancel
        );
    }

    #[test]
    fn numbers_get_thousands_separators() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(6840), "6,840");
        assert_eq!(thousands(16384), "16,384");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn the_hud_reads_the_daemons_answer() {
        let mut s = LongState::new("auto", "vertical");
        let png = s.apply(&json!({
            "running": true, "paused": false, "seq": 3, "frames": 7, "len_px": 6840,
            "max_len": 16384, "header_px": 64, "footer_px": 0,
            "preview": { "width": 2, "height": 2, "png_base64": "AAEC" }
        }));
        assert_eq!(png, Some(vec![0, 1, 2]));
        assert_eq!((s.frames, s.seq), (7, 3));
        assert_eq!(s.frames_text(), "7 frames");
        assert_eq!(s.length_text(), "6,840 / 16,384 px");
        assert!((s.fraction() - 6840.0 / 16384.0).abs() < 1e-6);
        assert_eq!(s.status_text(), "Adding frame 8\u{2026}");
        assert_eq!(s.band_tags(), vec!["Header kept once".to_owned()]);
        // No new picture: the old one and its seq stay.
        assert_eq!(
            s.apply(&json!({"running": true, "seq": 3, "frames": 8})),
            None
        );
        assert_eq!(s.seq, 3);
        // Paused, then ended.
        s.apply(&json!({"running": true, "paused": true}));
        assert_eq!(s.status_text(), "Paused");
        assert!(!s.finished);
        s.apply(&json!({"running": false}));
        assert!(s.finished);
    }

    #[test]
    fn hand_scroll_says_so_and_one_frame_is_singular() {
        let mut s = LongState::new("hand", "horizontal");
        s.apply(&json!({"running": true, "frames": 1, "len_px": 800, "max_len": 16384}));
        assert_eq!(s.status_text(), "Scroll at your own pace\u{2026}");
        assert_eq!(s.frames_text(), "1 frame");
        assert!(s.is_horizontal() && !s.is_auto());
        // The limit shown is never below the length (a stale max).
        let mut s = LongState::default();
        s.apply(&json!({"running": true, "len_px": 500, "max_len": 0}));
        assert_eq!(s.length_text(), "500 / 500 px");
    }

    /// S2 "How it ends": every ending keeps the image and offers Open and Copy; the two early
    /// stops are warnings with their own words.
    #[test]
    fn s2_every_ending_has_its_message_and_keeps_the_image() {
        let mk = |end: &str| {
            json!({"capture_id": "shot_1", "width": 798, "height": 12480,
                   "long_shot": {"end": end, "frames": 12}})
        };
        let t = done_toast(&mk("reached_end")).unwrap();
        assert_eq!(t.kind, ToastKind::Success);
        assert_eq!(t.title, "Long Shot saved");
        assert_eq!(
            t.body.as_deref(),
            Some("798 \u{d7} 12,480 px \u{b7} 12 frames")
        );
        let labels: Vec<_> = t.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, vec!["Open", "Copy"]);
        assert_eq!(
            t.buttons[0].action,
            ToastAction::OpenCapture("shot_1".into())
        );
        let t = done_toast(&mk("no_overlap")).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Stopped: the next part did not line up");
        let t = done_toast(&mk("limit_reached")).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Reached the longest image CaptureMann can edit");
        let t = done_toast(&mk("stopped_by_user")).unwrap();
        assert_eq!(t.kind, ToastKind::Info);
        assert_eq!(t.title, "Long Shot saved");
        assert!(done_toast(&json!({"width": 1})).is_none());
    }

    #[test]
    fn the_overlay_flag_hides_auto_where_the_system_cannot_scroll() {
        use cm_capture::long_shot::AutoScrollSupport as S;
        assert_eq!(overlay_flag(S::Available), "available");
        assert_eq!(overlay_flag(S::NeedsAccessibility), "needs_accessibility");
        assert_eq!(overlay_flag(S::NoInjection), "unavailable");
        assert_eq!(overlay_flag(S::NotBuilt), "unavailable");
    }
}
