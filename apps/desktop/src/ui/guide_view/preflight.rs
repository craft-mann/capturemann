//! What a Click Guide can do on this computer, read before anything is shown or recorded
//! (GDE-1.AC5, AC6; design S3). A pure model over the cursor crate's status: macOS without Input
//! Monitoring gets the permission card, a session with no click events at all (Wayland) gets the
//! entry points disabled with the reason, everything else starts.

use cm_core::Permission;
use cm_cursor::{Availability, CursorStatus};
use cm_i18n::tr;

/// The verdict of a status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    /// Clicks can be listened to.
    Ready,
    /// macOS: Input Monitoring is not granted. The card offers Grant access and Cancel.
    NeedsInputMonitoring,
    /// The session delivers no click events (Wayland). The text is the reason; the entry points
    /// show it as their tooltip and the start is refused.
    Unavailable(String),
}

/// Reads a cursor status.
pub fn gate(status: &CursorStatus) -> Gate {
    if status.clicks == Availability::Available {
        return Gate::Ready;
    }
    if status.missing_permission() == Some(Permission::InputMonitoring) {
        return Gate::NeedsInputMonitoring;
    }
    Gate::Unavailable(reason_text(status))
}

/// The reason a session has no click events, in words for a tooltip. The system's own wording is
/// used when it gave one, else the general sentence.
fn reason_text(status: &CursorStatus) -> String {
    match status.warnings.first() {
        Some(w) if !w.message.trim().is_empty() => w.message.clone(),
        _ => {
            tr!("This session gives no click events, so a Click Guide cannot see where you click.")
        }
    }
}

/// The live verdict of this computer (cheap, never prompts).
pub fn current() -> Gate {
    gate(&cm_cursor::preflight())
}

/// Why the "New Click Guide" entry points are off, or `None` when a guide can be started (a
/// missing macOS permission is not a reason to disable: the card handles it).
pub fn disabled_reason() -> Option<String> {
    match current() {
        Gate::Unavailable(r) => Some(r),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cm_core::ErrorCode;
    use cm_cursor::{CursorWarning, MoveSource};

    fn degraded(permission: Option<Permission>, message: &str) -> CursorStatus {
        let mut s = CursorStatus::full(MoveSource::Fake);
        s.clicks = Availability::Unavailable;
        s.warnings.push(CursorWarning {
            code: if permission.is_some() {
                ErrorCode::PermissionMissing
            } else {
                ErrorCode::NotImplemented
            },
            permission,
            message: message.to_owned(),
        });
        s
    }

    /// GDE-1.AC5: a missing Input Monitoring is the card, not a refusal.
    #[test]
    fn a_missing_input_monitoring_asks_for_the_permission() {
        let s = degraded(Some(Permission::InputMonitoring), "no tap");
        assert_eq!(gate(&s), Gate::NeedsInputMonitoring);
    }

    /// GDE-1.AC6: no click events and no permission to ask for is a disabled entry with the
    /// system's reason.
    #[test]
    fn a_session_without_click_events_is_unavailable_with_its_reason() {
        let s = degraded(None, "Wayland gives no global click events");
        assert_eq!(
            gate(&s),
            Gate::Unavailable("Wayland gives no global click events".to_owned())
        );
        // No message: the general sentence.
        let s = degraded(None, "  ");
        assert!(matches!(gate(&s), Gate::Unavailable(r) if r.contains("click events")));
    }

    #[test]
    fn full_status_is_ready() {
        assert_eq!(gate(&CursorStatus::full(MoveSource::Fake)), Gate::Ready);
    }
}
