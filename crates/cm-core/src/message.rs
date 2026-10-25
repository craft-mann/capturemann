//! Stable ids and parameters for the English messages a client may translate (UI languages, P3).
//!
//! The daemon, the CLI and MCP keep speaking English: an [`crate::CmError`] still carries its
//! English `message`, and its code and exit code are unchanged. A client that shows errors in
//! another language (the desktop app) asks [`describe`] for the message's stable id and its
//! parameters (a file name, a limit, a count) and builds the sentence itself, falling back to the
//! English message when the id is unknown (a newer daemon, or a message not in the table).
//!
//! [`TEMPLATES`] is the single list. Each entry pins the code and the English wording with
//! `{name}` placeholders; the matcher reads the parameters back out of the message, so the sites
//! that raise the errors stay as they are. Changing the wording at a site without changing the
//! entry makes the message fall back to English (never a wrong sentence); the tests here and in
//! `cm-daemon` pin the wording of the messages that matter.

use crate::error::ErrorCode;

/// One translatable message: a stable id, the code it is raised with and its English text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    pub id: &'static str,
    pub code: ErrorCode,
    /// The English message; `{name}` marks a parameter.
    pub english: &'static str,
}

const fn t(id: &'static str, code: ErrorCode, english: &'static str) -> Template {
    Template { id, code, english }
}

