//! Toast model (DESIGN section 4, redesign R17): at most 3 stacked in one bottom-right stack
//! (the agent toast joins it), auto-dismiss after 6 s with a pause while the pointer is over the
//! toast, except failures and toasts that carry an action (sticky, C7).

use std::time::{Duration, Instant};

// TODO(design): the R17 board also draws a "Change library" action on the disk-full toast and a
// "Published without captions" warning. Neither exists in the app (no such action, no such
// trigger), so they are not built. The Stop action of the agent toast and the upload progress
// toast (a bar, the percentage, an ETA, Cancel) are built.

pub const MAX_TOASTS: usize = 3;
pub const AUTO_DISMISS: Duration = Duration::from_secs(6);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    /// Started by an agent through MCP or the CLI (agent blue, tool name in mono).
    Agent,
    /// Failures stay until dismissed.
    Error,
}

/// What a toast button does; interpreted by the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToastAction {
    OpenLibrary,
    OpenCapture(String),
    CopyCapture(String),
    /// Save the rendered image to a file the user picks (CAP-4.AC1).
    SaveCaptureAs(String),
    /// Ask, then delete the capture (CAP-4.AC1).
    DeleteCapture(String),
    OpenScreenRecordingSettings,
    /// A System Settings -> Privacy & Security pane (`Privacy_Microphone`, `Privacy_Camera`, ...).
    OpenPrivacyPane(&'static str),
    /// Settings -> Permissions (PRM-1).
    OpenPermissions,
    /// Play the combined MP4 (the quick export) of a recording in the system player.
    PlayExport(String),
    /// Show the combined MP4 of a recording in Finder / Explorer.
    RevealExport(String),
    /// Copy a share link to the clipboard (STO-4).
    CopyLink(String),
    /// Open a share link in the browser.
    OpenLink(String),
    /// Show a file a local destination received in Finder / Explorer.
    RevealPath(String),
    /// `retry_job` on a failed upload (STO-3).
    RetryJob(String),
    /// Settings -> Destinations (to reconnect an account).
    OpenDestinations,
    /// `stop_recording` as the user, through the HUD's Stop path, only while recording `id` (its
    /// recording id) is still the one running.
    StopRecording(String),
    /// `cancel_job` on a running upload.
    CancelJob(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastButton {
    pub label: String,
    pub action: ToastAction,
}

/// A shortcut hint: text plus keycap labels, e.g. "Paste" + `[⌘, V]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub text: String,
    pub keys: Vec<String>,
}

/// Which agent tool call an agent toast reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCall {
    pub tool: String,
    pub client: String,
}

impl ToastKind {
    /// The kind's icon. The SHAPE differs by kind, so the tone is not carried by colour alone.
    pub fn icon(self) -> &'static str {
        match self {
            ToastKind::Info => "info",
            ToastKind::Success => "circle-check",
            ToastKind::Warning => "triangle-alert",
            ToastKind::Error => "octagon-alert",
            ToastKind::Agent => "bot",
        }
    }
}

impl ToastAction {
    /// The icon of a toast action button (the redesign's icon map).
    pub fn icon(&self) -> &'static str {
        match self {
            ToastAction::OpenLibrary => "images",
            ToastAction::OpenCapture(_) => "external-link",
            ToastAction::CopyCapture(_) => "copy",
            ToastAction::SaveCaptureAs(_) => "save",
            ToastAction::DeleteCapture(_) => "trash-2",
            ToastAction::OpenScreenRecordingSettings
            | ToastAction::OpenPrivacyPane(_)
            | ToastAction::OpenPermissions => "settings",
            ToastAction::PlayExport(_) => "play",
            ToastAction::RevealExport(_) | ToastAction::RevealPath(_) => "folder-open",
            ToastAction::CopyLink(_) => "link",
            ToastAction::OpenLink(_) => "external-link",
            ToastAction::RetryJob(_) => "rotate-cw",
            ToastAction::OpenDestinations => "plug",
            ToastAction::StopRecording(_) => "cm-stop",
            ToastAction::CancelJob(_) => "x",
        }
    }
}

/// How to make a toast again in the current language (I18N): a sticky toast (an error, or one
/// with buttons) stays on screen until dismissed, so when the language changes the queue calls
/// this to re-word it. Only the words are taken from the fresh spec; the clock and the id stay.
#[derive(Clone)]
pub struct Rebuild(std::sync::Arc<dyn Fn() -> ToastSpec + Send + Sync>);

