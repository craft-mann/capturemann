//! What the user is told about recordings (REC-1, REC-5, REC-8, AGT-2): toasts for a finished,
//! failed, recovered or agent-started recording.

use cm_core::{ErrorCode, Permission};
use cm_daemon::RecoveredRecording;
use cm_i18n::{tr, tr_ctx};

use crate::library::LocalClock;
use crate::recording::model::{clock_text, Ended};
use crate::toast::{ToastAction, ToastKind, ToastSpec};

/// [`recovered_toast`] as a live toast: it is worded again when the language changes.
pub fn recovered_toast(r: &RecoveredRecording, clock: &LocalClock) -> ToastSpec {
    let (r, clock) = (r.clone(), *clock);
    ToastSpec::live(move || recovered_toast_now(&r, &clock))
}

/// REC-8: "Recovered recording from <time> (lost up to 2 s)". The loss bound comes from the
/// recovery (one 2 s fragment per track), rounded up to whole seconds.
fn recovered_toast_now(r: &RecoveredRecording, clock: &LocalClock) -> ToastSpec {
    let when =
        time::OffsetDateTime::parse(&r.created, &time::format_description::well_known::Rfc3339)
            .map(|t| clock.format_date(t))
            .unwrap_or_else(|_| tr!("an earlier session"));
    let secs = (r.lost_ms.max(0) + 999) / 1000;
    let mut spec = ToastSpec::new(
        ToastKind::Warning,
        tr!(
            "Recovered recording from {when} (lost up to {secs} s)",
            when = when,
            secs = secs
        ),
    )
    .button(
        tr_ctx!("button", "Open"),
        ToastAction::OpenCapture(r.capture_id.clone()),
    );
    if !r.missing_tracks.is_empty() {
        spec = spec.body(tr!(
            "Couldn\u{2019}t recover: {tracks}.",
            tracks = r.missing_tracks.join(", ")
        ));
    }
    spec
}

/// The title of the "Recording saved" toast. The export finds the toast of a take by this title
/// (`ui::export_ui::saved_prefix`), so both sides call this one function.
pub fn saved_title() -> String {
    tr!("Recording saved")
}

/// [`ended_toast`] as a live toast: it is worded again when the language changes.
pub fn ended_toast(e: &Ended) -> Option<ToastSpec> {
    ended_toast_now(e)?;
    let e = e.clone();
    Some(ToastSpec::live(move || {
        ended_toast_now(&e).unwrap_or_else(|| ToastSpec::new(ToastKind::Info, saved_title()))
    }))
}

/// The toast after a take ended, or `None` when nothing needs saying (a discard the user just
/// confirmed, a cancelled countdown).
fn ended_toast_now(e: &Ended) -> Option<ToastSpec> {
    match e {
        Ended::Stopped {
            capture_id,
            duration_s,
            reason,
            error,
            permission,
        } => {
            if reason == "permission_revoked" {
                return Some(permission_revoked_toast(*permission, capture_id.as_deref()));
            }
            if let Some(err) = error {
                return Some(
                    ToastSpec::new(
                        ToastKind::Error,
                        tr!("The recording couldn\u{2019}t be saved"),
                    )
                    .body(tr!(
                        "{err} It stays on disk and is recovered the next time CaptureMann starts.",
                        err = err
                    )),
                );
            }
            let time = duration_s.map(|s| clock_text((s * 1000.0) as i64));
            let (title, kind) = match reason.as_str() {
                "max_duration" => (
                    tr!("Recording stopped at its length limit"),
                    ToastKind::Warning,
                ),
                "disk_full" => (
                    tr!("Recording stopped: the disk is almost full"),
                    ToastKind::Warning,
                ),
                "screen_halted" => (
                    screen_halted_title(crate::permissions::Os::current()),
                    ToastKind::Warning,
                ),
                "screen_encoder_failed" | "engine_failed" => {
                    (tr!("Recording stopped unexpectedly"), ToastKind::Warning)
                }
                _ => (saved_title(), ToastKind::Success),
            };
            let mut spec = ToastSpec::new(kind, title);
            let body = match (&time, kind) {
                (Some(t), ToastKind::Success) => tr!("{t} saved to your library", t = t),
                (Some(t), _) => tr!("{t} was saved to your library", t = t),
                (None, _) => tr!("Saved to your library"),
            };
            spec = spec.body(body);
            if let Some(id) = capture_id {
                spec = spec.button(
                    tr_ctx!("button", "Open"),
                    ToastAction::OpenCapture(id.clone()),
                );
            }
            Some(spec)
        }
        Ended::Discarded { .. } | Ended::Cancelled => None,
        Ended::StartFailed { code, message } => {
            let code = code.as_deref().and_then(|c| {
                ErrorCode::ALL
                    .iter()
                    .copied()
                    .find(|known| known.as_str() == c)
            });
            let fallback = tr!("The recording could not start.");
            let message = if message.is_empty() {
                fallback.as_str()
            } else {
                message
            };
            Some(start_error_toast(code, None, message))
        }
        Ended::Lost => Some(
            ToastSpec::new(ToastKind::Info, tr!("The recording ended")).body(tr!(
                "CaptureMann lost track of it. If it was saved, it is in your library."
            )),
        ),
        Ended::SelfTest { state, message } => match state.as_str() {
            "passed" => Some(ToastSpec::new(
                ToastKind::Success,
                tr!("60 fps test passed"),
            )),
            "failed" => Some(
                ToastSpec::new(ToastKind::Warning, tr!("60 fps test did not pass"))
                    .body(tr!("Recordings stay at 30 fps on this Mac.")),
            ),
            "error" => Some(
                ToastSpec::new(
                    ToastKind::Error,
                    tr!("The 60 fps test couldn\u{2019}t finish"),
                )
                .body(message.clone().unwrap_or_default()),
            ),
            _ => None,
        },
    }
}