/// Every message with a stable id. Ids are never renamed or reused.
pub const TEMPLATES: &[Template] = &[
    // The confirmation gate.
    t(
        "declined",
        ErrorCode::UserDeclined,
        "The user declined the request",
    ),
    t(
        "confirmation_timeout",
        ErrorCode::ConfirmationTimeout,
        "No answer within {wait}",
    ),
    t(
        "confirmation_queue_full",
        ErrorCode::Busy,
        "Too many confirmation requests are waiting; answer or dismiss them first",
    ),
    // Recording.
    t(
        "recording_running",
        ErrorCode::Busy,
        "A recording is already running",
    ),
    t(
        "selftest_running",
        ErrorCode::Busy,
        "The 60 fps self-test is running; wait for it to finish or stop it",
    ),
    t(
        "disk_low_to_start",
        ErrorCode::DiskFull,
        "Only {mb} MB free; recording needs more than 1 GB free to start",
    ),
    t(
        "no_recording_running",
        ErrorCode::NotFound,
        "No recording is running",
    ),
    t(
        "recording_not_running",
        ErrorCode::NotFound,
        "Recording `{id}` is not running",
    ),
    t(
        "recording_unknown",
        ErrorCode::NotFound,
        "Recording `{id}` is not known",
    ),
    t(
        "recording_ended",
        ErrorCode::NotFound,
        "The recording has already ended",
    ),
    t(
        "recording_ended_before_answer",
        ErrorCode::NotFound,
        "The recording ended before it could answer",
    ),
    t(
        "recording_ended_before_restart",
        ErrorCode::NotFound,
        "The recording ended before it could restart",
    ),
    t(
        "hardware_encoder_unavailable",
        ErrorCode::PlatformRejected,
        "This Mac has no usable hardware H.264 encoder for this recording. Try a lower resolution, update to macOS 13 or later, or use a Mac from about 2016 or later.",
    ),
    // FFmpeg is missing (the exporter, the GIF and WebM presets, frame picking). The ids are the
    // stable machine-readable "FFmpeg not found" code: `cm_media::tools` raises the messages (a
    // test there pins the wording), and a failed job's `error.data.reason_code` carries the id.
    t(
        "ffmpeg_not_found",
        ErrorCode::NotFound,
        "FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy.",
    ),
    t(
        "ffmpeg_bundle_missing",
        ErrorCode::NotFound,
        "FFmpeg was not found: the copy that ships inside CaptureMann is missing, so the app is damaged or incomplete. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)",
    ),
    t(
        "ffmpeg_env_invalid",
        ErrorCode::NotFound,
        "FFmpeg was not found: CM_FFMPEG points to `{path}`, which is not a file. Fix or unset CM_FFMPEG.",
    ),
    t(
        "ffmpeg_bundle_cannot_run",
        ErrorCode::NotFound,
        "FFmpeg was not found: the copy inside CaptureMann (`{path}`) cannot be run ({error}). The app is damaged or macOS blocked it. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)",
    ),
    t(
        "ffmpeg_permission_denied",
        ErrorCode::NotFound,
        "FFmpeg at `{path}` cannot be run (permission denied). FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy.",
    ),
    // Targets.
    t("no_display", ErrorCode::NotFound, "No display found"),
    t(
        "display_not_found",
        ErrorCode::NotFound,
        "Display {id} not found",
    ),
    t(
        "window_not_found",
        ErrorCode::NotFound,
        "Window {id} not found",
    ),
    t(
        "window_minimized",
        ErrorCode::InvalidInput,
        "Window is minimized",
    ),
    // Library and files.
    t(
        "recording_folder_moved",
        ErrorCode::NotFound,
        "The recording folder `{path}` was moved or deleted",
    ),
    t(
        "capture_folder_moved",
        ErrorCode::NotFound,
        "the folder of `{id}` was moved or deleted",
    ),
    t(
        "capture_still_recording",
        ErrorCode::Busy,
        "`{id}` is still being recorded; stop the recording before you {action} it",
    ),
    t(
        "capture_exporting",
        ErrorCode::Busy,
        "`{id}` is being exported; wait for the export to finish or cancel it before you {action} it",
    ),
    t(
        "no_export_yet",
        ErrorCode::NotFound,
        "`{id}` has no export yet; run `export_video` first",
    ),
    t(
        "no_export_to_upload",
        ErrorCode::NotFound,
        "`{id}` has no export to upload",
    ),
    t(
        "no_captions_yet",
        ErrorCode::NotFound,
        "`{id}` has no captions yet; run `transcribe` first",
    ),
    t(
        "no_transcript_yet",
        ErrorCode::NotFound,
        "`{id}` has no transcript yet; run `transcribe` first",
    ),
    t(
        "model_not_installed",
        ErrorCode::NotFound,
        "`{model}` is not installed",
    ),
    // Destinations and accounts.
    t(
        "no_destination",
        ErrorCode::NotFound,
        "no destination is set up",
    ),
    t(
        "no_publish_account",
        ErrorCode::NotFound,
        "no publish account is set up",
    ),
    t(
        "account_not_connected",
        ErrorCode::AuthExpired,
        "This account is not connected. Connect it again in Settings → Destinations.",
    ),
    t(
        "account_credentials_missing",
        ErrorCode::AuthExpired,
        "The app credentials of this account are missing. Connect it again in Settings → Destinations.",
    ),
    t(
        "account_renew_refused",
        ErrorCode::AuthExpired,
        "The provider refused to renew the connection. Connect the account again and check the client ID.",
    ),
    // Permissions.
    t(
        "screen_access_off",
        ErrorCode::PermissionMissing,
        "Screen Recording access is off. Turn it on in System Settings > Privacy & Security > Screen Recording, then restart CaptureMann.",
    ),
    t(
        "screen_permission_not_granted",
        ErrorCode::PermissionMissing,
        "Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen Recording, then restart CaptureMann.",
    ),
    t(
        "screen_permission_not_granted_audio",
        ErrorCode::PermissionMissing,
        "Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen & System Audio Recording, then restart CaptureMann.",
    ),
    t(
        "microphone_access_off",
        ErrorCode::PermissionMissing,
        "Microphone access is off. Turn it on in System Settings > Privacy & Security > Microphone.",
    ),
    t(
        "camera_not_requested",
        ErrorCode::PermissionMissing,
        "Camera access has not been requested yet. Allow camera access when prompted.",
    ),
    t(
        "camera_denied",
        ErrorCode::PermissionMissing,
        "Camera access is denied. Enable it in System Settings > Privacy & Security > Camera.",
    ),
    t(
        "camera_restricted",
        ErrorCode::PermissionMissing,
        "Camera access is restricted by a device policy and cannot be granted here.",
    ),
];