impl std::fmt::Debug for Rebuild {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Rebuild(..)") // i18n-ignore: Debug output
    }
}

impl PartialEq for Rebuild {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Rebuild {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub body: Option<String>,
    pub hints: Vec<Hint>,
    pub buttons: Vec<ToastButton>,
    pub agent: Option<AgentCall>,
    /// Makes the toast's words again after a language switch (see [`ToastSpec::live`]).
    pub rebuild: Option<Rebuild>,
    /// A progress bar under the text, 0..=100 percent (a running upload).
    pub progress: Option<u8>,
    pub created: Instant,
    /// Since when the pointer has been over the toast (its clock is paused).
    pub hovered_since: Option<Instant>,
}

impl Toast {
    /// Errors and toasts that carry buttons stay until dismissed (C7).
    pub fn sticky(&self) -> bool {
        self.kind == ToastKind::Error || !self.buttons.is_empty()
    }

    /// When the toast goes away by itself; `None` for a sticky toast and while it is hovered.
    pub fn expires_at(&self) -> Option<Instant> {
        (!self.sticky() && self.hovered_since.is_none()).then(|| self.created + AUTO_DISMISS)
    }
}

/// Builder input (no id or clock yet).
#[derive(Debug, Clone)]
pub struct ToastSpec {
    pub kind: ToastKind,
    pub title: String,
    pub body: Option<String>,
    pub hints: Vec<Hint>,
    pub buttons: Vec<ToastButton>,
    pub agent: Option<AgentCall>,
    pub rebuild: Option<Rebuild>,
    pub progress: Option<u8>,
}

impl ToastSpec {
    pub fn new(kind: ToastKind, title: impl Into<String>) -> Self {
        ToastSpec {
            kind,
            title: title.into(),
            body: None,
            hints: Vec::new(),
            buttons: Vec::new(),
            agent: None,
            rebuild: None,
            progress: None,
        }
    }

    /// A toast that can be worded again: `make` builds the whole spec from data it owns (ids,
    /// reasons, counts), in the language of the moment. It is called now, and again when the
    /// language changes while the toast is on screen ([`ToastQueue::relabel`]).
    pub fn live(make: impl Fn() -> ToastSpec + Send + Sync + 'static) -> Self {
        let mut spec = make();
        spec.rebuild = Some(Rebuild(std::sync::Arc::new(make)));
        spec
    }

    /// "Started by agent via MCP" with the tool name and the client.
    pub fn agent(tool: impl Into<String>, client: impl Into<String>) -> Self {
        let mut spec = ToastSpec::new(ToastKind::Agent, cm_i18n::tr!("Started by agent via MCP"));
        spec.agent = Some(AgentCall {
            tool: tool.into(),
            client: client.into(),
        });
        spec
    }

    /// The agent toast with a "Stop" button for recording `recording_id` (REC-5: the user can
    /// stop what an agent started).
    pub fn agent_with_stop(
        tool: impl Into<String>,
        client: impl Into<String>,
        recording_id: &str,
    ) -> Self {
        let (tool, client, id) = (tool.into(), client.into(), recording_id.to_owned());
        ToastSpec::live(move || {
            ToastSpec::agent(tool.clone(), client.clone()).button(
                cm_i18n::tr_ctx!("button", "Stop"),
                ToastAction::StopRecording(id.clone()),
            )
        })
    }