/// The title when the screen under the recording went away. On Linux the thing that ends is the
/// desktop's screen share (the user stopped sharing, or the stream broke); the data so far is
/// kept either way.
pub fn screen_halted_title(os: crate::permissions::Os) -> String {
    match os {
        crate::permissions::Os::Linux => {
            tr!("Recording stopped: the screen share ended")
        }
        _ => tr!("Recording stopped: the screen or window went away"),
    }
}

/// PRM-1: the OS removed a permission mid-recording and the take was stopped (only Screen
/// Recording does that). The take is in the library; the button leads to Permissions.
pub fn permission_revoked_toast(
    permission: Option<Permission>,
    capture_id: Option<&str>,
) -> ToastSpec {
    let what = match permission.unwrap_or(Permission::Screen) {
        Permission::Screen => tr_ctx!("permission", "Screen Recording"),
        Permission::Camera => tr_ctx!("permission", "Camera"),
        Permission::Microphone => tr_ctx!("permission", "Microphone"),
        Permission::Accessibility => tr_ctx!("permission", "Accessibility"),
        Permission::InputMonitoring => tr_ctx!("permission", "Input Monitoring"),
    };
    let mut spec = ToastSpec::new(
        ToastKind::Error,
        tr!(
            "Recording stopped: {what} permission was removed",
            what = what
        ),
    )
    .body(tr!(
        "What was recorded up to that moment is saved in your library."
    ))
    .button(
        tr!("Open Permissions\u{2026}"),
        ToastAction::OpenPermissions,
    );
    if let Some(id) = capture_id {
        spec = spec.button(
            tr!("Open recording"),
            ToastAction::OpenCapture(id.to_owned()),
        );
    }
    spec
}

/// The System Settings pane (`Privacy_*`) for a permission a `permission_missing` error names.
pub fn privacy_pane(p: Permission) -> &'static str {
    match p {
        Permission::Screen => "Privacy_ScreenCapture",
        Permission::Camera => "Privacy_Camera",
        Permission::Microphone => "Privacy_Microphone",
        Permission::Accessibility => "Privacy_Accessibility",
        Permission::InputMonitoring => "Privacy_ListenEvent",
    }
}

/// [`start_error_toast`] as a live toast: it is worded again when the language changes.
pub fn start_error_toast(
    code: Option<ErrorCode>,
    permission: Option<Permission>,
    message: &str,
) -> ToastSpec {
    let message = message.to_owned();
    ToastSpec::live(move || start_error_toast_now(code, permission, &message))
}

/// The Linux recorder's refusals worded in the UI language: what the daemon answers for a
/// window, a region or a camera (`not_implemented`). Text it does not know stays as it came.
pub fn not_implemented_text(message: &str) -> String {
    if message.contains("window or a region") {
        tr!("Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.")
    } else if message.contains("Camera recording") {
        tr!("Recording with a webcam bubble is not available on Linux yet.")
    } else {
        message.to_owned()
    }
}