/// A message's stable id and its parameters, in the order the template names them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageParts {
    pub id: &'static str,
    pub params: Vec<(&'static str, String)>,
}

impl MessageParts {
    /// The parameter called `name`, or an empty string.
    pub fn get(&self, name: &str) -> &str {
        self.params
            .iter()
            .find(|(k, _)| *k == name)
            .map_or("", |(_, v)| v.as_str())
    }
}

/// Splits a template into literal pieces and placeholder names: `a{x}b` gives
/// `(["a", "b"], ["x"])`.
fn split(english: &'static str) -> (Vec<&'static str>, Vec<&'static str>) {
    let mut lits = Vec::new();
    let mut names = Vec::new();
    let mut rest = english;
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        lits.push(&rest[..open]);
        names.push(&rest[open + 1..open + close]);
        rest = &rest[open + close + 1..];
    }
    lits.push(rest);
    (lits, names)
}

fn match_template(tpl: &Template, message: &str) -> Option<MessageParts> {
    let (lits, names) = split(tpl.english);
    let mut rest = message.strip_prefix(lits[0])?;
    let mut params = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let lit = lits[i + 1];
        let end = if i + 1 == names.len() {
            // The tail must be exactly the closing literal.
            rest.strip_suffix(lit)?.len()
        } else {
            rest.find(lit)?
        };
        let value = &rest[..end];
        if value.is_empty() {
            return None;
        }
        params.push((*name, value.to_owned()));
        rest = &rest[end + lit.len()..];
    }
    rest.is_empty()
        .then_some(MessageParts { id: tpl.id, params })
}

/// The stable id and parameters of an error message raised with `code`, if it is in
/// [`TEMPLATES`].
pub fn describe(code: ErrorCode, message: &str) -> Option<MessageParts> {
    TEMPLATES
        .iter()
        .filter(|t| t.code == code)
        .find_map(|t| match_template(t, message))
}

/// [`describe`] for a message whose code was lost on the way (a text passed around as a plain
/// string): the first template, whatever its code, that the whole message matches.
pub fn describe_any(message: &str) -> Option<MessageParts> {
    TEMPLATES.iter().find_map(|t| match_template(t, message))
}

/// The stable id when `reason` (a failed job's English reason, which wraps the original message)
/// is a missing-FFmpeg message: one of the `ffmpeg_*` ids above.
pub fn ffmpeg_missing_id(reason: &str) -> Option<&'static str> {
    describe(ErrorCode::NotFound, reason)
        .map(|p| p.id)
        .filter(|id| id.starts_with("ffmpeg_"))
}

/// The English text of template `id` with `params` filled in; `None` for an unknown id.
pub fn render(id: &str, params: &[(&str, &str)]) -> Option<String> {
    let tpl = TEMPLATES.iter().find(|t| t.id == id)?;
    let (lits, names) = split(tpl.english);
    let mut out = String::from(lits[0]);
    for (i, name) in names.iter().enumerate() {
        out.push_str(
            params
                .iter()
                .find(|(k, _)| k == name)
                .map_or("", |(_, v)| v),
        );
        out.push_str(lits[i + 1]);
    }
    Some(out)
}