    pub fn body(mut self, b: impl Into<String>) -> Self {
        self.body = Some(b.into());
        self
    }
    pub fn hint(mut self, text: impl Into<String>, keys: Vec<String>) -> Self {
        self.hints.push(Hint {
            text: text.into(),
            keys,
        });
        self
    }
    pub fn button(mut self, label: impl Into<String>, action: ToastAction) -> Self {
        self.buttons.push(ToastButton {
            label: label.into(),
            action,
        });
        self
    }
}

/// Toasts are 440 px wide windows (kit `TOAST_W`); each card sits in a 6 px inset.
pub const TOAST_WIDTH: f32 = crate::theme::scale::TOAST_W;
/// Inset around each card inside the window.
pub const TOAST_INSET: f32 = 6.;
const TOAST_GAP: f32 = 0.;

fn wrapped_lines(text: &str, chars_per_line: usize) -> usize {
    text.split('\n')
        .map(|l| l.chars().count().div_ceil(chars_per_line).max(1))
        .sum()
}

/// Height of one toast in px, estimated from its content (the toast window is sized from it;
/// generous so text is never clipped). Card: 14 px padding, a 30 px icon tile beside a text
/// column of about 330 px (13 px text, about 48 characters per line).
pub fn estimate_height(t: &Toast) -> f32 {
    // The title is 14/600 in a column of about 320 px: about 38 characters per line, in a row that
    // is at least 30 px tall (the icon tile's height).
    let mut col = (21. * wrapped_lines(&t.title, 38) as f32).max(30.);
    if t.agent.is_some() {
        col += 6. + 18.;
    }
    if let Some(b) = &t.body {
        col += 4. + 18. * wrapped_lines(b, 48) as f32;
    }
    if t.progress.is_some() {
        col += 4. + 4.;
    }
    if !t.hints.is_empty() {
        col += 8. + 22.;
    }
    if !t.buttons.is_empty() {
        // Two small (32 px) actions share a row of the text column; more wrap.
        let rows = t.buttons.len().div_ceil(2) as f32;
        col += 10. + 32. + (rows - 1.) * (8. + 32.);
    }
    // The card: the inset on both sides, its 1 px border, 14 px padding top and bottom.
    2. * TOAST_INSET + 2. + 28. + col.max(30.)
}

/// Total height of the stacked toasts including the gaps between them.
pub fn stack_height(toasts: &[Toast]) -> f32 {
    toasts.iter().map(estimate_height).sum::<f32>()
        + TOAST_GAP * toasts.len().saturating_sub(1) as f32
}

#[derive(Debug, Default)]
pub struct ToastQueue {
    toasts: Vec<Toast>,
    next_id: u64,
}

impl ToastQueue {
    pub fn push(&mut self, spec: ToastSpec, now: Instant) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.toasts.push(Toast {
            id,
            kind: spec.kind,
            title: spec.title,
            body: spec.body,
            hints: spec.hints,
            buttons: spec.buttons,
            agent: spec.agent,
            rebuild: spec.rebuild,
            progress: spec.progress,
            created: now,
            hovered_since: None,
        });
        // Oldest goes first when more than three are stacked.
        while self.toasts.len() > MAX_TOASTS {
            self.toasts.remove(0);
        }
        id
    }

    /// Replaces the content of toast `id` in place (a progress toast turning into its result),
    /// keeping its position. Restarts its auto-dismiss clock. `false` if it is gone (dismissed or
    /// pushed out), in which case the caller pushes a new one.
    pub fn replace(&mut self, id: u64, spec: ToastSpec, now: Instant) -> bool {
        match self.toasts.iter_mut().find(|t| t.id == id) {
            Some(t) => {
                *t = Toast {
                    id,
                    kind: spec.kind,
                    title: spec.title,
                    body: spec.body,
                    hints: spec.hints,
                    buttons: spec.buttons,
                    agent: spec.agent,
                    rebuild: spec.rebuild,
                    progress: spec.progress,
                    created: now,
                    hovered_since: None,
                };
                true
            }
            None => false,
        }
    }

    /// The language changed: toasts made with [`ToastSpec::live`] are worded again (title, body,
    /// hints and button labels; the id, the clock and the hover state stay). Returns whether any
    /// toast changed, so the caller resizes the stack.
    pub fn relabel(&mut self) -> bool {
        let mut changed = false;
        for t in &mut self.toasts {
            let Some(r) = t.rebuild.clone() else {
                continue;
            };
            let fresh = (r.0)();
            if t.title != fresh.title
                || t.body != fresh.body
                || t.hints != fresh.hints
                || t.buttons != fresh.buttons
            {
                changed = true;
                t.title = fresh.title;
                t.body = fresh.body;
                t.hints = fresh.hints;
                t.buttons = fresh.buttons;
            }
        }
        changed
    }

    /// The newest toast whose first button opens capture `capture_id` and whose title starts with
    /// `title_prefix` (finds the "Recording saved" toast of a take to turn it into a progress toast).
    pub fn find_for_capture(&self, capture_id: &str, title_prefix: &str) -> Option<u64> {
        self.toasts
            .iter()
            .rev()
            .find(|t| {
                t.title.starts_with(title_prefix)
                    && t.buttons
                        .iter()
                        .any(|b| b.action == ToastAction::OpenCapture(capture_id.to_owned()))
            })
            .map(|t| t.id)
    }

