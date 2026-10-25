//! The daemon's `Confirmer` for the desktop app (AGT-2, DESIGN section 5.6).
//!
//! * [`DialogModel`]: what the modal shows, built from the daemon's `ConfirmationRequest`.
//! * [`EnterGuard`]: Enter is ignored for 1 s after the dialog appears (AGT-2.AC2).
//! * [`UiConfirmer`]: implements `cm_daemon::Confirmer` by sending the request to the UI
//!   thread and waiting for the user's answer. Dropping the `confirm` future (client gone,
//!   5 min timeout) closes the dialog (AGT-2.AC3).

use async_trait::async_trait;
use cm_daemon::{ConfirmationRequest, Confirmer, Decision};
use cm_i18n::{tr, tr_ctx, trn};
use serde_json::Value;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

use crate::daemon_text;

/// The dialog is 520 px wide (redesign R17: the title row carries an icon tile and the agent chip).
pub const DIALOG_WIDTH: f32 = 520.;

/// Height of the dialog panel in px, estimated from its content (the window is sized from
/// it; generous so nothing is clipped).
pub fn dialog_height(m: &DialogModel) -> f32 {
    let summary_lines = m.summary.chars().count().div_ceil(60).max(1) as f32;
    // Padding, the 44 px title row, the summary.
    let mut h = 24. + 44. + 14. + 21. * summary_lines;
    // A long title wraps next to the tile (and the agent chip takes room from it).
    let per_line = if m.agent_name().is_some() { 24 } else { 36 };
    let title_lines = m.title.chars().count().div_ceil(per_line).max(1);
    h += 22. * (title_lines - 1) as f32;
    if !m.details.is_empty() {
        // The details box: a long value (a file path, a title) wraps onto more lines.
        let rows: f32 = m
            .details
            .iter()
            .map(|(k, v)| (k.chars().count() + v.chars().count()).div_ceil(62).max(1) as f32)
            .sum();
        h += 14. + 24. + 22. * rows;
    }
    if m.checkbox_label().is_some() {
        h += 14. + 22. + 21. * (m.option.as_deref().map_or(0, |o| o.chars().count()) / 58) as f32;
    }
    // The footer: a rule, 16 px padding and the 40 px buttons.
    h + 20. + 1. + 16. + 40. + 16.
}

/// Space kept free around the dialog on the visible part of the display.
pub const DIALOG_MARGIN: f32 = 24.;

/// The window height for a dialog that wants `wanted` px on a display whose visible part is
/// `visible_h` tall: as wanted when it fits with margins, else as tall as the display allows
/// (the text scrolls, the buttons stay).
pub fn fit_dialog_height(wanted: f32, visible_h: f32) -> f32 {
    wanted.min((visible_h - 2. * DIALOG_MARGIN).max(1.))
}

/// AGT-2.AC2.
pub const ENTER_IGNORE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogModel {
    pub id: String,
    /// "Requested by Claude Desktop" (agent chip), in the UI language. Empty for a question the
    /// app asks itself (the HUD's Discard and Restart): no chip is drawn.
    pub chip: String,
    /// Who asks (`Claude Desktop`); empty for the app's own question.
    pub agent: String,
    /// The tool the agent called (`publish_video`); empty for the app's own question.
    pub tool: String,
    pub title: String,
    /// One plain sentence of what will happen.
    pub summary: String,
    /// File, account, privacy, size ... as label/value rows.
    pub details: Vec<(String, String)>,
    /// Names the exact action, never "OK".
    pub primary_label: String,
    /// Destructive primary (Delete): named for the action; styled like any primary (OD-12).
    pub primary_is_danger: bool,
    /// The primary starts a recording (`start_recording`): rec red like the Record button
    /// (OD-17); every other non-destructive primary is neutral.
    pub primary_is_record: bool,
    /// Offer the "Allow for this session" checkbox (`start_recording` only).
    pub allow_session: bool,
    /// A checkbox of the app's own question, off by default (the library Delete's "Also delete
    /// the copies online"). Ticked, the answer is `Decision::ApproveForSession`.
    pub option: Option<String>,
}

