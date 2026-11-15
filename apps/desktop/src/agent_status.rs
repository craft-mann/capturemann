//! "MCP" status for the sidebar card and the Agents page, derived from what the app really
//! knows: whether the embedded daemon serves the IPC endpoint and the daemon's agent activity
//! log. The log only holds gated or refused requests (confirmations, read-only and blocked
//! clients), not every agent call, so "last call" means the last such request.

use cm_daemon::{ActivityDecision, ActivityEntry};
use cm_i18n::{tr, tr_ctx};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpState {
    /// The daemon runs and serves the IPC endpoint the CLI and `mcp` bridge connect to.
    Ready,
    /// Nothing can connect; the reason is worded when drawn ([`OfflineReason::text`]).
    Offline(OfflineReason),
}

/// Why nothing can connect. Stored as a reason, not as text, so the line follows a language
/// switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineReason {
    LibraryUnavailable,
    NoEndpoint,
    SocketOwned,
    /// The folder of the socket could not be created or opened (permissions, a full disk).
    SocketFolder,
    /// The endpoint could not be opened for another reason.
    EndpointFailed,
}

/// Why binding the IPC endpoint failed, as far as the line under "MCP offline" tells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcFailure {
    /// Another daemon already listens on it.
    AlreadyServed,
    /// The folder holding the socket could not be made or used.
    Folder,
    /// Anything else (a path that is too long, a file in the way).
    Other,
}

impl IpcFailure {
    pub fn of(e: &std::io::Error) -> IpcFailure {
        use std::io::ErrorKind as K;
        match e.kind() {
            K::AddrInUse => IpcFailure::AlreadyServed,
            K::InvalidInput | K::InvalidData => IpcFailure::Other,
            _ => IpcFailure::Folder,
        }
    }
}

