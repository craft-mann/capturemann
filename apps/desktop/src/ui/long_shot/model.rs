//! What the Long Shot HUD and side preview show, and the toast that ends a Long Shot (design
//! S2). Plain data and text: no window, so it is unit tested. The numbers come from the daemon's
//! `long_shot_state`; nothing here sees a pixel except the already scaled preview picture.

use std::sync::Arc;
use std::time::{Duration, Instant};

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
    /// The newest settled frame did not line up with the last one taken (Hand scroll).
    pub join_failed: bool,
    /// The "did not line up" toast was shown for this failure (one toast per failure, not one
    /// per poll).
    join_warned: bool,
    /// A run an agent or the CLI started: the passive HUD (Stop, the count and an Agent chip).
    pub passive: bool,
    /// The agent's name for the chip's tooltip, when the daemon said it.
    pub client: Option<String>,
    /// When the count or the length last changed (a frame was taken); `None` before the first
    /// answer.
    last_change: Option<Instant>,
}

/// No new rows for this long at the bottom of a Hand scroll: "End of page" (S18).
pub const END_OF_PAGE_AFTER: Duration = Duration::from_secs(1);
/// A frame was taken this recently: content is moving and frames are being taken (the live dot).
pub const LIVE_FOR: Duration = Duration::from_millis(1500);
/// The new counter number fades in over this long (S18); no bounce.
pub const COUNT_FADE: Duration = Duration::from_millis(120);

/// The dot after the frame count (S18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    /// Frames are being taken: success.
    Live,
    /// The last part did not line up: warning.
    Warning,
    /// Nothing to say (idle, paused, or ended): the slot stays so the count does not move.
    Off,
}

/// What the side preview's bottom says (S18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    /// "Keep scrolling" with a chevron over a fade.
    KeepScrolling,
    /// "End of page \u{b7} press Stop".
    EndOfPage,
    Hidden,
}

