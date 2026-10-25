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
}

impl OfflineReason {
    pub fn text(self) -> String {
        match self {
            OfflineReason::LibraryUnavailable => tr!("Library unavailable"),
            OfflineReason::NoEndpoint => tr!("No IPC endpoint"),
            OfflineReason::SocketOwned => tr!("Another app owns the socket"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LastCall {
    pub tool: String,
    pub client: String,
    pub decision: ActivityDecision,
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

/// `daemon_up`: the library opened so a daemon exists. `ipc_warning`: set when the endpoint
/// could not be bound. `endpoint_configured`: the app was asked to serve one.
pub fn derive(
    daemon_up: bool,
    endpoint_configured: bool,
    ipc_warning: Option<&str>,
    activity: &[ActivityEntry],
) -> McpStatus {
    let state = if !daemon_up {
        McpState::Offline(OfflineReason::LibraryUnavailable)
    } else if !endpoint_configured {
        McpState::Offline(OfflineReason::NoEndpoint)
    } else if ipc_warning.is_some() {
        McpState::Offline(OfflineReason::SocketOwned)
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
        let s = derive(true, true, Some("socket in use"), &[]);
        assert_eq!(s.state, McpState::Offline(OfflineReason::SocketOwned));
        assert_eq!(s.title(), "MCP offline");
        assert_eq!(s.lines(), ["Another app owns the socket"]);
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

    #[test]
    fn decisions_have_labels() {
        assert_eq!(decision_label(ActivityDecision::TimedOut), "timed out");
        assert_eq!(
            decision_label(ActivityDecision::ClientReadOnly),
            "refused (read-only)"
        );
    }
}