/// The notes the recording session raises while it runs or while it is planned
/// (`recording.warning` events, the `warnings` of a start result, `session.json`): a stable id
/// and the English text with `{name}` parameters. They carry no error code on the wire, so they
/// are matched by their text alone ([`describe_warning`]); the English message and the `code` of
/// the event are unchanged. A message that matches none is shown as sent.
pub const WARNING_TEMPLATES: &[(&str, &str)] = &[
    ("warn_disk_low", "About {minutes} minutes of disk space left"),
    (
        "warn_disk_low_plan",
        "About {minutes} minutes of disk space left at this setting",
    ),
    ("warn_disk_full", "Recording stopped: only {mb} MB free"),
    (
        "warn_screen_late",
        "{pct}% of screen frames were late over the last 30 s",
    ),
    ("warn_screen_healthy", "Screen capture is keeping up again"),
    (
        "warn_capture_stalled",
        "Screen capture delivered no frames for {secs} s (display asleep or capture stuck); the video repeats the last frame until it resumes",
    ),
    ("warn_screen_ended", "Screen capture ended: {detail}"),
    ("warn_screen_encoder", "Screen encoder failed: {detail}"),
    (
        "warn_mic_fallback",
        "Microphone {requested} is not available; using {using}",
    ),
    (
        "warn_audio_changed",
        "Audio device changed from {from} to {to}",
    ),
    (
        "warn_audio_lost",
        "Audio device {name} was lost; recording silence",
    ),
    (
        "warn_mic_silent",
        "The microphone has delivered only silence for 15 s (muted, or its access was revoked); the recording continues",
    ),
    (
        "warn_audio_write_failed",
        "The audio file could not be written ({detail}); the track ends here and the recording continues",
    ),
    (
        "warn_camera_lost",
        "The camera was disconnected; the camera track has a gap",
    ),
    ("warn_camera_back", "The camera is back"),
    (
        "warn_camera_write_failed",
        "The camera file could not be written: {detail}",
    ),
    (
        "warn_permission_removed_stop",
        "{what} permission was removed; the recording is stopped and what was recorded is kept",
    ),
    (
        "warn_permission_removed_go_on",
        "{what} permission was removed; the recording continues without this track from here on",
    ),
    (
        "warn_engine_failed",
        "The recording engine failed; saving what was recorded",
    ),
    (
        "warn_engine_stopped",
        "The recording engine was stopped; saving what was recorded",
    ),
    (
        "warn_track_start_failed",
        "{track} track could not start: {detail}",
    ),
    (
        "warn_track_pause_failed",
        "{track} track could not pause: {detail}",
    ),
    (
        "warn_track_resume_failed",
        "{track} track could not resume: {detail}",
    ),
    (
        "warn_track_finish_failed",
        "{track} track could not be finished: {detail}; its files were kept in the recording folder",
    ),
    (
        "warn_track_stopped_early",
        "The {track} track stopped early ({detail}); what was written is kept",
    ),
    (
        "warn_file_ended_early",
        "The {track} file ended early ({detail}); what was written is kept",
    ),
    ("warn_fps_fallback", "Recording at 30 fps. {detail}"),
    (
        "warn_fps60_camera",
        "The 60 fps check measured the screen alone. With the camera on, frames may arrive late; watch the health indicator or record at 30 fps",
    ),
    (
        "warn_cursor_clicks_off",
        "Cursor clicks will not be recorded (only the position): Input Monitoring access is off.",
    ),
];

/// The stable id and parameters of a session note's English `message`, if it is in
/// [`WARNING_TEMPLATES`].
pub fn describe_warning(message: &str) -> Option<MessageParts> {
    WARNING_TEMPLATES.iter().find_map(|(id, english)| {
        match_template(
            &Template {
                id,
                code: ErrorCode::Internal,
                english,
            },
            message,
        )
    })
}