impl DialogModel {
    /// Who is asking, for the agent chip (`Claude Desktop`); `None` for the app's own question.
    pub fn agent_name(&self) -> Option<&str> {
        Some(self.agent.as_str()).filter(|n| !n.is_empty())
    }

    /// The icon of the title tile: Record for a start, trash for anything destructive, send for a
    /// publish, a shield otherwise.
    pub fn title_icon(&self) -> &'static str {
        if self.primary_is_record {
            "cm-record"
        } else if self.primary_is_danger {
            "trash-2"
        } else if self.tool == "publish_video" {
            "send"
        } else {
            "shield-alert"
        }
    }

    /// The primary button's icon: the trash for a destructive confirm (OD-17), the Record glyph
    /// for a start, send for a publish, none otherwise.
    pub fn primary_icon(&self) -> Option<&'static str> {
        match self.title_icon() {
            "shield-alert" => None,
            i => Some(i),
        }
    }

    /// The label of the checkbox the dialog shows, if any.
    pub fn checkbox_label(&self) -> Option<String> {
        match (&self.option, self.allow_session) {
            (Some(o), _) => Some(o.clone()),
            (None, true) => Some(tr!("Allow for this session")),
            (None, false) => None,
        }
    }
}

/// `claude-desktop` -> `Claude Desktop`.
pub fn display_client_name(raw: &str) -> String {
    raw.split(['-', '_', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut cs = w.chars();
            match cs.next() {
                Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn platform_name(p: &str) -> String {
    match p {
        "youtube" => "YouTube".to_owned(),
        "tiktok" => "TikTok".to_owned(),
        "facebook" => "Facebook".to_owned(),
        other => daemon_text::humanize_key(other),
    }
}

impl DialogModel {
    pub fn from_request(r: &ConfirmationRequest) -> DialogModel {
        let ask_first = r.kind == cm_daemon::tool_policy::KIND_ASK_FIRST;
        let (title, primary, danger) = match r.tool.as_str() {
            // AGT-2.AC11: a tool the user set to Ask first; the sentence and rows come from the
            // typed parameters below.
            _ if ask_first => (tr!("Allow this action?"), tr_ctx!("button", "Allow"), false),
            "start_recording" => (tr!("Start recording?"), tr!("Start recording"), false),
            "publish_video" => {
                let label = r
                    .details
                    .get("platform")
                    .and_then(Value::as_str)
                    .map(|p| tr!("Publish to {platform}", platform = platform_name(p)))
                    .unwrap_or_else(|| tr!("Publish video"));
                (tr!("Publish video?"), label, false)
            }
            "delete_capture" => (tr!("Delete capture?"), tr!("Delete capture"), true),
            // LIB-2: an agent asks to change the library folder.
            "move_library" => {
                let mode = r
                    .params
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("move");
                let label = match mode {
                    "fresh" => tr!("Start a new library"),
                    "use_existing" => tr!("Use that library"),
                    _ => tr!("Move the library"),
                };
                (tr!("Change the library folder?"), label, false)
            }
            // REC-5: discarding or restarting deletes the take; both are destructive (red is
            // reserved: a destructive primary looks like any primary, per OD-12) and name the exact action.
            "discard_recording" => (tr!("Discard this take?"), tr!("Discard take"), true),
            "restart_recording" => (
                tr!("Discard this take and start over?"),
                tr!("Discard and restart"),
                true,
            ),
            _ => (
                tr!("Allow this action?"),
                daemon_text::humanize_key(&r.tool),
                false,
            ),
        };
        // The Tool row only exists for agent requests (existing information, redesign R17).
        let mut details = vec![(daemon_text::row_label("tool"), r.tool.clone())];
        // The wording comes from `kind` and `params` (P3); the daemon's English `summary` and
        // `details` are what shows for a kind or a field this version does not know.
        let kind = if r.kind.is_empty() { &r.tool } else { &r.kind };
        let params = daemon_text::params_of(&r.params);
        details.extend(daemon_text::confirm_rows(kind, &r.details, &params));
        if let Some(n) = ask_first
            .then(|| params.get("omitted").and_then(Value::as_u64))
            .flatten()
            .filter(|n| *n > 0)
        {
            details.push((
                tr!("More"),
                trn!(
                    "{n} more parameter not shown",
                    "{n} more parameters not shown",
                    n
                ),
            ));
        }
        let agent = display_client_name(&r.client);
        DialogModel {
            id: r.id.clone(),
            chip: tr!("Requested by {name}", name = agent),
            agent,
            tool: r.tool.clone(),
            title,
            summary: daemon_text::confirm_summary(kind, &params, &r.summary),
            details,
            primary_label: primary,
            primary_is_danger: danger,
            primary_is_record: r.tool == "start_recording",
            allow_session: r.allow_session,
            option: None,
        }
    }
}

/// Id of the dialog the HUD opens for Discard and Restart.
pub const LOCAL_DISCARD_ID: &str = "local_discard";

/// Id of the dialog before deleting a capture from the toast or the library.
pub const LOCAL_DELETE_ID: &str = "local_delete";

/// CAP-4.AC1 / LIB-1: "Delete capture?" asked by the app itself (no agent chip). The capture goes
/// to the system Trash, so the words say it can be restored.
pub fn delete_dialog(title: &str) -> DialogModel {
    DialogModel {
        id: LOCAL_DELETE_ID.to_owned(),
        chip: String::new(),
        agent: String::new(),
        tool: String::new(),
        title: tr!("Delete this capture?"),
        summary: tr!("It is removed from your library and moved to the Trash."),
        details: vec![(tr!("Capture"), title.to_owned())],
        primary_label: tr!("Delete capture"),
        primary_is_danger: true,
        primary_is_record: false,
        allow_session: false,
        option: None,
    }
}

/// Id of the dialog before letting agents start recordings without asking.
pub const LOCAL_LOOSEN_ID: &str = "local_loosen_start";

/// Id of the dialog before resetting several tool permissions.
pub const LOCAL_RESET_TOOLS_ID: &str = "local_reset_tools";

fn local_dialog(
    id: &str,
    title: String,
    summary: String,
    details: Vec<(String, String)>,
    primary_label: String,
) -> DialogModel {
    DialogModel {
        id: id.to_owned(),
        chip: String::new(),
        agent: String::new(),
        tool: String::new(),
        title,
        summary,
        details,
        primary_label,
        primary_is_danger: false,
        primary_is_record: false,
        allow_session: false,
        option: None,
    }
}

/// AGT-2.AC13: asked by Settings -> Agents & CLI before `start_recording` is set to Allow. Asked
/// by the app itself (no agent chip); only after the answer is the setting written.
pub fn loosen_start_dialog() -> DialogModel {
    local_dialog(
        LOCAL_LOOSEN_ID,
        tr!("Let AI agents start recordings without asking?"),
        tr!("An AI agent will be able to start a recording as soon as it asks, with no confirmation. What it records (the screen, camera and microphone) follows your Recording settings. The recording indicator and the tray icon still show, and you can stop from the toast. You can turn this off here at any time."),
        Vec::new(),
        tr!("Allow without asking"),
    )
}

/// AGT-2.AC9: asked before "Reset to defaults" when more than three tools were changed.
pub fn reset_tools_dialog(changed: usize) -> DialogModel {
    local_dialog(
        LOCAL_RESET_TOOLS_ID,
        tr!("Reset tool permissions?"),
        trn!(
            "{n} tool is set differently from its default. Resetting puts every tool back to its default: tools that change something run without asking, and starting a recording asks first.",
            "{n} tools are set differently from their defaults. Resetting puts every tool back to its default: tools that change something run without asking, and starting a recording asks first.",
            changed
        ),
        Vec::new(),
        tr!("Reset to defaults"),
    )
}

/// The HUD's confirmation before deleting a take (REC-5.AC3, AC12): "Discard this take?" for
/// Discard, "Discard this take and start over?" for Restart. Asked by the app itself, so no agent
/// chip. `recorded` is the timer text of what would be lost.
pub fn discard_dialog(restart: bool, recorded: &str, title: Option<&str>) -> DialogModel {
    let mut details = vec![(tr!("Recorded"), recorded.to_owned())];
    if let Some(t) = title.filter(|t| !t.is_empty()) {
        details.insert(0, (tr!("Recording"), t.to_owned()));
    }
    DialogModel {
        id: LOCAL_DISCARD_ID.to_owned(),
        chip: String::new(),
        agent: String::new(),
        tool: String::new(),
        title: if restart {
            tr!("Discard this take and start over?")
        } else {
            tr!("Discard this take?")
        },
        summary: if restart {
            tr!("The recording so far is deleted and a new one starts after the countdown, with the same settings.")
        } else {
            tr!("The recording is stopped and its files are deleted.")
        },
        details,
        primary_label: if restart {
            tr!("Discard and restart")
        } else {
            tr!("Discard take")
        },
        primary_is_danger: true,
        primary_is_record: false,
        allow_session: false,
        option: None,
    }
}

/// AGT-2.AC2: Enter does nothing until `ENTER_IGNORE` has passed since the dialog appeared.
#[derive(Debug, Clone, Copy)]
pub struct EnterGuard {
    shown_at: Instant,
}

impl EnterGuard {
    pub fn new(shown_at: Instant) -> Self {
        EnterGuard { shown_at }
    }

    pub fn accepts_enter(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.shown_at) >= ENTER_IGNORE
    }
}

/// Messages from the daemon side to the UI thread.
#[allow(clippy::large_enum_variant)] // one short-lived message per dialog
pub enum ConfirmUiMsg {
    Show {
        model: DialogModel,
        reply: oneshot::Sender<Decision>,
    },
    /// The request ended without an answer (timeout, client disconnected): close its dialog.
    Close { id: String },
}

#[derive(Clone)]
pub struct UiConfirmer {
    tx: mpsc::UnboundedSender<ConfirmUiMsg>,
}

impl UiConfirmer {
    pub fn channel() -> (UiConfirmer, mpsc::UnboundedReceiver<ConfirmUiMsg>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (UiConfirmer { tx }, rx)
    }
}

/// Sends `Close` when the `confirm` future ends or is dropped; closing an already answered
/// dialog is a no-op on the UI side.
struct CloseOnDrop {
    tx: mpsc::UnboundedSender<ConfirmUiMsg>,
    id: String,
}

impl Drop for CloseOnDrop {
    fn drop(&mut self) {
        let _ = self.tx.send(ConfirmUiMsg::Close {
            id: std::mem::take(&mut self.id),
        });
    }
}

#[async_trait]
impl Confirmer for UiConfirmer {
    async fn confirm(&self, request: &ConfirmationRequest) -> Decision {
        let (reply, answer) = oneshot::channel();
        let model = DialogModel::from_request(request);
        let guard = CloseOnDrop {
            tx: self.tx.clone(),
            id: model.id.clone(),
        };
        if self.tx.send(ConfirmUiMsg::Show { model, reply }).is_err() {
            // No UI to ask: never approve on the user's behalf.
            return Decision::Decline;
        }
        let decision = answer.await.unwrap_or(Decision::Decline);
        drop(guard);
        decision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(tool: &str, details: Value, allow_session: bool) -> ConfirmationRequest {
        ConfirmationRequest {
            id: "conf_1".into(),
            client: "claude-desktop".into(),
            tool: tool.into(),
            summary: "Publish 'Kubernetes probes in 5 minutes' to YouTube as Unlisted".into(),
            details,
            kind: tool.into(),
            params: json!({}),
            allow_session,
        }
    }

    /// DESIGN 5.6: agent chip, one sentence, details, an action-naming primary button.
    #[test]
    fn publish_dialog_model() {
        let m = DialogModel::from_request(&request(
            "publish_video",
            json!({"platform": "youtube", "privacy": "unlisted", "capture_id": "rec_1", "skip": null}),
            false,
        ));
        assert_eq!(m.chip, "Requested by Claude Desktop");
        assert_eq!(m.primary_label, "Publish to YouTube");
        assert!(!m.primary_is_danger && !m.allow_session);
        assert!(m.details.contains(&("Privacy".into(), "unlisted".into())));
        assert!(m.details.iter().all(|(k, _)| k != "Skip"));
        assert!(m.summary.contains("Unlisted"));
    }

    /// AGT-2: "Allow for this session" is offered for start_recording only; delete is red.
    #[test]
    fn tool_specific_buttons() {
        let rec = DialogModel::from_request(&request("start_recording", json!({}), true));
        assert_eq!(rec.primary_label, "Start recording");
        assert!(rec.allow_session);
        let del = DialogModel::from_request(&request("delete_capture", json!({}), false));
        assert_eq!(del.primary_label, "Delete capture");
        assert!(del.primary_is_danger && !del.allow_session);
        // OD-17: only Start recording is rec red among the non-destructive primaries.
        assert!(rec.primary_is_record && !del.primary_is_record);
    }

    /// AGT-2.AC2: Enter is ignored for the first second, then accepted.
    #[test]
    fn enter_is_ignored_for_one_second() {
        let t0 = Instant::now();
        let g = EnterGuard::new(t0);
        assert!(!g.accepts_enter(t0));
        assert!(!g.accepts_enter(t0 + Duration::from_millis(999)));
        assert!(g.accepts_enter(t0 + Duration::from_secs(1)));
        // A clock that went backwards never accepts early.
        assert!(!g.accepts_enter(t0 - Duration::from_millis(1)));
    }

    /// AGT-2.AC1: nothing is approved until the user answers.
    #[tokio::test]
    async fn waits_for_the_user_then_returns_the_decision() {
        let (c, mut rx) = UiConfirmer::channel();
        let req = request("start_recording", json!({}), true);
        let task = tokio::spawn({
            let c = c.clone();
            async move { c.confirm(&req).await }
        });
        let Some(ConfirmUiMsg::Show { model, reply }) = rx.recv().await else {
            panic!("no dialog requested");
        };
        assert_eq!(model.id, "conf_1");
        assert!(!task.is_finished());
        reply.send(Decision::ApproveForSession).unwrap();
        assert_eq!(task.await.unwrap(), Decision::ApproveForSession);
    }

    /// AGT-2.AC3: when the daemon gives up (timeout) the future is dropped and the dialog
    /// is told to close.
    #[tokio::test]
    async fn dropping_the_future_closes_the_dialog() {
        let (c, mut rx) = UiConfirmer::channel();
        let req = request("delete_capture", json!({}), false);
        let fut = c.confirm(&req);
        let res = tokio::time::timeout(Duration::from_millis(20), fut).await;
        assert!(res.is_err());
        assert!(matches!(rx.recv().await, Some(ConfirmUiMsg::Show { .. })));
        assert!(matches!(rx.recv().await, Some(ConfirmUiMsg::Close { id }) if id == "conf_1"));
    }

    #[tokio::test]
    async fn no_ui_means_decline() {
        let (c, rx) = UiConfirmer::channel();
        drop(rx);
        assert_eq!(
            c.confirm(&request("start_recording", json!({}), true))
                .await,
            Decision::Decline
        );
    }

    #[tokio::test]
    async fn dialog_dropped_without_answer_declines() {
        let (c, mut rx) = UiConfirmer::channel();
        let req = request("start_recording", json!({}), true);
        let task = tokio::spawn({
            let c = c.clone();
            async move { c.confirm(&req).await }
        });
        if let Some(ConfirmUiMsg::Show { reply, .. }) = rx.recv().await {
            drop(reply);
        }
        assert_eq!(task.await.unwrap(), Decision::Decline);
    }

    /// Redesign R17: the agent chip shows the client, the Tool row exists only for agent
    /// requests, the session checkbox only for `start_recording`, and the icons and primary kinds
    /// follow the action.
    #[test]
    fn r17_confirm_dialog_model() {
        let publish = DialogModel::from_request(&request(
            "publish_video",
            json!({"platform": "youtube"}),
            false,
        ));
        assert_eq!(publish.agent_name(), Some("Claude Desktop"));
        assert_eq!(publish.details[0], ("Tool".into(), "publish_video".into()));
        assert_eq!(publish.checkbox_label(), None, "12-agent.md:52");
        assert_eq!(publish.title_icon(), "send");
        assert_eq!(publish.primary_icon(), Some("send"));
        assert!(!publish.primary_is_danger && !publish.primary_is_record);

        let start = DialogModel::from_request(&request("start_recording", json!({}), true));
        assert_eq!(
            start.checkbox_label().as_deref(),
            Some("Allow for this session")
        );
        assert_eq!(start.title_icon(), "cm-record");
        assert!(start.primary_is_record);

        let del = DialogModel::from_request(&request("delete_capture", json!({}), false));
        assert_eq!(del.title_icon(), "trash-2");
        assert_eq!(del.primary_icon(), Some("trash-2"));
        assert!(del.primary_is_danger);

        let other = DialogModel::from_request(&request("something_else", json!({}), false));
        assert_eq!(other.primary_icon(), None);

        // The app's own questions: no chip, no Tool row.
        let local = delete_dialog("Recording 1");
        assert_eq!(local.agent_name(), None);
        assert!(local.details.iter().all(|(k, _)| k != "Tool"));
        assert_eq!(local.title_icon(), "trash-2");
        let discard = discard_dialog(false, "00:42", None);
        assert_eq!(discard.agent_name(), None);
        assert!(discard.details.iter().all(|(k, _)| k != "Tool"));
        assert_eq!(DIALOG_WIDTH, 520.);
    }

    #[test]
    fn dialog_never_outgrows_the_display() {
        assert_eq!(fit_dialog_height(300., 900.), 300.);
        assert_eq!(fit_dialog_height(900., 600.), 600. - 2. * DIALOG_MARGIN);
        assert!(fit_dialog_height(900., 10.) >= 1.);
    }

    #[test]
    fn long_detail_values_make_the_dialog_taller() {
        let mut m = DialogModel::from_request(&request("delete_capture", json!({}), false));
        m.details = vec![("File".into(), "a".repeat(10))];
        let short = dialog_height(&m);
        m.details = vec![("File".into(), "a".repeat(200))];
        assert!(dialog_height(&m) > short + 40.);
    }

    #[test]
    fn dialog_height_grows_with_details() {
        let base = DialogModel::from_request(&request("delete_capture", json!({}), false));
        let more = DialogModel::from_request(&request(
            "start_recording",
            json!({"a": 1, "b": 2, "c": 3}),
            true,
        ));
        assert!(dialog_height(&more) > dialog_height(&base) + 60.);
    }

    /// REC-5 / AGT-2: agent-requested discard and restart name the action and use the danger button.
    #[test]
    fn discard_and_restart_tools_are_destructive_dialogs() {
        let d = DialogModel::from_request(&request(
            "discard_recording",
            json!({"recorded": "00:42"}),
            false,
        ));
        assert_eq!(d.title, "Discard this take?");
        assert_eq!(d.primary_label, "Discard take");
        assert!(d.primary_is_danger && !d.allow_session);
        assert!(d.details.contains(&("Recorded".into(), "00:42".into())));
        let r = DialogModel::from_request(&request("restart_recording", json!({}), false));
        assert_eq!(r.title, "Discard this take and start over?");
        assert_eq!(r.primary_label, "Discard and restart");
        assert!(r.primary_is_danger);
    }

    /// REC-5.AC3 / AC12: the HUD's own dialogs have no agent chip, name the action, and cancel is the
    /// safe default (the window focuses Cancel).
    #[test]
    fn rec5_hud_discard_dialogs() {
        let d = discard_dialog(false, "00:42", Some("Recording 10.30"));
        assert!(d.chip.is_empty() && d.primary_is_danger && !d.allow_session);
        assert_eq!(d.title, "Discard this take?");
        assert_eq!(d.details[0], ("Recording".into(), "Recording 10.30".into()));
        let r = discard_dialog(true, "00:10", None);
        assert_eq!(r.title, "Discard this take and start over?");
        assert_eq!(r.primary_label, "Discard and restart");
        assert_eq!(r.details, vec![("Recorded".to_owned(), "00:10".to_owned())]);
    }

    #[test]
    fn delete_dialog_names_the_capture_and_the_action() {
        let d = delete_dialog("Screenshot 10.30");
        assert!(d.chip.is_empty() && d.primary_is_danger && !d.allow_session);
        assert_eq!(d.primary_label, "Delete capture");
        assert_eq!(d.details[0].1, "Screenshot 10.30");
        assert!(d.summary.contains("Trash"));
    }

    /// P3: the dialog is worded from `kind` and `params`; in English it reads as the daemon's own
    /// text, in another language every sentence, label and button changes, and an unknown kind
    /// shows the daemon's English.
    #[test]
    fn p3_dialog_text_follows_the_language() {
        use cm_i18n::{with_language, Lang};
        let mut r = request(
            "delete_capture",
            json!({"capture": "Take", "size": "2.4 MB", "result": "Moved to the Trash"}),
            false,
        );
        r.summary = "Delete \"Take\" (2.4 MB)".into();
        r.params = json!({"title": "Take", "size_bytes": 2_400_000, "remote_copies": 0});
        let en = DialogModel::from_request(&r);
        assert_eq!(en.summary, "Delete \"Take\" (2.4 MB)");
        assert_eq!(en.title, "Delete capture?");
        assert_eq!(en.chip, "Requested by Claude Desktop");
        assert_eq!(en.agent_name(), Some("Claude Desktop"));
        assert_eq!(
            en.details[3],
            ("Result".into(), "Moved to the Trash".into())
        );
        with_language(Lang::EnXa, || {
            let x = DialogModel::from_request(&r);
            assert_ne!(x.summary, en.summary);
            assert_ne!(x.title, en.title);
            assert_ne!(x.primary_label, en.primary_label);
            assert_ne!(x.chip, en.chip);
            assert_ne!(x.details[0].0, en.details[0].0, "the Tool label");
            assert_ne!(x.details[3].1, en.details[3].1, "the result sentence");
            assert_eq!(
                x.agent_name(),
                Some("Claude Desktop"),
                "a name is not translated"
            );
            assert_ne!(
                delete_dialog("Take").summary,
                delete_dialog_english_summary()
            );
            assert_ne!(
                discard_dialog(true, "00:10", None).title,
                "Discard this take and start over?"
            );
        });
        // A kind from a newer daemon: its English sentence, a humanized title.
        let mut n = request("brand_new", json!({"x": "y"}), false);
        n.summary = "Do the brand new thing".into();
        let m = DialogModel::from_request(&n);
        assert_eq!(m.summary, "Do the brand new thing");
        assert_eq!(m.primary_label, "Brand new");
    }

    /// AGT-2.AC11: the generic dialog of a tool set to Ask first, built from what the daemon
    /// really sends: the agent chip, the tool row and the parameter rows, a neutral primary.
    #[test]
    fn agt_2_ac11_ask_first_dialog_model() {
        let spec = *cm_daemon::spec_for("rename_capture").unwrap();
        let input = json!({"capture_id": "shot_1", "title": "x".repeat(300), "request_id": "r"});
        let (summary, details, params) =
            cm_daemon::tool_policy::ask_first_dialog("claude-desktop", &spec, &input);
        let r = ConfirmationRequest {
            id: "conf_9".into(),
            client: "claude-desktop".into(),
            tool: "rename_capture".into(),
            summary,
            details,
            kind: cm_daemon::tool_policy::KIND_ASK_FIRST.into(),
            params,
            allow_session: false,
        };
        let m = DialogModel::from_request(&r);
        assert_eq!(m.title, "Allow this action?");
        assert_eq!(m.primary_label, "Allow");
        assert!(!m.primary_is_danger && !m.primary_is_record && !m.allow_session);
        assert_eq!(m.agent_name(), Some("Claude Desktop"));
        assert_eq!(m.chip, "Requested by Claude Desktop");
        assert_eq!(
            m.summary,
            "Claude Desktop is asking to run \u{201C}Rename a capture\u{201D}. You set this tool to Ask first in Settings \u{2192} Agents & CLI."
        );
        assert_eq!(m.details[0], ("Tool".into(), "rename_capture".into()));
        let get = |k: &str| {
            m.details
                .iter()
                .find(|(l, _)| l == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("Capture id").as_deref(), Some("shot_1"));
        assert_eq!(
            get("Title").unwrap().chars().count(),
            123,
            "cut at 120 characters"
        );
        assert!(m.details.iter().all(|(k, _)| k != "Request id"));
        assert_eq!(m.title_icon(), "shield-alert");
        assert_eq!(m.checkbox_label(), None);

        // Parameters that did not fit are counted in a last row.
        let mut many = r.clone();
        many.params["omitted"] = json!(3);
        let m = DialogModel::from_request(&many);
        assert_eq!(
            m.details.last().unwrap(),
            &("More".to_owned(), "3 more parameters not shown".to_owned())
        );
        many.params["omitted"] = json!(1);
        assert_eq!(
            DialogModel::from_request(&many).details.last().unwrap().1,
            "1 more parameter not shown"
        );
        // A tool the app has no title for still reads.
        let mut future = r.clone();
        future.tool = "brand_new_tool".into();
        future.params["tool"] = json!("brand_new_tool");
        future.params["title"] = json!("Brand new tool");
        assert!(DialogModel::from_request(&future)
            .summary
            .contains("\u{201C}Brand new tool\u{201D}"));
    }

    /// AGT-2.AC13 / AC9: the two questions Settings asks itself have no agent chip, a neutral
    /// primary that names the action, and follow the language.
    #[test]
    fn agt_2_ac13_settings_dialogs() {
        use cm_i18n::{with_language, Lang};
        let l = loosen_start_dialog();
        assert_eq!(l.title, "Let AI agents start recordings without asking?");
        assert_eq!(l.primary_label, "Allow without asking");
        assert!(l.agent_name().is_none() && !l.primary_is_danger && !l.primary_is_record);
        for needle in [
            "screen, camera and microphone",
            "Recording settings",
            "indicator",
            "toast",
        ] {
            assert!(l.summary.contains(needle), "{needle}: {}", l.summary);
        }
        let r4 = reset_tools_dialog(4);
        assert!(r4.summary.starts_with("4 tools are set differently"));
        assert!(reset_tools_dialog(1)
            .summary
            .starts_with("1 tool is set differently"));
        assert_eq!(r4.primary_label, "Reset to defaults");
        assert_ne!(l.id, r4.id);
        with_language(Lang::EnXa, || {
            assert_ne!(loosen_start_dialog().title, l.title);
            assert_ne!(reset_tools_dialog(4).summary, r4.summary);
        });
    }

    fn delete_dialog_english_summary() -> String {
        "It is removed from your library and moved to the Trash.".to_owned()
    }

    #[test]
    fn client_names() {
        assert_eq!(display_client_name("claude-desktop"), "Claude Desktop");
        assert_eq!(display_client_name("cursor"), "Cursor");
    }
}