    /// Ids of the toasts with a button whose action satisfies `f`.
    pub fn with_action(&self, f: impl Fn(&ToastAction) -> bool) -> Vec<u64> {
        self.toasts
            .iter()
            .filter(|t| t.buttons.iter().any(|b| f(&b.action)))
            .map(|t| t.id)
            .collect()
    }

    pub fn dismiss(&mut self, id: u64) -> bool {
        let n = self.toasts.len();
        self.toasts.retain(|t| t.id != id);
        self.toasts.len() != n
    }

    /// Removes expired toasts; returns whether anything changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let n = self.toasts.len();
        self.toasts
            .retain(|t| t.expires_at().is_none_or(|e| e > now));
        self.toasts.len() != n
    }

    /// Time until the next auto-dismissal, for scheduling a single timer.
    pub fn next_expiry(&self, now: Instant) -> Option<Duration> {
        self.toasts
            .iter()
            .filter_map(Toast::expires_at)
            .min()
            .map(|e| e.saturating_duration_since(now))
    }

    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// The pointer entered or left toast `id`: its auto-dismiss clock stops while it is over it
    /// and resumes with the time it had left. `true` if anything changed.
    pub fn set_hover(&mut self, id: u64, hovered: bool, now: Instant) -> bool {
        let Some(t) = self.toasts.iter_mut().find(|t| t.id == id) else {
            return false;
        };
        match (hovered, t.hovered_since) {
            (true, None) => t.hovered_since = Some(now),
            (false, Some(since)) => {
                t.created += now.saturating_duration_since(since);
                t.hovered_since = None;
            }
            _ => return false,
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ToastKind) -> ToastSpec {
        ToastSpec::new(kind, "x")
    }

    /// DESIGN toasts: max 3 stacked.
    #[test]
    fn at_most_three_stacked() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        let first = q.push(spec(ToastKind::Info), t);
        for _ in 0..3 {
            q.push(spec(ToastKind::Info), t);
        }
        assert_eq!(q.toasts().len(), 3);
        assert!(q.toasts().iter().all(|x| x.id != first));
    }

    /// DESIGN toasts: auto-dismiss 6 s except failures and anything with an action.
    #[test]
    fn auto_dismiss_rules() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        q.push(spec(ToastKind::Success), t);
        q.push(spec(ToastKind::Error), t);
        q.push(
            spec(ToastKind::Info).button("Open library", ToastAction::OpenLibrary),
            t,
        );
        assert_eq!(q.next_expiry(t), Some(AUTO_DISMISS));
        assert!(!q.expire(t + Duration::from_millis(5999)));
        assert!(q.expire(t + AUTO_DISMISS));
        assert_eq!(q.toasts().len(), 2);
        assert_eq!(q.next_expiry(t + AUTO_DISMISS), None);
        assert!(!q.expire(t + Duration::from_secs(3600)));
    }

    /// I18N: a sticky toast on screen is worded again after a language switch; the clock and the
    /// id stay, and a toast made without `live` keeps its words.
    #[test]
    fn live_toasts_follow_the_language() {
        use cm_i18n::{with_language, Lang};
        let mut q = ToastQueue::default();
        let t = Instant::now();
        let make = || {
            ToastSpec::new(ToastKind::Error, cm_i18n::tr!("Dismiss"))
                .body(cm_i18n::tr!("Copied"))
                .button(cm_i18n::tr!("Retry"), ToastAction::OpenLibrary)
        };
        let live = with_language(Lang::En, || q.push(ToastSpec::live(make), t));
        let fixed = with_language(Lang::En, || q.push(make(), t));
        assert!(
            !q.relabel(),
            "nothing changes while the language is the same"
        );
        let changed = with_language(Lang::EnXa, || q.relabel());
        assert!(changed);
        let live_toast = q.toasts().iter().find(|x| x.id == live).unwrap();
        assert!(live_toast.title.starts_with('['), "{}", live_toast.title);
        assert!(live_toast.buttons[0].label.starts_with('['));
        assert_eq!(live_toast.buttons[0].action, ToastAction::OpenLibrary);
        assert_eq!(live_toast.created, t);
        let fixed_toast = q.toasts().iter().find(|x| x.id == fixed).unwrap();
        assert_eq!(fixed_toast.title, "Dismiss");
        // Back to English.
        assert!(q.relabel());
        let live_toast = q.toasts().iter().find(|x| x.id == live).unwrap();
        assert_eq!(live_toast.title, "Dismiss");
    }

    #[test]
    fn height_grows_with_content() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        q.push(ToastSpec::new(ToastKind::Info, "a"), t);
        let plain = stack_height(q.toasts());
        q.push(
            ToastSpec::new(ToastKind::Info, "b")
                .body("x".repeat(100))
                .hint("Paste", vec!["V".into()])
                .button("Open", ToastAction::OpenLibrary),
            t,
        );
        assert!(stack_height(q.toasts()) > plain * 2.0);
    }

    #[test]
    fn a_toast_can_be_replaced_in_place_and_found_by_capture() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        let a = q.push(
            ToastSpec::new(ToastKind::Success, "Recording saved")
                .button("Open", ToastAction::OpenCapture("rec_1".into())),
            t,
        );
        let b = q.push(ToastSpec::new(ToastKind::Info, "Other"), t);
        assert_eq!(q.find_for_capture("rec_1", "Recording saved"), Some(a));
        assert_eq!(q.find_for_capture("rec_2", "Recording saved"), None);
        assert!(q.replace(a, ToastSpec::new(ToastKind::Info, "Exporting"), t));
        assert_eq!(q.toasts()[0].title, "Exporting");
        assert_eq!(q.toasts()[0].id, a, "same place and id");
        assert!(q.toasts()[0].buttons.is_empty());
        q.dismiss(b);
        assert!(!q.replace(b, ToastSpec::new(ToastKind::Info, "x"), t));
    }

    #[test]
    fn agent_toast_carries_tool_and_client() {
        let spec = ToastSpec::agent("start_recording", "claude-desktop");
        assert_eq!(spec.kind, ToastKind::Agent);
        assert_eq!(spec.title, "Started by agent via MCP");
        let a = spec.agent.clone().unwrap();
        assert_eq!(
            (a.tool.as_str(), a.client.as_str()),
            ("start_recording", "claude-desktop")
        );
        let mut q = ToastQueue::default();
        let now = Instant::now();
        q.push(spec, now);
        // Not sticky: it auto-dismisses like any informational toast.
        assert_eq!(q.next_expiry(now), Some(AUTO_DISMISS));
        let plain = estimate_height(&Toast {
            agent: None,
            ..q.toasts()[0].clone()
        });
        assert!(estimate_height(&q.toasts()[0]) > plain);
    }

    /// The estimate counts what the card really takes: the 6 px inset on both sides, the 1 px
    /// border, 14 px padding top and bottom and a content row of at least 30 px (the icon tile), so
    /// a one-line toast is 72 px; a body adds the 30 px title row (not 20) plus the gap and lines.
    /// The window is also fitted to the painted height (`app::fit_toast_window`), this is the start.
    #[test]
    fn a_one_line_toast_is_72_px_and_a_body_adds_to_the_title_row() {
        let mut q = ToastQueue::default();
        q.push(ToastSpec::new(ToastKind::Info, "Saved"), Instant::now());
        assert_eq!(estimate_height(&q.toasts()[0]), 72.);
        let mut q = ToastQueue::default();
        q.push(
            ToastSpec::new(ToastKind::Info, "Saved").body("A short line."),
            Instant::now(),
        );
        // 12 inset + 2 border + 28 padding + 30 title row + 4 gap + 18 for one body line.
        assert_eq!(
            estimate_height(&q.toasts()[0]),
            12. + 2. + 28. + 30. + 4. + 18.
        );
    }

    /// Redesign R17: the agent toast joins the one bottom-right stack of at most three.
    #[test]
    fn the_agent_toast_joins_the_single_stack() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        q.push(ToastSpec::new(ToastKind::Success, "saved"), t);
        q.push(ToastSpec::agent("start_recording", "claude-desktop"), t);
        q.push(spec(ToastKind::Info), t);
        q.push(spec(ToastKind::Info), t);
        assert_eq!(q.toasts().len(), MAX_TOASTS);
        assert!(q.toasts().iter().all(|x| x.kind != ToastKind::Success));
    }

    /// Redesign R17: every kind has its own icon shape.
    #[test]
    fn each_kind_has_its_own_icon() {
        let kinds = [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Error,
            ToastKind::Agent,
        ];
        let icons: Vec<_> = kinds.iter().map(|k| k.icon()).collect();
        assert_eq!(
            icons,
            [
                "info",
                "circle-check",
                "triangle-alert",
                "octagon-alert",
                "bot"
            ]
        );
    }

    /// Which toasts fade (C7): no buttons and not an error; the agent toast fades.
    #[test]
    fn stickiness_decision_per_kind() {
        let t = Instant::now();
        let mk = |kind, buttons: bool| {
            let mut s = ToastSpec::new(kind, "x");
            if buttons {
                s = s.button("Open", ToastAction::OpenLibrary);
            }
            let mut q = ToastQueue::default();
            q.push(s, t);
            q.toasts()[0].sticky()
        };
        for kind in [
            ToastKind::Info,
            ToastKind::Success,
            ToastKind::Warning,
            ToastKind::Agent,
        ] {
            assert!(!mk(kind, false), "{kind:?} without buttons fades");
            assert!(mk(kind, true), "{kind:?} with buttons stays");
        }
        assert!(mk(ToastKind::Error, false), "errors stay");
    }

    /// R17: the clock stops while the pointer is over a toast and resumes with what was left.
    #[test]
    fn hover_pauses_the_auto_dismiss_clock() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        let id = q.push(spec(ToastKind::Success), t);
        assert!(q.set_hover(id, true, t + Duration::from_secs(4)));
        assert!(
            !q.set_hover(id, true, t + Duration::from_secs(5)),
            "no change"
        );
        assert_eq!(q.next_expiry(t + Duration::from_secs(10)), None);
        assert!(
            !q.expire(t + Duration::from_secs(3600)),
            "paused: never expires"
        );
        // Leaving 10 s after entering: 2 s of the 6 s were left.
        assert!(q.set_hover(id, false, t + Duration::from_secs(14)));
        assert_eq!(
            q.next_expiry(t + Duration::from_secs(14)),
            Some(Duration::from_secs(2))
        );
        assert!(!q.expire(t + Duration::from_millis(15_999)));
        assert!(q.expire(t + Duration::from_secs(16)));
        assert!(!q.set_hover(id, true, t), "gone");
    }

    #[test]
    fn every_action_has_an_icon() {
        use ToastAction::*;
        for a in [
            OpenLibrary,
            OpenCapture("a".into()),
            CopyCapture("a".into()),
            SaveCaptureAs("a".into()),
            DeleteCapture("a".into()),
            OpenScreenRecordingSettings,
            OpenPrivacyPane("p"),
            OpenPermissions,
            PlayExport("a".into()),
            RevealExport("a".into()),
            CopyLink("a".into()),
            OpenLink("a".into()),
            RevealPath("a".into()),
            RetryJob("a".into()),
            OpenDestinations,
            StopRecording("a".into()),
            CancelJob("a".into()),
        ] {
            assert!(!a.icon().is_empty());
        }
    }

    /// Redesign R17: 440 px wide.
    #[test]
    fn toasts_are_440_wide() {
        assert_eq!(TOAST_WIDTH, 440.);
    }

    /// R17: the agent toast can carry Stop; a running toast with a bar is sticky (it has Cancel),
    /// is replaced in place and grows by the bar.
    #[test]
    fn agent_stop_and_progress_toasts() {
        let mut q = ToastQueue::default();
        let t = Instant::now();
        let a = q.push(
            ToastSpec::agent_with_stop("start_recording", "claude", "rec_9"),
            t,
        );
        let toast = &q.toasts()[0];
        assert_eq!(toast.kind, ToastKind::Agent);
        assert!(toast.agent.is_some());
        assert_eq!(
            toast.buttons[0].action,
            ToastAction::StopRecording("rec_9".into())
        );
        assert!(toast.sticky(), "a toast with an action stays");
        q.dismiss(a);
        let mut spec = ToastSpec::new(ToastKind::Info, "Uploading")
            .button("Cancel", ToastAction::CancelJob("j".into()));
        spec.progress = Some(38);
        let id = q.push(spec.clone(), t);
        assert_eq!(q.next_expiry(t), None, "never auto-dismissed while running");
        let with_bar = estimate_height(&q.toasts()[0]);
        let mut plain = q.toasts()[0].clone();
        plain.progress = None;
        assert!(with_bar > estimate_height(&plain));
        spec.progress = Some(50);
        assert!(q.replace(id, spec, t));
        assert_eq!(q.toasts()[0].progress, Some(50));
        assert_eq!(q.toasts().len(), 1, "updated in place");
    }

    #[test]
    fn dismiss_by_id() {
        let mut q = ToastQueue::default();
        let id = q.push(spec(ToastKind::Error), Instant::now());
        assert!(q.dismiss(id));
        assert!(!q.dismiss(id));
        assert!(q.is_empty());
    }
}