/// The English text of warning template `id` with `params` filled in.
pub fn render_warning(id: &str, params: &[(&str, &str)]) -> Option<String> {
    let (_, english) = WARNING_TEMPLATES.iter().find(|(i, _)| *i == id)?;
    let (lits, names) = split(english);
    let mut out = String::from(lits[0]);
    for (i, name) in names.iter().enumerate() {
        out.push_str(
            params
                .iter()
                .find(|(k, _)| k == name)
                .map_or("", |(_, v)| v),
        );
        out.push_str(lits[i + 1]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_are_unique_and_every_template_round_trips() {
        let mut seen = HashSet::new();
        for t in TEMPLATES {
            assert!(seen.insert(t.id), "duplicate id {}", t.id);
            let (_, names) = split(t.english);
            let values: Vec<(&str, String)> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (*n, format!("v{i} x")))
                .collect();
            let refs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let english = render(t.id, &refs).unwrap();
            let parts = describe(t.code, &english)
                .unwrap_or_else(|| panic!("{} does not match itself: {english}", t.id));
            assert_eq!(parts.id, t.id, "{english}");
            assert_eq!(parts.params, values, "{}", t.id);
        }
    }

    #[test]
    fn parameters_are_read_out_of_the_message() {
        let p = describe(
            ErrorCode::DiskFull,
            "Only 312 MB free; recording needs more than 1 GB free to start",
        )
        .unwrap();
        assert_eq!((p.id, p.get("mb")), ("disk_low_to_start", "312"));
        let p = describe(
            ErrorCode::NotFound,
            "The recording folder `/a b/c.cmrec` was moved or deleted",
        )
        .unwrap();
        assert_eq!(
            (p.id, p.get("path")),
            ("recording_folder_moved", "/a b/c.cmrec")
        );
        let p = describe(
            ErrorCode::Busy,
            "`rec_1` is still being recorded; stop the recording before you delete it",
        )
        .unwrap();
        assert_eq!((p.get("id"), p.get("action")), ("rec_1", "delete"));
    }

    #[test]
    fn code_must_match_and_unknown_text_has_no_id() {
        assert!(describe(ErrorCode::Internal, "A recording is already running").is_none());
        assert!(describe(ErrorCode::Busy, "Something else entirely").is_none());
        assert!(describe(ErrorCode::NotFound, "Display  not found").is_none());
        assert!(render("nope", &[]).is_none());
    }

    /// P3 changes nothing on the wire: the payload of an error that has a stable id is still
    /// `{ code, message, data }` with the English message and the same exit code.
    #[test]
    fn the_wire_form_of_an_error_is_unchanged() {
        let e = crate::CmError::disk_full(
            "Only 312 MB free; recording needs more than 1 GB free to start",
        );
        let v = serde_json::to_value(e.to_payload()).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "code": "disk_full",
                "message": "Only 312 MB free; recording needs more than 1 GB free to start",
                "data": null,
            })
        );
        assert_eq!(e.exit_code(), 5);
        let parts = e.message_parts().unwrap();
        assert_eq!((parts.id, parts.get("mb")), ("disk_low_to_start", "312"));
        assert!(crate::CmError::internal("boom").message_parts().is_none());
    }

    #[test]
    fn warning_templates_round_trip_and_ids_are_unique() {
        let mut seen = HashSet::new();
        for (id, english) in WARNING_TEMPLATES {
            assert!(seen.insert(*id), "duplicate id {id}");
            let (_, names) = split(english);
            let values: Vec<(&str, String)> = names
                .iter()
                .enumerate()
                .map(|(i, n)| (*n, format!("v{i} x")))
                .collect();
            let refs: Vec<(&str, &str)> = values.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let text = render_warning(id, &refs).unwrap();
            let parts = describe_warning(&text)
                .unwrap_or_else(|| panic!("{id} does not match itself: {text}"));
            assert_eq!(parts.id, *id, "{text}");
            assert_eq!(parts.params, values, "{id}");
        }
    }

    #[test]
    fn warning_parameters_are_read_out() {
        let p = describe_warning("Audio device changed from USB Mic to Built-in").unwrap();
        assert_eq!(
            (p.id, p.get("from"), p.get("to")),
            ("warn_audio_changed", "USB Mic", "Built-in")
        );
        let p = describe_warning("camera track could not start: no device").unwrap();
        assert_eq!(
            (p.id, p.get("track"), p.get("detail")),
            ("warn_track_start_failed", "camera", "no device")
        );
        assert!(describe_warning("Something else entirely").is_none());
    }
}
