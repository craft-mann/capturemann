//! What the user sees after actions: toasts for CAP-4 and for failures.

use cm_i18n::{tr, tr_ctx};

use crate::backend::{AfterCapture, CopyResult};
use crate::overlay::OverlayError;
use crate::save_folder::FolderResult;
use crate::shortcuts::{Platform, ShortcutAction, ShortcutMap};
use crate::toast::{ToastAction, ToastKind, ToastSpec};

/// [`capture_done_toast`] as a live toast: it is worded again when the language changes.
pub fn capture_done_toast(
    done: &AfterCapture,
    platform: Platform,
    shortcuts: &ShortcutMap,
) -> ToastSpec {
    let (done, shortcuts) = (done.clone(), shortcuts.clone());
    ToastSpec::live(move || capture_done_toast_now(&done, platform, &shortcuts))
}

/// CAP-4.AC1: after a capture, a toast offers Annotate, Copy, Save as and Delete, with keycap
/// hints for pasting and for opening the library. Copy is always there: it copies the rendered
/// image again, whether or not the automatic copy worked.
fn capture_done_toast_now(
    done: &AfterCapture,
    platform: Platform,
    shortcuts: &ShortcutMap,
) -> ToastSpec {
    let paste: Vec<String> = match platform {
        Platform::Mac => vec!["\u{2318}".into(), "V".into()],
        Platform::Windows | Platform::Linux => vec!["Ctrl".into(), "V".into()],
    };
    let library_keys = shortcuts.get(ShortcutAction::OpenLibrary).keycaps(platform);
    let id = done.capture_id.clone();
    let spec = match &done.copy {
        CopyResult::Copied => ToastSpec::new(ToastKind::Success, tr!("Copied to clipboard"))
            .body(tr!("Saved to your library"))
            .hint(tr_ctx!("keyboard shortcut hint", "Paste"), paste),
        CopyResult::Skipped => ToastSpec::new(ToastKind::Success, tr!("Saved to your library")),
        // CAP-4 edge case wording.
        CopyResult::Failed(_) => ToastSpec::new(ToastKind::Warning, tr!("Copied to library only"))
            .body(tr!("The clipboard wasn\u{2019}t available")),
    };
    spec.button(
        tr_ctx!("button", "Annotate"),
        ToastAction::OpenCapture(id.clone()),
    )
    .button(
        tr_ctx!("button", "Copy"),
        ToastAction::CopyCapture(id.clone()),
    )
    .button(
        tr!("Save as\u{2026}"),
        ToastAction::SaveCaptureAs(id.clone()),
    )
    .button(tr_ctx!("button", "Delete"), ToastAction::DeleteCapture(id))
    .hint(tr!("Open library"), library_keys)
}

/// [`folder_warning_toast`] as a live toast: it is worded again when the language changes.
pub fn folder_warning_toast(result: &FolderResult) -> Option<ToastSpec> {
    folder_warning_toast_now(result)?;
    let result = result.clone();
    Some(ToastSpec::live(move || {
        folder_warning_toast_now(&result)
            .unwrap_or_else(|| ToastSpec::new(ToastKind::Info, tr!("Saved to your library")))
    }))
}

/// CAP-4.AC4: the warning when the "also save to a folder" copy failed. The capture itself is
/// in the library. `None` when nothing went wrong (the save worked or was off).
fn folder_warning_toast_now(result: &FolderResult) -> Option<ToastSpec> {
    match result {
        FolderResult::Failed { folder, reason } => Some(
            ToastSpec::new(
                ToastKind::Warning,
                tr!(
                    "Couldn\u{2019}t save to {folder}: {reason}",
                    folder = folder,
                    reason = reason
                ),
            )
            .body(tr!("The capture is saved in your library.")),
        ),
        FolderResult::Skipped | FolderResult::Saved(_) => None,
    }
}

/// [`overlay_error_toast`] as a live toast: it is worded again when the language changes.
pub fn overlay_error_toast(err: &OverlayError) -> ToastSpec {
    let err = err.clone();
    ToastSpec::live(move || overlay_error_toast_now(&err))
}

fn overlay_error_toast_now(err: &OverlayError) -> ToastSpec {
    let mut spec = ToastSpec::new(ToastKind::Error, tr!("Capture didn\u{2019}t start"))
        .body(err.user_message());
    if matches!(err, OverlayError::PermissionMissing(_)) {
        // CAP-1: a dialog with "Open System Settings"; PRM-1: and the page that lists every
        // permission with its state.
        spec = spec
            .button(
                tr!("Open Permissions\u{2026}"),
                ToastAction::OpenPermissions,
            )
            .button(
                tr!("Open System Settings"),
                ToastAction::OpenScreenRecordingSettings,
            );
    }
    spec
}