/// A start that failed. A missing permission offers Permissions and the System Settings pane of
/// the permission the error names (screen when it names none); everything else shows the
/// engine's message.
fn start_error_toast_now(
    code: Option<ErrorCode>,
    permission: Option<Permission>,
    message: &str,
) -> ToastSpec {
    start_error_spec(crate::permissions::Os::current(), code, permission, message)
}

/// [`start_error_toast_now`] for a given system. On Linux a missing screen permission means the
/// desktop's share dialog was cancelled (there is no System Settings pane to open), and the
/// recorder's `not_implemented` answers are worded here.
pub fn start_error_spec(
    os: crate::permissions::Os,
    code: Option<ErrorCode>,
    permission: Option<Permission>,
    message: &str,
) -> ToastSpec {
    use crate::permissions::Os;
    let linux = os == Os::Linux;
    let screen_dialog = linux
        && code == Some(ErrorCode::PermissionMissing)
        && permission.unwrap_or(Permission::Screen) == Permission::Screen;
    let body = if screen_dialog {
        tr!("The screen share was cancelled in your desktop\u{2019}s dialog, so nothing was recorded. Start again and choose the screen to share.")
    } else if linux && code == Some(ErrorCode::NotImplemented) {
        not_implemented_text(message)
    } else {
        crate::daemon_text::error_text(code, message)
    };
    let mut spec =
        ToastSpec::new(ToastKind::Error, tr!("Recording didn\u{2019}t start")).body(body);
    if code == Some(ErrorCode::PermissionMissing) && !linux {
        let pane = match permission.unwrap_or(Permission::Screen) {
            Permission::Screen => ToastAction::OpenScreenRecordingSettings,
            other => ToastAction::OpenPrivacyPane(privacy_pane(other)),
        };
        spec = spec
            .button(
                tr!("Open Permissions\u{2026}"),
                ToastAction::OpenPermissions,
            )
            .button(tr!("Open System Settings"), pane);
    }
    if linux && code == Some(ErrorCode::NotImplemented) {
        spec.kind = ToastKind::Info;
    }
    if code == Some(ErrorCode::Busy) {
        spec = ToastSpec::new(ToastKind::Info, tr!("A recording is already running"));
    }
    spec
}

/// REC-5: the agent toast for a recording an agent started.
pub fn agent_started_toast(client: &str, tool: &str, recording_id: Option<&str>) -> ToastSpec {
    match recording_id {
        Some(id) => ToastSpec::agent_with_stop(tool, client, id),
        None => ToastSpec::agent(tool, client),
    }
}