impl OfflineReason {
    pub fn text(self) -> String {
        match self {
            OfflineReason::LibraryUnavailable => tr!("Library unavailable"),
            OfflineReason::NoEndpoint => tr!("No IPC endpoint"),
            OfflineReason::SocketOwned => tr!("Another app owns the socket"),
            OfflineReason::SocketFolder => tr!("Could not create the socket folder"),
            OfflineReason::EndpointFailed => tr!("Could not open the IPC endpoint"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastCall {
    pub tool: String,
    pub client: String,
    pub decision: ActivityDecision,
}

/// The status shape beside the Agents & CLI icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusMark {
    /// Ready: a solid agent-blue dot.
    Dot,
    /// Offline (no library, no endpoint): a hollow ring.
    Ring,
    /// Another app owns the socket: an amber "!" badge.
    Bang,
}

/// What the Agents & CLI tooltip says under its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusTip {
    pub line: String,
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpStatus {
    pub state: McpState,
    pub last_call: Option<LastCall>,
}

impl McpStatus {
    pub fn is_ready(&self) -> bool {
        self.state == McpState::Ready
    }

    pub fn title(&self) -> String {
        if self.is_ready() {
            tr!("MCP ready")
        } else {
            tr!("MCP offline")
        }
    }

    /// The shape drawn at the Agents & CLI icon's bottom-right (board S14): colour is never the
    /// only cue.
    pub fn mark(&self) -> StatusMark {
        match &self.state {
            McpState::Ready => StatusMark::Dot,
            McpState::Offline(OfflineReason::SocketOwned) => StatusMark::Bang,
            McpState::Offline(_) => StatusMark::Ring,
        }
    }

    /// The status in the accessible name after "Agents & CLI, ": "MCP ready", "MCP offline" or
    /// "another app owns the socket". It names the real cause: a missing library is plain offline.
    pub fn aria_status(&self) -> String {
        match &self.state {
            McpState::Offline(OfflineReason::SocketOwned) => {
                tr!("another app owns the socket")
            }
            _ => self.title(),
        }
    }

    /// The status tooltip: the status line, then what it means for agents (board S14).
    pub fn tip(&self) -> StatusTip {
        match &self.state {
            McpState::Ready => StatusTip {
                line: tr!("MCP ready \u{b7} stdio and socket"),
                detail: self.last_call.iter().map(|c| tr!("Last call: {tool}", tool = c.tool)).collect(),
            },
            McpState::Offline(OfflineReason::SocketOwned) => StatusTip {
                line: OfflineReason::SocketOwned.text(),
                detail: vec![tr!(
                    "Agents can\u{2019}t reach CaptureMann. Open Agents & CLI to see which app and how to fix it."
                )],
            },
            McpState::Offline(OfflineReason::LibraryUnavailable) => StatusTip {
                line: tr!("MCP offline \u{b7} {reason}", reason = OfflineReason::LibraryUnavailable.text()),
                detail: vec![tr!(
                    "CaptureMann has no library, so agents can\u{2019}t reach it. Open Agents & CLI for details."
                )],
            },
            McpState::Offline(OfflineReason::NoEndpoint) => StatusTip {
                line: tr!("MCP offline \u{b7} {reason}", reason = OfflineReason::NoEndpoint.text()),
                detail: vec![tr!(
                    "CaptureMann is not serving an agent endpoint, so agents can\u{2019}t reach it. Open Agents & CLI for details."
                )],
            },
        }
    }

    /// The two mono lines under the title, as in the kit ("Last call: tool / from client").
    pub fn lines(&self) -> Vec<String> {
        match (&self.state, &self.last_call) {
            (McpState::Offline(reason), _) => vec![reason.text()],
            (McpState::Ready, Some(c)) => vec![
                tr!("Last call: {tool}", tool = c.tool),
                tr!("from {client}", client = c.client),
            ],
            (McpState::Ready, None) => vec![tr!("No agent requests yet")],
        }
    }
}

/// `daemon_up`: the library opened so a daemon exists. `ipc_failure`: set when the endpoint
/// could not be bound. `endpoint_configured`: the app was asked to serve one.
pub fn derive(
    daemon_up: bool,
    endpoint_configured: bool,
    ipc_failure: Option<IpcFailure>,
    activity: &[ActivityEntry],
) -> McpStatus {
    let state = if !daemon_up {
        McpState::Offline(OfflineReason::LibraryUnavailable)
    } else if !endpoint_configured {
        McpState::Offline(OfflineReason::NoEndpoint)
    } else if let Some(f) = ipc_failure {
        McpState::Offline(match f {
            IpcFailure::AlreadyServed => OfflineReason::SocketOwned,
            IpcFailure::Folder => OfflineReason::SocketFolder,
            IpcFailure::Other => OfflineReason::EndpointFailed,
        })
    } else {
        McpState::Ready
    };
    let last_call = activity.last().map(|e| LastCall {
        tool: e.tool.clone(),
        client: e.client.clone(),
        decision: e.decision,
    });
    McpStatus { state, last_call }
}

/// Entries added since the last poll. `seen` is how many the caller has already handled; if
/// the log shrank (30-day pruning) everything is treated as seen.
pub fn new_entries(seen: usize, activity: &[ActivityEntry]) -> &[ActivityEntry] {
    activity.get(seen..).unwrap_or(&[])
}

/// Whether the agent ran something because the user (or policy) allowed it: those get the
/// "Started by agent via MCP" toast. Refusals and timeouts do not.
pub fn started_by_agent(d: ActivityDecision) -> bool {
    matches!(
        d,
        ActivityDecision::Approved | ActivityDecision::ApprovedForSession
    )
}

/// What the activity log says about a request, in the current language.
pub fn decision_label(d: ActivityDecision) -> String {
    match d {
        ActivityDecision::Approved => tr_ctx!("agent decision", "approved"),
        ActivityDecision::ApprovedForSession => tr_ctx!("agent decision", "approved for session"),
        ActivityDecision::Declined => tr_ctx!("agent decision", "declined"),
        ActivityDecision::TimedOut => tr_ctx!("agent decision", "timed out"),
        ActivityDecision::ClientReadOnly => tr_ctx!("agent decision", "refused (read-only)"),
        ActivityDecision::ClientBlocked => tr_ctx!("agent decision", "refused (blocked)"),
    }
}

/// What the activity log says about one entry: [`decision_label`], except that a call the user's
/// own setting let through (`start_recording` set to Allow, AGT-2.AC13) says so instead of
/// "approved", because nobody was asked.
pub fn entry_label(e: &ActivityEntry) -> String {
    match (e.decision, e.approval) {
        (ActivityDecision::Approved, Some(cm_daemon::Approval::Policy)) => {
            tr_ctx!("agent decision", "allowed by your setting")
        }
        (d, _) => decision_label(d),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::OffsetDateTime;

    fn entry(tool: &str, client: &str, decision: ActivityDecision) -> ActivityEntry {
        ActivityEntry {
            at: OffsetDateTime::UNIX_EPOCH,
            client: client.to_owned(),
            tool: tool.to_owned(),
            summary: String::new(),
            decision,
            approval: None,
        }
    }

    #[test]
    fn ready_without_calls() {
        let s = derive(true, true, None, &[]);
        assert!(s.is_ready());
        assert_eq!(s.title(), "MCP ready");
        assert_eq!(s.lines(), ["No agent requests yet"]);
    }

    #[test]
    fn last_call_is_the_newest_entry() {
        let log = [
            entry("delete_capture", "cursor", ActivityDecision::Declined),
            entry(
                "start_recording",
                "claude-desktop",
                ActivityDecision::Approved,
            ),
        ];
        let s = derive(true, true, None, &log);
        let c = s.last_call.clone().unwrap();
        assert_eq!(
            (c.tool.as_str(), c.client.as_str()),
            ("start_recording", "claude-desktop")
        );
        assert_eq!(
            s.lines(),
            ["Last call: start_recording", "from claude-desktop"]
        );
    }

    #[test]
    fn offline_reasons() {
        assert!(!derive(false, true, None, &[]).is_ready());
        assert!(!derive(true, false, None, &[]).is_ready());
        let s = derive(true, true, Some(IpcFailure::AlreadyServed), &[]);
        assert_eq!(s.state, McpState::Offline(OfflineReason::SocketOwned));
        assert_eq!(s.title(), "MCP offline");
        assert_eq!(s.lines(), ["Another app owns the socket"]);
        // QA A1: a folder that cannot be made is not "another app".
        let s = derive(true, true, Some(IpcFailure::Folder), &[]);
        assert_eq!(s.lines(), ["Could not create the socket folder"]);
        let s = derive(true, true, Some(IpcFailure::Other), &[]);
        assert_eq!(s.lines(), ["Could not open the IPC endpoint"]);
    }

    #[test]
    fn ipc_errors_are_classified() {
        use std::io::{Error, ErrorKind};
        assert_eq!(
            IpcFailure::of(&Error::new(ErrorKind::AddrInUse, "x")),
            IpcFailure::AlreadyServed
        );
        assert_eq!(
            IpcFailure::of(&Error::new(ErrorKind::PermissionDenied, "x")),
            IpcFailure::Folder
        );
        assert_eq!(
            IpcFailure::of(&Error::new(ErrorKind::InvalidInput, "x")),
            IpcFailure::Other
        );
    }

    /// Board S14 (D7): the shape follows the real state, and the offline reasons stay apart.
    #[test]
    fn s14_status_shape_and_names_follow_the_cause() {
        let ready = derive(true, true, None, &[]);
        assert_eq!(ready.mark(), StatusMark::Dot);
        assert_eq!(ready.aria_status(), "MCP ready");
        assert_eq!(ready.tip().line, "MCP ready \u{b7} stdio and socket");

        let socket = derive(true, true, Some("in use"), &[]);
        assert_eq!(socket.mark(), StatusMark::Bang);
        assert_eq!(socket.aria_status(), "another app owns the socket");
        assert_eq!(socket.tip().line, "Another app owns the socket");
        assert!(socket.tip().detail[0].starts_with("Agents can\u{2019}t reach CaptureMann."));

        // A missing library or endpoint is plain offline and never blames another app.
        for s in [
            derive(false, true, None, &[]),
            derive(true, false, None, &[]),
        ] {
            assert_eq!(s.mark(), StatusMark::Ring);
            assert_eq!(s.aria_status(), "MCP offline");
            let tip = s.tip();
            assert!(tip.line.starts_with("MCP offline"), "{}", tip.line);
            assert!(!tip.line.contains("Another app") && !tip.detail[0].contains("another app"));
        }
        assert_eq!(
            derive(false, true, None, &[]).tip().line,
            "MCP offline \u{b7} Library unavailable"
        );
        assert_eq!(
            derive(true, false, None, &[]).tip().line,
            "MCP offline \u{b7} No IPC endpoint"
        );
    }

    #[test]
    fn s14_ready_tooltip_names_the_last_call() {
        let log = [entry(
            "capture_screenshot",
            "claude",
            ActivityDecision::Approved,
        )];
        let s = derive(true, true, None, &log);
        assert_eq!(s.tip().detail, ["Last call: capture_screenshot"]);
        assert!(derive(true, true, None, &[]).tip().detail.is_empty());
    }

    #[test]
    fn new_entries_since_last_poll() {
        let log = [
            entry("a", "c", ActivityDecision::Approved),
            entry("b", "c", ActivityDecision::Declined),
            entry("d", "c", ActivityDecision::ApprovedForSession),
        ];
        assert_eq!(new_entries(0, &log).len(), 3);
        assert_eq!(new_entries(2, &log)[0].tool, "d");
        assert!(new_entries(3, &log).is_empty());
        // The log shrank: nothing is replayed.
        assert!(new_entries(9, &log).is_empty());
        assert!(started_by_agent(ActivityDecision::Approved));
        assert!(started_by_agent(ActivityDecision::ApprovedForSession));
        assert!(!started_by_agent(ActivityDecision::Declined));
        assert!(!started_by_agent(ActivityDecision::ClientBlocked));
    }

    /// AGT-2.AC13: a call the user's own setting let through is not "approved": nobody was asked.
    #[test]
    fn agt_2_ac13_a_policy_approval_says_so() {
        let mut e = entry("start_recording", "claude", ActivityDecision::Approved);
        assert_eq!(entry_label(&e), "approved");
        e.approval = Some(cm_daemon::Approval::User);
        assert_eq!(entry_label(&e), "approved");
        e.approval = Some(cm_daemon::Approval::Policy);
        assert_eq!(entry_label(&e), "allowed by your setting");
        // It still counts as something the agent was let do, for the toast.
        assert!(started_by_agent(e.decision));
        cm_i18n::with_language(cm_i18n::Lang::Vi, || {
            assert_ne!(entry_label(&e), "allowed by your setting");
        });
    }

    #[test]
    fn decisions_have_labels() {
        assert_eq!(decision_label(ActivityDecision::TimedOut), "timed out");
        assert_eq!(
            decision_label(ActivityDecision::ClientReadOnly),
            "refused (read-only)"
        );
    }
}