/// An error toast whose body is the text of an error as it came: the daemon's English message
/// when it has a stable id (`cm_core::message`) is worded in the current language, anything else
/// (an OS error, provider text, an app sentence made earlier) is shown as it is. The title is
/// fixed; use [`error_toast_live`] to have it follow a language switch too.
pub fn error_toast(title: &str, body: impl Into<String>) -> ToastSpec {
    ToastSpec::new(ToastKind::Error, title).body(crate::daemon_text::any_error_text(&body.into()))
}

/// [`error_toast`] as a live toast: `title` and the stable-id part of the body are worded again
/// when the language changes while the toast is on screen.
pub fn error_toast_live(title: fn() -> String, body: impl Into<String>) -> ToastSpec {
    let body: String = body.into();
    ToastSpec::live(move || {
        ToastSpec::new(ToastKind::Error, title()).body(crate::daemon_text::any_error_text(&body))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done(copy: CopyResult) -> AfterCapture {
        AfterCapture {
            capture_id: "shot_1".into(),
            title: "Screenshot".into(),
            copy,
            folder: FolderResult::Skipped,
        }
    }

    /// CAP-4.AC1: toast after capture offers Annotate, shows keycap hints.
    #[test]
    fn success_toast_has_annotate_and_keycap_hints() {
        let map = ShortcutMap::defaults(Platform::Mac);
        let t = capture_done_toast(&done(CopyResult::Copied), Platform::Mac, &map);
        assert_eq!(t.title, "Copied to clipboard");
        assert_eq!(t.buttons[0].label, "Annotate");
        assert_eq!(
            t.buttons[0].action,
            ToastAction::OpenCapture("shot_1".into())
        );
        assert_eq!(t.hints[0].keys, ["\u{2318}", "V"]);
        assert_eq!(t.hints[1].text, "Open library");
        assert_eq!(t.hints[1].keys, ["\u{2325}", "\u{21E7}", "L"]);
        let labels: Vec<_> = t.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Annotate", "Copy", "Save as\u{2026}", "Delete"]);
        assert_eq!(
            t.buttons[3].action,
            ToastAction::DeleteCapture("shot_1".into())
        );
        assert_eq!(
            t.buttons[2].action,
            ToastAction::SaveCaptureAs("shot_1".into())
        );
    }

    /// CAP-4 edge case: clipboard locked -> "Copied to library only" with a retry button.
    #[test]
    fn clipboard_failure_is_a_warning_with_copy_retry() {
        let map = ShortcutMap::defaults(Platform::Windows);
        let t = capture_done_toast(
            &done(CopyResult::Failed("locked".into())),
            Platform::Windows,
            &map,
        );
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(t.title, "Copied to library only");
        assert!(t.buttons.iter().any(|b| b.label == "Copy"));
        assert_eq!(t.hints[0].keys, ["Ctrl", "Alt", "Shift", "L"]);
    }

    /// CAP-4.AC4: an offline folder is a warning that names the folder and the reason.
    #[test]
    fn cap4_ac4_folder_failure_is_a_warning() {
        let failed = FolderResult::Failed {
            folder: "/Volumes/USB/Shots".into(),
            reason: "drive not available".into(),
        };
        let t = folder_warning_toast(&failed).unwrap();
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(
            t.title,
            "Couldn\u{2019}t save to /Volumes/USB/Shots: drive not available"
        );
        assert!(folder_warning_toast(&FolderResult::Skipped).is_none());
        assert!(folder_warning_toast(&FolderResult::Saved("/x/a.png".into())).is_none());
    }

    #[test]
    fn overlay_failures_are_sticky_errors_with_a_clear_message() {
        let t = overlay_error_toast(&OverlayError::NotFound);
        assert_eq!(t.kind, ToastKind::Error);
        assert!(t.body.unwrap().contains("cm-overlay"));
        let p = overlay_error_toast(&OverlayError::PermissionMissing("x".into()));
        assert_eq!(p.buttons[0].action, ToastAction::OpenPermissions);
        assert_eq!(
            p.buttons[1].action,
            ToastAction::OpenScreenRecordingSettings
        );
        assert_eq!(p.title, "Capture didn\u{2019}t start");
    }
}