/// Whether the Stop button of an agent toast still has something to stop: recording `id` is the
/// one in progress (running, paused or resuming), not a later take and not one already ending.
pub fn stop_applies(
    phase: crate::recording::model::Phase,
    current: Option<&str>,
    id: &str,
) -> bool {
    use crate::recording::model::Phase;
    current == Some(id) && matches!(phase, Phase::Recording | Phase::Paused | Phase::Resuming)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock() -> LocalClock {
        LocalClock {
            offset: time::UtcOffset::UTC,
        }
    }

    /// REC-8: the toast names when the recording was made and the loss bound.
    #[test]
    fn rec8_recovered_toast() {
        let r = RecoveredRecording {
            capture_id: "rec_1".into(),
            title: "Demo".into(),
            created: "2020-03-04T10:30:00Z".into(),
            duration_s: 61.0,
            lost_ms: 2000,
            missing_tracks: vec![],
        };
        let t = recovered_toast(&r, &clock());
        assert_eq!(
            t.title,
            "Recovered recording from Mar 4, 2020 (lost up to 2 s)"
        );
        assert_eq!(t.kind, ToastKind::Warning);
        assert_eq!(
            t.buttons[0].action,
            ToastAction::OpenCapture("rec_1".into())
        );
        assert!(t.body.is_none());
        let mut with_missing = r;
        with_missing.missing_tracks = vec!["camera".into()];
        assert!(recovered_toast(&with_missing, &clock())
            .body
            .unwrap()
            .contains("camera"));
    }

    /// The export turns the "Recording saved" toast into its progress toast by finding it with
    /// this title: both sides use `saved_title`, in every language.
    #[test]
    fn the_saved_toast_and_the_export_look_up_use_one_title() {
        use cm_i18n::{with_language, Lang};
        for lang in [Lang::En, Lang::Vi, Lang::EnXa] {
            with_language(lang, || {
                let t = ended_toast(&Ended::Stopped {
                    capture_id: Some("rec_9".into()),
                    duration_s: None,
                    reason: "user".into(),
                    error: None,
                    permission: None,
                })
                .unwrap();
                assert_eq!(t.title, saved_title());
                assert_eq!(crate::ui::export_ui::saved_prefix(), saved_title());
            });
        }
    }

    #[test]
    fn a_finished_take_opens_from_the_toast() {
        let t = ended_toast(&Ended::Stopped {
            capture_id: Some("rec_9".into()),
            duration_s: Some(42.6),
            reason: "user".into(),
            error: None,
            permission: None,
        })
        .unwrap();
        assert_eq!(t.title, "Recording saved");
        assert_eq!(t.body.as_deref(), Some("00:42 saved to your library"));
        assert_eq!(
            t.buttons[0].action,
            ToastAction::OpenCapture("rec_9".into())
        );
    }

    #[test]
    fn automatic_stops_say_why() {
        for (reason, expect) in [
            ("max_duration", "length limit"),
            ("disk_full", "disk is almost full"),
            ("screen_halted", "went away"),
        ] {
            let t = ended_toast(&Ended::Stopped {
                capture_id: Some("rec_9".into()),
                duration_s: Some(10.0),
                reason: reason.into(),
                error: None,
                permission: None,
            })
            .unwrap();
            assert_eq!(t.kind, ToastKind::Warning, "{reason}");
            assert!(t.title.contains(expect), "{reason}: {}", t.title);
        }
    }

    /// PRM-1: a stop because the OS removed Screen Recording says so and leads to Permissions.
    #[test]
    fn prm1_revoked_permission_toast_offers_permissions() {
        let t = ended_toast(&Ended::Stopped {
            capture_id: Some("rec_9".into()),
            duration_s: Some(12.0),
            reason: "permission_revoked".into(),
            error: None,
            permission: Some(Permission::Screen),
        })
        .unwrap();
        assert_eq!(
            t.title,
            "Recording stopped: Screen Recording permission was removed"
        );
        assert_eq!(t.kind, ToastKind::Error);
        assert_eq!(t.buttons[0].label, "Open Permissions\u{2026}");
        assert_eq!(t.buttons[0].action, ToastAction::OpenPermissions);
        assert_eq!(
            t.buttons[1].action,
            ToastAction::OpenCapture("rec_9".into())
        );
        // An older engine that sends no `permission` still names the screen.
        let t = permission_revoked_toast(None, None);
        assert!(t.title.contains("Screen Recording"));
        assert_eq!(t.buttons.len(), 1);
    }

    #[test]
    fn a_failed_stop_is_a_sticky_error() {
        let t = ended_toast(&Ended::Stopped {
            capture_id: None,
            duration_s: None,
            reason: "error".into(),
            error: Some("disk full".into()),
            permission: None,
        })
        .unwrap();
        assert_eq!(t.kind, ToastKind::Error);
        assert!(t.body.unwrap().contains("recovered"));
    }

    /// REC-5.AC3: a discard the user confirmed, and a cancelled countdown, need no toast.
    #[test]
    fn discards_are_silent() {
        assert!(ended_toast(&Ended::Discarded {
            reason: "user".into()
        })
        .is_none());
        assert!(ended_toast(&Ended::Cancelled).is_none());
    }

    #[test]
    fn start_errors() {
        let p = start_error_toast(
            Some(ErrorCode::PermissionMissing),
            None,
            "Screen Recording is off",
        );
        assert_eq!(p.buttons[0].action, ToastAction::OpenPermissions);
        assert_eq!(
            p.buttons[1].action,
            ToastAction::OpenScreenRecordingSettings
        );
        let b = start_error_toast(Some(ErrorCode::Busy), None, "x");
        assert_eq!(b.kind, ToastKind::Info);
        let o = start_error_toast(None, None, "boom");
        assert_eq!(o.kind, ToastKind::Error);
        assert!(o.buttons.is_empty());
    }

    /// Linux: a cancelled share dialog is not a System Settings problem; the recorder's
    /// "not implemented" answers read as plain sentences.
    #[test]
    fn linux_start_errors_are_readable() {
        use crate::permissions::Os;
        let cancelled = start_error_spec(
            Os::Linux,
            Some(ErrorCode::PermissionMissing),
            Some(Permission::Screen),
            "Screen sharing was cancelled in the desktop's dialog.",
        );
        assert!(
            cancelled.buttons.is_empty(),
            "no System Settings pane on Linux"
        );
        assert!(cancelled
            .body
            .unwrap()
            .contains("cancelled in your desktop"));
        let window = start_error_spec(
            Os::Linux,
            Some(ErrorCode::NotImplemented),
            None,
            "Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.",
        );
        assert_eq!(window.kind, ToastKind::Info);
        assert!(window
            .body
            .unwrap()
            .starts_with("Recording a window or a region"));
        let cam = start_error_spec(
            Os::Linux,
            Some(ErrorCode::NotImplemented),
            None,
            "Camera recording is not available on Linux yet.",
        );
        assert!(cam.body.unwrap().contains("webcam bubble"));
        // Unknown text stays as the daemon wrote it; the Mac keeps its buttons.
        assert_eq!(not_implemented_text("Something new."), "Something new.");
        let mac = start_error_spec(Os::Mac, Some(ErrorCode::PermissionMissing), None, "m");
        assert_eq!(mac.buttons.len(), 2);
        assert_eq!(
            screen_halted_title(Os::Linux),
            "Recording stopped: the screen share ended"
        );
        assert_eq!(
            screen_halted_title(Os::Mac),
            "Recording stopped: the screen or window went away"
        );
    }

    /// R4-5: the Settings pane follows the permission the error names.
    #[test]
    fn r4_5_start_error_opens_the_pane_of_the_missing_permission() {
        for (perm, pane) in [
            (Permission::Microphone, "Privacy_Microphone"),
            (Permission::Camera, "Privacy_Camera"),
            (Permission::InputMonitoring, "Privacy_ListenEvent"),
        ] {
            let t = start_error_toast(Some(ErrorCode::PermissionMissing), Some(perm), "m");
            assert_eq!(t.buttons[1].action, ToastAction::OpenPrivacyPane(pane));
        }
        let screen = start_error_toast(
            Some(ErrorCode::PermissionMissing),
            Some(Permission::Screen),
            "m",
        );
        assert_eq!(
            screen.buttons[1].action,
            ToastAction::OpenScreenRecordingSettings
        );
    }

    /// R4-1 / R4-3 / R4-9: the end states the engine did not announce are told to the user.
    #[test]
    fn unannounced_endings_have_toasts() {
        let t = ended_toast(&Ended::StartFailed {
            code: Some("permission_missing".into()),
            message: "Camera access is off".into(),
        })
        .unwrap();
        assert_eq!(t.title, "Recording didn\u{2019}t start");
        assert_eq!(t.buttons.len(), 2);
        assert!(ended_toast(&Ended::Lost).is_some());
        let passed = ended_toast(&Ended::SelfTest {
            state: "passed".into(),
            message: None,
        });
        assert_eq!(passed.unwrap().kind, ToastKind::Success);
        assert!(ended_toast(&Ended::SelfTest {
            state: "cancelled".into(),
            message: None
        })
        .is_none());
    }

    /// AGT-2 / DESIGN 4: the agent toast carries the tool and the client.
    #[test]
    fn agent_toast_text() {
        let t = agent_started_toast("claude-desktop", "start_recording", Some("rec_1"));
        assert_eq!(t.title, "Started by agent via MCP");
        assert_eq!(t.buttons.len(), 1);
        assert_eq!(
            t.buttons[0].action,
            ToastAction::StopRecording("rec_1".into())
        );
        assert!(agent_started_toast("c", "t", None).buttons.is_empty());
        let a = t.agent.unwrap();
        assert_eq!(
            (a.tool.as_str(), a.client.as_str()),
            ("start_recording", "claude-desktop")
        );
    }

    /// The agent toast's Stop only acts on the recording it was made for, while it runs.
    #[test]
    fn stop_applies_only_to_the_running_recording() {
        use crate::recording::model::Phase;
        assert!(stop_applies(Phase::Recording, Some("r1"), "r1"));
        assert!(stop_applies(Phase::Paused, Some("r1"), "r1"));
        assert!(!stop_applies(Phase::Stopping, Some("r1"), "r1"));
        assert!(!stop_applies(Phase::Idle, None, "r1"));
        assert!(
            !stop_applies(Phase::Recording, Some("r2"), "r1"),
            "a later take"
        );
        assert!(!stop_applies(Phase::Countdown, Some("r1"), "r1"));
    }
}