/// The HUD's second line (S18).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecondLine {
    /// "6,840 / 16,384 px".
    Size(String),
    /// "Scroll back up a little" with an amber dot, until a frame lines up again.
    ScrollBack,
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
        self.apply_at(v, Instant::now())
    }

    /// [`LongState::apply`] at a known time (tests).
    pub fn apply_at(&mut self, v: &Value, now: Instant) -> Option<Vec<u8>> {
        if v.get("running").and_then(Value::as_bool) != Some(true) {
            self.finished = true;
            return None;
        }
        let (frames, len_px) = (u32_of(v, "frames"), u32_of(v, "len_px"));
        if self.last_change.is_none() || frames != self.frames || len_px != self.len_px {
            self.last_change = Some(now);
        }
        self.frames = frames;
        self.len_px = len_px;
        self.max_len = u32_of(v, "max_len");
        self.set_join_ok(v.get("join_ok").and_then(Value::as_bool) != Some(false));
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

    /// What the daemon's progress event of a run this app did not start says (the passive HUD
    /// has no picture and does not poll): the count, the length and whether the join was good.
    pub fn apply_progress(&mut self, v: &Value, now: Instant) {
        let (frames, len_px) = (u32_of(v, "frames"), u32_of(v, "len_px"));
        if self.last_change.is_none() || frames != self.frames || len_px != self.len_px {
            self.last_change = Some(now);
        }
        self.frames = frames;
        self.len_px = len_px;
        self.max_len = u32_of(v, "max_len");
        self.set_join_ok(v.get("join_ok").and_then(Value::as_bool) != Some(false));
    }

    fn set_join_ok(&mut self, ok: bool) {
        self.join_failed = !ok;
        if ok {
            self.join_warned = false;
        }
    }

    /// True once per "did not line up" episode: the app shows the toast that offers Stop and keep
    /// then (Hand scroll only; Auto scroll ends the run instead).
    pub fn take_join_warning(&mut self) -> bool {
        if self.join_failed && !self.join_warned && !self.is_auto() && !self.passive {
            self.join_warned = true;
            return true;
        }
        false
    }

    /// A Long Shot an agent or the CLI started: Stop, the count and an Agent chip. It always
    /// shows Auto (Hand scroll is refused for agents).
    pub fn passive(scroll: &str, direction: &str, client: Option<String>) -> Self {
        LongState {
            passive: true,
            client,
            ..LongState::new(scroll, direction)
        }
    }

    fn since_change(&self, now: Instant) -> Option<Duration> {
        self.last_change.map(|t| now.saturating_duration_since(t))
    }

    /// The dot after the count (S18): success while frames are being taken, warning while the
    /// last part did not line up, nothing when idle or paused.
    pub fn dot(&self, now: Instant) -> Dot {
        if self.finished || self.paused {
            Dot::Off
        } else if self.join_failed && !self.is_auto() {
            Dot::Warning
        } else if self.since_change(now).is_some_and(|d| d < LIVE_FOR) {
            Dot::Live
        } else {
            Dot::Off
        }
    }

    /// The HUD's second line: the size, or "Scroll back up a little" while the last part did not
    /// line up in Hand scroll (it returns by itself when a frame lines up again).
    pub fn second_line(&self) -> SecondLine {
        if self.join_failed && !self.is_auto() && !self.passive {
            SecondLine::ScrollBack
        } else {
            SecondLine::Size(self.length_text())
        }
    }

    /// The cue at the bottom of the side preview (Hand scroll): "Keep scrolling" while the person
    /// scrolls; "End of page" once no new rows came for a second after at least a second frame;
    /// nothing while paused, while a join failed (the HUD says what to do) or in Auto scroll.
    pub fn cue(&self, now: Instant) -> Cue {
        if self.is_auto() || self.passive || self.paused || self.finished || self.join_failed {
            Cue::Hidden
        } else if self.frames >= 2
            && self
                .since_change(now)
                .is_some_and(|d| d >= END_OF_PAGE_AFTER)
        {
            Cue::EndOfPage
        } else {
            Cue::KeepScrolling
        }
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

/// How the run was stopped and with what, which the daemon's answer does not say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ending {
    /// The run was Auto scroll (the answer's `long_shot` does not carry the mode).
    pub auto: bool,
    /// Esc stopped it (Stop on the HUD and Esc both give `stopped_by_user`).
    pub esc: bool,
}

/// "758 × 7,320".
fn size_text(w: u32, h: u32) -> String {
    format!(
        "{} \u{d7} {}",
        thousands(u64::from(w)),
        thousands(u64::from(h))
    )
}

/// The toast that ends a Long Shot (S2, S18): "Long Shot saved" with **Open** and **Copy**; the
/// words say how it ended (Esc, the size limit, a page that jumped in Auto scroll) and what the
/// person can do. Every ending keeps the image. `result` is the `capture_screenshot` answer
/// (`width`, `height`, `capture_id`, `long_shot.end`, `long_shot.frames`).
pub fn done_toast(result: &Value, how: Ending) -> Option<ToastSpec> {
    let id = result.get("capture_id")?.as_str()?.to_owned();
    let (w, h) = (u32_of(result, "width"), u32_of(result, "height"));
    let ls = result.get("long_shot");
    let frames = ls.map_or(0, |l| u32_of(l, "frames"));
    let end = ls
        .and_then(|l| l.get("end"))
        .and_then(Value::as_str)
        .unwrap_or("reached_end");
    let open = |t: ToastSpec| {
        t.button(tr!("Open"), ToastAction::OpenCapture(id.clone()))
            .button(tr!("Copy"), ToastAction::CopyCapture(id.clone()))
    };
    let frames_text = if frames == 1 {
        tr!("1 frame")
    } else {
        tr!("{n} frames", n = frames)
    };
    let generic = |kind: ToastKind, title: String| {
        open(ToastSpec::new(kind, title).body(format!(
            "{} px \u{b7} {}",
            size_text(w, h),
            frames_text
        )))
    };
    Some(match end {
        // Auto scroll stops where the page jumped and keeps what it has (AC3); Hand scroll
        // leaves a part out and goes on, so a stop in it is the person's.
        "no_overlap" if how.auto => {
            ToastSpec::new(ToastKind::Warning, tr!("Stopped where the page jumped"))
                .body(if frames == 1 {
                    tr!("Auto scroll stops here and keeps 1 frame. Try Hand scroll for pages that redraw while scrolling.")
                } else {
                    tr!("Auto scroll stops here and keeps {n} frames. Try Hand scroll for pages that redraw while scrolling.", n = frames)
                })
                .button(tr!("Open"), ToastAction::OpenCapture(id.clone()))
                .button(tr!("Try Hand scroll"), ToastAction::TryHandScroll)
        }
        "nothing_scrolled" => generic(
            ToastKind::Warning,
            tr!("Nothing scrolled: the area may be covered by another window or the page is not scrollable"),
        ),
        "limit_reached" => {
            // The limit that was met: the person's own `max_px`, the 50 megapixel rule, or the
            // longest image the editor handles (an answer without `limit_px` is the last).
            let limit = ls.map_or(0, |l| u32_of(l, "limit_px"));
            let body = match cm_capture::long_shot::limit_kind(
                cm_capture::long_shot::Limits::with_max_px((limit > 0).then_some(limit)),
                w.max(h),
            ) {
                _ if limit == 0 => None,
                cm_capture::long_shot::LimitKind::MaxPx => Some(tr!(
                    "It reached the size limit of {limit} px. Everything up to there is kept.",
                    limit = thousands(u64::from(limit))
                )),
                cm_capture::long_shot::LimitKind::Pixels => Some(tr!(
                    "It reached the size limit of 50 megapixels. Everything up to there is kept."
                )),
                cm_capture::long_shot::LimitKind::Editor => None,
            };
            open(
                ToastSpec::new(ToastKind::Warning, tr!("Long Shot saved at the size limit")).body(
                    body.unwrap_or_else(|| {
                        tr!(
                            "It reached {limit} px, the longest image CaptureMann can edit. Everything up to there is kept.",
                            limit = thousands(u64::from(cm_capture::long_shot::MAX_LONG_SIDE))
                        )
                    }),
                ),
            )
        }
        // The window was moved or resized during the run: not a page that jumped.
        "window_changed" => open(
            ToastSpec::new(ToastKind::Warning, tr!("Long Shot saved")).body(if frames == 1 {
                tr!(
                    "The window was moved or resized. Everything up to there is kept: 1 frame, {size}.",
                    size = size_text(w, h)
                )
            } else {
                tr!(
                    "The window was moved or resized. Everything up to there is kept: {n} frames, {size}.",
                    n = frames,
                    size = size_text(w, h)
                )
            }),
        ),
        // The window was closed during the run (the frames before are kept).
        "window_closed" => open(
            ToastSpec::new(ToastKind::Warning, tr!("Long Shot saved")).body(if frames == 1 {
                tr!(
                    "The window was closed. Everything up to there is kept: 1 frame, {size}.",
                    size = size_text(w, h)
                )
            } else {
                tr!(
                    "The window was closed. Everything up to there is kept: {n} frames, {size}.",
                    n = frames,
                    size = size_text(w, h)
                )
            }),
        ),
        "stopped_by_user" | "no_overlap" if how.esc => open(
            ToastSpec::new(ToastKind::Info, tr!("Long Shot saved")).body(if frames == 1 {
                tr!(
                    "Stopped with Esc. Everything up to here is kept: 1 frame, {size}.",
                    size = size_text(w, h)
                )
            } else {
                tr!(
                    "Stopped with Esc. Everything up to here is kept: {n} frames, {size}.",
                    n = frames,
                    size = size_text(w, h)
                )
            }),
        ),
        "stopped_by_user" | "no_overlap" => generic(ToastKind::Info, tr!("Long Shot saved")),
        _ => generic(ToastKind::Success, tr!("Long Shot saved")),
    })
}

/// The toast while a Hand scroll goes on and the last part did not line up (S18): what to do, and
/// **Stop and keep** for a person who would rather end it.
pub fn did_not_line_up_toast(frames: u32) -> ToastSpec {
    ToastSpec::new(
        ToastKind::Warning,
        tr!("The last part didn\u{2019}t line up"),
    )
    .body(if frames == 1 {
        tr!("Scroll back up a little and keep going, or stop and keep the 1 frame so far.")
    } else {
        tr!(
            "Scroll back up a little and keep going, or stop and keep the {n} frames so far.",
            n = frames
        )
    })
    .button(tr!("Stop and keep"), ToastAction::StopLongShot)
    .dismiss_after(Duration::from_secs(6))
}

/// A Long Shot an agent or the CLI started, as the `capture.long_shot.started` event says it
/// (S18: the passive HUD).
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignRun {
    pub request_id: Option<String>,
    pub client: Option<String>,
    pub scroll: String,
    pub direction: String,
    pub display_id: u32,
    /// `[x, y, w, h]` physical px in the display's own frame.
    pub area: [i64; 4],
    pub scale: f64,
}

impl ForeignRun {
    /// `None` when the event does not say where the area is (no HUD can be placed).
    pub fn from_event(v: &Value) -> Option<ForeignRun> {
        let a = v.get("area")?.as_array()?;
        let n = |i: usize| a.get(i).and_then(Value::as_i64);
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        Some(ForeignRun {
            request_id: text("request_id"),
            client: text("client"),
            scroll: text("scroll").unwrap_or_else(|| "auto".to_owned()),
            direction: text("direction").unwrap_or_else(|| "vertical".to_owned()),
            display_id: u32::try_from(v.get("display_id")?.as_u64()?).ok()?,
            area: [n(0)?, n(1)?, n(2)?, n(3)?],
            scale: v
                .get("scale")
                .and_then(Value::as_f64)
                .filter(|s| *s > 0.0)
                .unwrap_or(1.0),
        })
    }
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

    fn answer(end: &str) -> Value {
        json!({"capture_id": "shot_1", "width": 758, "height": 7320,
               "long_shot": {"end": end, "frames": 9}})
    }

    fn labels(t: &ToastSpec) -> Vec<&str> {
        t.buttons.iter().map(|b| b.label.as_str()).collect()
    }

    /// S2 "How it ends": every ending keeps the image and offers Open and Copy.
    #[test]
    fn s2_every_ending_has_its_message_and_keeps_the_image() {
        let t = done_toast(&answer("reached_end"), Ending::default()).unwrap();
        assert_eq!(t.kind, ToastKind::Success);
        assert_eq!(t.title, "Long Shot saved");
        assert_eq!(
            t.body.as_deref(),
            Some("758 \u{d7} 7,320 px \u{b7} 9 frames")
        );
        assert_eq!(labels(&t), vec!["Open", "Copy"]);
        assert_eq!(
            t.buttons[0].action,
            ToastAction::OpenCapture("shot_1".into())
        );
        assert_eq!(
            t.buttons[1].action,
            ToastAction::CopyCapture("shot_1".into())
        );
        // A stop with the button (not Esc) is the plain "saved".
        let t = done_toast(&answer("stopped_by_user"), Ending::default()).unwrap();
        assert_eq!(t.kind, ToastKind::Info);
        assert_eq!(t.title, "Long Shot saved");
        assert!(done_toast(&json!({"width": 1}), Ending::default()).is_none());
    }

    /// S18: the four endings, with exactly these words.
    #[test]
    fn s18_the_end_toasts_say_what_happened_and_what_to_do() {
        let esc = Ending {
            auto: false,
            esc: true,
        };
        let t = done_toast(&answer("stopped_by_user"), esc).unwrap();
        assert_eq!(t.kind, ToastKind::Info);
        assert_eq!(t.title, "Long Shot saved");
        assert_eq!(
            t.body.as_deref(),
            Some("Stopped with Esc. Everything up to here is kept: 9 frames, 758 \u{d7} 7,320.")
        );
        assert_eq!(labels(&t), vec!["Open", "Copy"]);
        // Esc while the last part was left out (the engine says no_overlap) is still Esc's.
        let t = done_toast(&answer("no_overlap"), esc).unwrap();
        assert!(t.body.as_deref().unwrap().starts_with("Stopped with Esc."));

        // The window was moved or resized: its own words, and nothing about a page that jumped.
        let t = done_toast(
            &answer("window_changed"),
            Ending {
                auto: true,
                esc: false,
            },
        )
        .unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Long Shot saved");
        assert_eq!(
            t.body.as_deref(),
            Some("The window was moved or resized. Everything up to there is kept: 9 frames, 758 \u{d7} 7,320.")
        );
        assert_eq!(labels(&t), vec!["Open", "Copy"]);

        // LS-14: a window that was closed has its own words.
        let t = done_toast(&answer("window_closed"), Ending::default()).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(
            t.body.as_deref(),
            Some("The window was closed. Everything up to there is kept: 9 frames, 758 \u{d7} 7,320.")
        );
        assert_eq!(labels(&t), vec!["Open", "Copy"]);

        // LS-4: the limit that was met is the one named. A user `max_px` ...
        let with_limit = |limit: u32, w: u32, h: u32| {
            done_toast(
                &json!({"capture_id": "shot_1", "width": w, "height": h,
                        "long_shot": {"end": "limit_reached", "frames": 9, "limit_px": limit}}),
                Ending::default(),
            )
            .unwrap()
        };
        assert_eq!(
            with_limit(4000, 1540, 4000).body.as_deref(),
            Some("It reached the size limit of 4,000 px. Everything up to there is kept.")
        );
        // ... the 50 megapixel rule (the image is shorter than the length limit) ...
        assert_eq!(
            with_limit(16384, 4000, 12500).body.as_deref(),
            Some("It reached the size limit of 50 megapixels. Everything up to there is kept.")
        );
        // ... and only the product limit says "the longest image CaptureMann can edit".
        assert!(with_limit(16384, 1540, 16384)
            .body
            .as_deref()
            .unwrap()
            .contains("the longest image CaptureMann can edit"));

        let t = done_toast(&answer("limit_reached"), Ending::default()).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Long Shot saved at the size limit");
        assert_eq!(
            t.body.as_deref(),
            Some("It reached 16,384 px, the longest image CaptureMann can edit. Everything up to there is kept.")
        );
        assert_eq!(labels(&t), vec!["Open", "Copy"]);

        let auto = Ending {
            auto: true,
            esc: false,
        };
        let t = done_toast(&answer("no_overlap"), auto).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Stopped where the page jumped");
        assert_eq!(
            t.body.as_deref(),
            Some("Auto scroll stops here and keeps 9 frames. Try Hand scroll for pages that redraw while scrolling.")
        );
        assert_eq!(labels(&t), vec!["Open", "Try Hand scroll"]);
        assert_eq!(t.buttons[1].action, ToastAction::TryHandScroll);

        // The live toast while a Hand scroll goes on.
        let t = did_not_line_up_toast(9);
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "The last part didn\u{2019}t line up");
        assert_eq!(
            t.body.as_deref(),
            Some("Scroll back up a little and keep going, or stop and keep the 9 frames so far.")
        );
        assert_eq!(labels(&t), vec!["Stop and keep"]);
        assert_eq!(t.buttons[0].action, ToastAction::StopLongShot);
        assert_eq!(t.dismiss_after, Some(Duration::from_secs(6)));
        assert!(did_not_line_up_toast(1)
            .body
            .unwrap()
            .contains("keep the 1 frame so far"));
    }

    /// A Hand scroll that was stopped while a part was left out is the person's stop, not the
    /// page's jump: only Auto scroll says "the page jumped".
    #[test]
    fn only_auto_scroll_blames_the_page() {
        let t = done_toast(&answer("no_overlap"), Ending::default()).unwrap();
        assert_ne!(t.title, "Stopped where the page jumped");
        assert_eq!(labels(&t), vec!["Open", "Copy"]);
        let t = done_toast(
            &answer("nothing_scrolled"),
            Ending {
                auto: true,
                esc: true,
            },
        )
        .unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
    }

    fn t0() -> Instant {
        Instant::now()
    }

    /// S18: the dot is success while frames are being taken, warning when the last part did not
    /// line up, and absent when idle or paused; the slot stays (the view draws it either way).
    #[test]
    fn s18_the_dot_follows_the_frames() {
        let t = t0();
        let mut s = LongState::new("hand", "vertical");
        s.apply_at(&json!({"running": true, "frames": 1, "len_px": 800}), t);
        assert_eq!(s.dot(t), Dot::Live);
        // Nothing new for a while: no dot.
        assert_eq!(s.dot(t + Duration::from_millis(2000)), Dot::Off);
        // A new frame: live again.
        let t1 = t + Duration::from_millis(2100);
        s.apply_at(&json!({"running": true, "frames": 2, "len_px": 1500}), t1);
        assert_eq!(s.dot(t1), Dot::Live);
        // The last part did not line up: warning, whatever the clock says.
        s.apply_at(
            &json!({"running": true, "frames": 2, "len_px": 1500, "join_ok": false}),
            t1,
        );
        assert_eq!(s.dot(t1), Dot::Warning);
        assert_eq!(s.dot(t1 + Duration::from_secs(10)), Dot::Warning);
        // Paused: none.
        s.apply_at(
            &json!({"running": true, "paused": true, "frames": 2, "len_px": 1500}),
            t1,
        );
        assert_eq!(s.dot(t1), Dot::Off);
    }

    /// S18: "Scroll back up a little" replaces the size until a frame lines up again, by itself;
    /// the toast is offered once per failure.
    #[test]
    fn s18_scroll_back_up_returns_by_itself() {
        let t = t0();
        let mut s = LongState::new("hand", "vertical");
        let ok = json!({"running": true, "frames": 3, "len_px": 2000, "max_len": 16384, "join_ok": true});
        let bad = json!({"running": true, "frames": 3, "len_px": 2000, "max_len": 16384, "join_ok": false});
        s.apply_at(&ok, t);
        assert_eq!(
            s.second_line(),
            SecondLine::Size("2,000 / 16,384 px".into())
        );
        assert!(!s.take_join_warning());
        s.apply_at(&bad, t);
        assert_eq!(s.second_line(), SecondLine::ScrollBack);
        assert!(s.take_join_warning());
        // Still failing at the next poll: the toast is not shown again.
        s.apply_at(&bad, t);
        assert!(!s.take_join_warning());
        // A frame lines up again: back to the size, and a later failure warns again.
        s.apply_at(&ok, t);
        assert_eq!(
            s.second_line(),
            SecondLine::Size("2,000 / 16,384 px".into())
        );
        s.apply_at(&bad, t);
        assert!(s.take_join_warning());
        // An answer without the field (an older daemon) reads as lined up.
        s.apply_at(&json!({"running": true, "frames": 4, "len_px": 2500}), t);
        assert!(!s.join_failed);
        // Auto scroll ends at a failed join instead: no live line, no live toast.
        let mut a = LongState::new("auto", "vertical");
        a.apply_at(&bad, t);
        assert_eq!(
            a.second_line(),
            SecondLine::Size("2,000 / 16,384 px".into())
        );
        assert!(!a.take_join_warning());
    }

    /// S18: "Keep scrolling" until no new rows for a second at the bottom, then "End of page";
    /// a new row brings the cue back; never in Auto scroll, paused or with the HUD's own line.
    #[test]
    fn s18_the_cue_at_the_bottom_of_the_preview() {
        let t = t0();
        let mut s = LongState::new("hand", "vertical");
        s.apply_at(&json!({"running": true, "frames": 1, "len_px": 800}), t);
        // One frame, idle: the person has not started; keep scrolling.
        assert_eq!(s.cue(t + Duration::from_secs(5)), Cue::KeepScrolling);
        let t1 = t + Duration::from_secs(6);
        s.apply_at(&json!({"running": true, "frames": 2, "len_px": 1500}), t1);
        assert_eq!(s.cue(t1), Cue::KeepScrolling);
        assert_eq!(s.cue(t1 + Duration::from_millis(999)), Cue::KeepScrolling);
        assert_eq!(s.cue(t1 + Duration::from_secs(1)), Cue::EndOfPage);
        // The length changes without a new frame count (the stitcher grew): not the end.
        let t2 = t1 + Duration::from_secs(3);
        s.apply_at(&json!({"running": true, "frames": 2, "len_px": 1600}), t2);
        assert_eq!(s.cue(t2), Cue::KeepScrolling);
        // The same answer again does not restart the clock.
        s.apply_at(
            &json!({"running": true, "frames": 2, "len_px": 1600}),
            t2 + Duration::from_millis(900),
        );
        assert_eq!(s.cue(t2 + Duration::from_millis(1000)), Cue::EndOfPage);
        s.apply_at(
            &json!({"running": true, "paused": true, "frames": 2, "len_px": 1600}),
            t2,
        );
        assert_eq!(s.cue(t2 + Duration::from_secs(9)), Cue::Hidden);
        let mut a = LongState::new("auto", "vertical");
        a.apply_at(&json!({"running": true, "frames": 5, "len_px": 4000}), t);
        assert_eq!(a.cue(t + Duration::from_secs(9)), Cue::Hidden);
    }

    /// S18: the daemon's progress event of a run this app did not start feeds the passive HUD.
    #[test]
    fn s18_a_passive_run_follows_the_progress_event() {
        let t = t0();
        let mut s = LongState::passive("auto", "vertical", Some("claude-desktop".into()));
        assert!(s.passive && s.is_auto());
        s.apply_progress(
            &json!({"frames": 7, "len_px": 6840, "max_len": 16384, "join_ok": true}),
            t,
        );
        assert_eq!(s.frames_text(), "7 frames");
        assert_eq!(s.length_text(), "6,840 / 16,384 px");
        assert_eq!(s.dot(t), Dot::Live);
        assert_eq!(s.cue(t), Cue::Hidden);
        assert!(!s.take_join_warning());
        assert_eq!(s.client.as_deref(), Some("claude-desktop"));
    }

    /// S18: the started event carries who, how and where; without an area there is nothing to
    /// place a HUD against.
    #[test]
    fn s18_the_started_event_says_where_the_area_is() {
        let v = json!({
            "request_id": "mcp-1", "client": "claude-desktop", "scroll": "auto",
            "direction": "vertical", "display_id": 2, "area": [100, 50, 800, 600], "scale": 2.0
        });
        let r = ForeignRun::from_event(&v).unwrap();
        assert_eq!(r.request_id.as_deref(), Some("mcp-1"));
        assert_eq!(r.client.as_deref(), Some("claude-desktop"));
        assert_eq!(
            (r.display_id, r.area, r.scale),
            (2, [100, 50, 800, 600], 2.0)
        );
        let r = ForeignRun::from_event(&json!({
            "request_id": null, "client": null, "display_id": 1, "area": [0, 0, 1, 1], "scale": null
        }))
        .unwrap();
        assert_eq!((r.request_id, r.client, r.scale), (None, None, 1.0));
        assert_eq!(r.scroll, "auto");
        assert!(ForeignRun::from_event(&json!({"request_id": "x", "area": null})).is_none());
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
