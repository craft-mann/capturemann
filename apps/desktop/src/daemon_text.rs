//! Text the daemon words in English and the app shows in the chosen UI language (i18n phase P3).
//!
//! The CLI, MCP and JSON-RPC keep the English; this module is the only place the app turns a
//! daemon sentence into a translated one. Two kinds:
//!
//! * **Confirmations.** `ConfirmationRequest` carries a stable `kind` (the action id) and typed
//!   `params` next to the English `summary` and `details` rows. [`confirm_summary`] and
//!   [`confirm_rows`] build the sentence and the rows from them with `tr!`, one message per
//!   sentence. A `kind` or a parameter this app version does not know falls back to the English
//!   the daemon sent, so a newer daemon never shows a blank or a key. In English the output is
//!   byte-identical to the daemon's own text (tests pin it).
//! * **Errors.** [`error_text`] maps an error code and its English message to a translated sentence
//!   through the stable ids of `cm_core::message`, falling back to the English message.
//!
//! Anything else the daemon returns in English (job failure reasons, publish rules, warnings) is
//! listed in `design/i18n-STATUS-p3-daemon.md`.

use cm_core::message::{self, MessageParts};
use cm_core::ErrorCode;
use cm_i18n::{fmt, tr, tr_ctx, trn};
use serde_json::{Map, Value};

use crate::ai::client::AiError;
use crate::backend::CallError;

// ---- small readers -------------------------------------------------------------------------

fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn num(v: &Value, k: &str) -> Option<u64> {
    v.get(k).and_then(Value::as_u64)
}

fn float(v: &Value, k: &str) -> Option<f64> {
    v.get(k).and_then(Value::as_f64)
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

// ---- number and duration formats (same shapes as the daemon's English) ----------------------

/// `1.2 MB` like the daemon's `human_size` (MB = 10^6 bytes), with the language's decimal mark.
pub fn size_text(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut v = bytes as f64;
    let mut unit = 0;
    while v >= 1000.0 && unit < UNITS.len() - 1 {
        v /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{} {}", fmt::decimal(v, 1), UNITS[unit])
    }
}

/// `45 s`, `2 min`, `1 min 30 s`, `1 h`, `1 h 5 min` like the daemon's `duration_words`.
pub fn duration_text(secs: f64) -> String {
    let s = secs.round() as u64;
    match (s / 3600, (s / 60) % 60, s % 60) {
        (0, 0, s) => format!("{s} s"),
        (0, m, 0) => format!("{m} min"),
        (0, m, s) => format!("{m} min {s} s"),
        (h, 0, _) => format!("{h} h"),
        (h, m, _) => format!("{h} h {m} min"),
    }
}

// ---- errors ---------------------------------------------------------------------------------

/// The translated sentence for an error with this code and English `message`; the English
/// message when the daemon's text has no stable id here.
pub fn error_text(code: Option<ErrorCode>, message: &str) -> String {
    match code {
        Some(c) => message::describe(c, message),
        // The code was lost (a message kept as a plain string): any template may match.
        None => message::describe_any(message),
    }
    .and_then(|p| translated(&p))
    .unwrap_or_else(|| message.to_owned())
}

/// [`error_text`] for a message whose code was lost on the way (a plain string from
/// `Backend::call`, an error toast body): any stable id may match; other text stays.
pub fn any_error_text(message: &str) -> String {
    error_text(None, message)
}

/// [`error_text`] for the error type of `Backend::call_coded`.
pub fn call_error_text(e: &CallError) -> String {
    error_text(e.code, &e.message)
}

/// [`error_text`] for the error type of the AI client (its code is the code's string).
pub fn ai_error_text(e: &AiError) -> String {
    let code = e
        .code
        .as_deref()
        .and_then(|c| serde_json::from_value::<ErrorCode>(Value::String(c.to_owned())).ok());
    error_text(code, &e.message)
}

/// An error kept in a view's state and worded when it is drawn, so a note that is still on
/// screen after a language switch follows the switch (the daemon's stable ids are translated at
/// paint; text the daemon has no id for, and provider text, stays as it came).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorNote {
    code: Option<ErrorCode>,
    message: String,
    /// A sentence the app itself words (it has no daemon id).
    app: Option<AppNote>,
}

/// The sentences of the app that an [`ErrorNote`] can hold instead of daemon text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppNote {
    /// The daemon has no AI actions (an older build).
    AiUnavailable,
    /// There is no daemon (or no library) to ask.
    LibraryUnavailable,
    /// The same with a full stop (the editor's opening screen).
    LibraryUnavailableDot,
    /// A capture finished without an id.
    CaptureNoId,
    /// `original.png` could not be read (the OS text).
    ReadOriginal(String),
    /// `original.png` could not be decoded (the image library's text).
    DecodeOriginal(String),
}

impl From<String> for ErrorNote {
    fn from(message: String) -> ErrorNote {
        ErrorNote::plain(message)
    }
}

impl From<&str> for ErrorNote {
    fn from(message: &str) -> ErrorNote {
        ErrorNote::plain(message)
    }
}

impl ErrorNote {
    pub fn from_call(e: &CallError) -> ErrorNote {
        ErrorNote {
            code: e.code,
            message: e.message.clone(),
            app: None,
        }
    }

    pub fn from_ai(e: &AiError) -> ErrorNote {
        ErrorNote {
            code: e
                .code
                .as_deref()
                .and_then(|c| serde_json::from_value(Value::String(c.to_owned())).ok()),
            message: e.message.clone(),
            app: None,
        }
    }

    /// A sentence of the app, worded when drawn.
    pub fn app(note: AppNote) -> ErrorNote {
        ErrorNote {
            code: None,
            message: String::new(),
            app: Some(note),
        }
    }

    /// A sentence of the app that carries a detail, and the detail as plain text for code that
    /// looks at it.
    pub fn app_with(note: AppNote, detail: String) -> ErrorNote {
        ErrorNote {
            code: None,
            message: detail,
            app: Some(note),
        }
    }

    /// Text that is shown as it is (an app-made English sentence has no id here: use a reason
    /// enum of the view for those).
    pub fn plain(message: impl Into<String>) -> ErrorNote {
        ErrorNote {
            code: None,
            message: message.into(),
            app: None,
        }
    }

    /// The text as it came (the daemon's English message, or the detail of an app note): for
    /// code that looks for words in it, never for display.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The note in the current language.
    pub fn text(&self) -> String {
        match &self.app {
            Some(AppNote::AiUnavailable) => {
                tr!("The AI actions are not available in this build of the daemon.")
            }
            Some(AppNote::LibraryUnavailable) => tr!("The library is not available"),
            Some(AppNote::LibraryUnavailableDot) => tr!("The library is not available."),
            Some(AppNote::CaptureNoId) => tr!("The capture has no id"),
            Some(AppNote::ReadOriginal(e)) => tr!("cannot read the original: {e}", e = e),
            Some(AppNote::DecodeOriginal(e)) => tr!("cannot decode the original: {e}", e = e),
            None => error_text(self.code, &self.message),
        }
    }
}

/// One arm per `cm_core::message::TEMPLATES` id (a test makes sure none is missing).
fn translated(p: &MessageParts) -> Option<String> {
    let g = |k: &str| p.get(k).to_owned();
    Some(match p.id {
        "declined" => tr!("The user declined the request"),
        "confirmation_timeout" => tr!("No answer within {wait}", wait = g("wait")),
        "confirmation_queue_full" => {
            tr!("Too many confirmation requests are waiting; answer or dismiss them first")
        }
        "recording_running" => tr!("A recording is already running"),
        "selftest_running" => {
            tr!("The 60 fps self-test is running; wait for it to finish or stop it")
        }
        "disk_low_to_start" => tr!(
            "Only {mb} MB free; recording needs more than 1 GB free to start",
            mb = g("mb")
        ),
        "hardware_encoder_unavailable" => tr!(
            "This Mac has no usable hardware H.264 encoder for this recording. Try a lower resolution, update to macOS 13 or later, or use a Mac from about 2016 or later."
        ),
        "ffmpeg_not_found" => tr!(
            "FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy."
        ),
        "ffmpeg_bundle_missing" => tr!(
            "FFmpeg was not found: the copy that ships inside CaptureMann is missing, so the app is damaged or incomplete. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)"
        ),
        "ffmpeg_env_invalid" => tr!(
            "FFmpeg was not found: CM_FFMPEG points to `{path}`, which is not a file. Fix or unset CM_FFMPEG.",
            path = g("path")
        ),
        "ffmpeg_bundle_cannot_run" => tr!(
            "FFmpeg was not found: the copy inside CaptureMann (`{path}`) cannot be run ({error}). The app is damaged or macOS blocked it. Reinstall CaptureMann. (Advanced: set CM_FFMPEG to an ffmpeg executable.)",
            path = g("path"),
            error = g("error")
        ),
        "ffmpeg_permission_denied" => tr!(
            "FFmpeg at `{path}` cannot be run (permission denied). FFmpeg was not found: install it with Homebrew (brew install ffmpeg) or set CM_FFMPEG to the ffmpeg executable. CaptureMann releases include their own copy.",
            path = g("path")
        ),
        "no_recording_running" => tr!("No recording is running"),
        "recording_not_running" => tr!("Recording `{id}` is not running", id = g("id")),
        "recording_unknown" => tr!("Recording `{id}` is not known", id = g("id")),
        "recording_ended" => tr!("The recording has already ended"),
        "recording_ended_before_answer" => tr!("The recording ended before it could answer"),
        "recording_ended_before_restart" => tr!("The recording ended before it could restart"),
        "no_display" => tr!(
            "No display found. If the screen is locked or asleep, unlock it and try again."
        ),
        "display_not_found" => tr!("Display {id} not found", id = g("id")),
        "window_not_found" => tr!("Window {id} not found", id = g("id")),
        "window_minimized" => tr!("Window is minimized"),
        "recording_folder_moved" => tr!(
            "The recording folder `{path}` was moved or deleted",
            path = g("path")
        ),
        "capture_folder_moved" => {
            tr!("the folder of `{id}` was moved or deleted", id = g("id"))
        }
        "capture_still_recording" => tr!(
            "`{id}` is still being recorded; stop the recording before you {action} it",
            id = g("id"),
            action = action_word(p.get("action"))
        ),
        "capture_exporting" => tr!(
            "`{id}` is being exported; wait for the export to finish or cancel it before you {action} it",
            id = g("id"),
            action = action_word(p.get("action"))
        ),
        "no_export_yet" => tr!(
            "`{id}` has no export yet; run `export_video` first",
            id = g("id")
        ),
        "no_export_to_upload" => tr!("`{id}` has no export to upload", id = g("id")),
        "no_captions_yet" => tr!(
            "`{id}` has no captions yet; run `transcribe` first",
            id = g("id")
        ),
        "no_transcript_yet" => tr!(
            "`{id}` has no transcript yet; run `transcribe` first",
            id = g("id")
        ),
        "model_not_installed" => tr!("`{model}` is not installed", model = g("model")),
        "no_destination" => tr!("no destination is set up"),
        "no_publish_account" => tr!("no publish account is set up"),
        "account_not_connected" => {
            tr!("This account is not connected. Connect it again in Settings → Destinations.")
        }
        "account_credentials_missing" => tr!(
            "The app credentials of this account are missing. Connect it again in Settings → Destinations."
        ),
        "account_renew_refused" => tr!(
            "The provider refused to renew the connection. Connect the account again and check the client ID."
        ),
        "screen_access_off" => tr!(
            "Screen Recording access is off. Turn it on in System Settings > Privacy & Security > Screen Recording, then restart CaptureMann."
        ),
        "screen_permission_not_granted" => tr!(
            "Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen Recording, then restart CaptureMann."
        ),
        "screen_permission_not_granted_audio" => tr!(
            "Screen Recording permission is not granted. Enable it in System Settings > Privacy & Security > Screen & System Audio Recording, then restart CaptureMann."
        ),
        "microphone_access_off" => tr!(
            "Microphone access is off. Turn it on in System Settings > Privacy & Security > Microphone."
        ),
        "camera_not_requested" => {
            tr!("Camera access has not been requested yet. Allow camera access when prompted.")
        }
        "camera_denied" => tr!(
            "Camera access is denied. Enable it in System Settings > Privacy & Security > Camera."
        ),
        "camera_restricted" => {
            tr!("Camera access is restricted by a device policy and cannot be granted here.")
        }
        _ => return None,
    })
}

/// The verb the daemon puts in "before you {action} it" (`delete`, `publish`, ...).
fn action_word(raw: &str) -> String {
    match raw {
        "delete" => tr_ctx!("verb before 'it'", "delete"),
        "publish" => tr_ctx!("verb before 'it'", "publish"),
        "upload" => tr_ctx!("verb before 'it'", "upload"),
        "export" => tr_ctx!("verb before 'it'", "export"),
        "edit" => tr_ctx!("verb before 'it'", "edit"),
        other => other.to_owned(),
    }
}

// ---- confirmations --------------------------------------------------------------------------

/// The dialog's one-sentence summary for action `kind`, built from `params`; `english` (the
/// daemon's own sentence) when the kind or its parameters are not understood.
pub fn confirm_summary(kind: &str, params: &Value, english: &str) -> String {
    let built = match kind {
        "start_recording" => start_summary(params, false),
        "restart_recording" => start_summary(params, true),
        "run_fps_selftest" => selftest_summary(params),
        "discard_recording" => Some(tr!("Discard the current recording and delete its files")),
        "publish_video" => publish_summary(params),
        "delete_capture" => delete_summary(params),
        "move_library" => move_library_summary(params),
        "download_model" => model_summary(params, true),
        "remove_model" => model_summary(params, false),
        "tool_ask_first" => ask_first_summary(params),
        _ => None,
    };
    built.unwrap_or_else(|| english.to_owned())
}

/// The title of the tool a generic ask-first dialog is about: the Agents page's own title for a
/// tool this version knows (translated), else the daemon's English one.
pub fn ask_first_tool_title(params: &Value) -> Option<String> {
    let tool = text(params, "tool")?;
    let known = crate::ui::settings::model::tool_title(tool);
    if known != crate::ui::settings::model::tool_title("") {
        return Some(known);
    }
    Some(text(params, "title").map_or_else(|| humanize_key(tool), str::to_owned))
}

/// AGT-2.AC11: the sentence of the generic dialog for a tool the user set to Ask first. `None`
/// (the daemon's English is shown) when the parameters are not understood.
fn ask_first_summary(p: &Value) -> Option<String> {
    let title = ask_first_tool_title(p)?;
    let agent = crate::confirm::display_client_name(text(p, "client")?);
    Some(tr!(
        "{agent} is asking to run \u{201C}{title}\u{201D}. You set this tool to Ask first in Settings \u{2192} Agents & CLI.",
        agent = agent,
        title = title
    ))
}

/// The dialog's rows for action `kind`: the daemon's English `details` (label to text) with the
/// labels translated and every value that has typed parameters worded from them. The row order is
/// the daemon's. Rows this version cannot word stay as the daemon sent them.
pub fn confirm_rows(kind: &str, details: &Value, params: &Value) -> Vec<(String, String)> {
    let Value::Object(map) = details else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    for (k, v) in map {
        if k == "request_id" || v.is_null() {
            continue;
        }
        let value = row_value(kind, k, params).or_else(|| plain_value(v));
        if let Some(value) = value {
            rows.push((row_label(k), value));
        }
    }
    rows
}

/// A value as the daemon sent it, with booleans worded.
fn plain_value(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(if *b { tr!("Yes") } else { tr!("No") }),
        Value::Number(n) => Some(n.to_string()),
        other => Some(other.to_string()),
    }
}

/// `system_audio` -> `System audio`: the English a key shows when it has no message of its own.
pub fn humanize_key(k: &str) -> String {
    let s = k.replace('_', " ");
    let mut cs = s.chars();
    match cs.next() {
        Some(c) => c.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}

/// The label of a detail row. The English is `humanize_key(key)` for every key, so the dialog
/// reads as it always did.
pub fn row_label(key: &str) -> String {
    // The numbered rows of a delete with remote copies.
    for (prefix, label) in [
        ("remote_delete_", 0),
        ("remote_gone_", 1),
        ("remote_left_", 2),
    ] {
        if let Some(n) = key
            .strip_prefix(prefix)
            .filter(|n| n.parse::<u32>().is_ok())
        {
            return match label {
                0 => tr!("Remote delete {n}", n = n),
                1 => tr!("Remote gone {n}", n = n),
                _ => tr!("Remote left {n}", n = n),
            };
        }
    }
    match key {
        "tool" => tr!("Tool"),
        "target" => tr!("Target"),
        "output" => tr!("Output"),
        "screen" => tr!("Screen"),
        "microphone" => tr!("Microphone"),
        "camera" => tr!("Camera"),
        "system_audio" => tr!("System audio"),
        "cursor" => tr!("Cursor"),
        "save_to" => tr!("Save to"),
        "max_duration" => tr!("Max duration"),
        "countdown" => tr!("Countdown"),
        "excluded_windows" => tr!("Excluded windows"),
        "discarded" => tr!("Discarded"),
        "disk_space" => tr!("Disk space"),
        "purpose" => tr!("Purpose"),
        "recording" => tr!("Recording"),
        "title" => tr!("Title"),
        "recorded" => tr!("Recorded"),
        "this_cannot_be_undone" => tr!("This cannot be undone"),
        "capture" => tr!("Capture"),
        "id" => tr!("Id"),
        "capture_id" => tr!("Capture id"),
        "kind" => tr!("Kind"),
        "created" => tr!("Created"),
        "size" => tr!("Size"),
        "result" => tr!("Result"),
        "remote" => tr!("Remote"),
        "permanent" => tr!("Permanent"),
        "order" => tr!("Order"),
        "platform" => tr!("Platform"),
        "account" => tr!("Account"),
        "privacy" => tr!("Privacy"),
        "privacy_source" => tr!("Privacy source"),
        "made_for_kids" => tr!("Made for kids"),
        "file" => tr!("File"),
        "description" => tr!("Description"),
        "tags" => tr!("Tags"),
        "playlist" => tr!("Playlist"),
        "thumbnail" => tr!("Thumbnail"),
        "captions" => tr!("Captions"),
        "sends" => tr!("Sends"),
        "uploads_today" => tr!("Uploads today"),
        "note" => tr!("Note"),
        "model" => tr!("Model"),
        "name" => tr!("Name"),
        "size_mb" => tr!("Size mb"),
        "destination" => tr!("Destination"),
        "network" => tr!("Network"),
        "from" => tr_ctx!("dialog row", "From"),
        "to" => tr_ctx!("dialog row", "To"),
        "captures" => tr_ctx!("dialog row", "Captures"),
        "restart" => tr_ctx!("dialog row", "Restart"),
        other => humanize_key(other),
    }
}

/// The text of one row worded from `params`, or `None` to show the daemon's English value.
fn row_value(kind: &str, key: &str, p: &Value) -> Option<String> {
    match kind {
        "start_recording" | "restart_recording" | "run_fps_selftest" => start_row(kind, key, p),
        "discard_recording" => match key {
            "this_cannot_be_undone" => Some(tr!("The take is deleted and cannot be recovered")),
            _ => None,
        },
        "publish_video" => publish_row(key, p),
        "delete_capture" => delete_row(key, p),
        "move_library" => move_library_row(key, p),
        "download_model" => match key {
            "network" => Some(tr!("Downloads the model from the internet")),
            _ => None,
        },
        _ => None,
    }
}

// -- start_recording, restart_recording, run_fps_selftest --

/// `display 1`, `the window "T"`, ... as the noun phrase of the summary sentence.
fn target_phrase(t: &Value) -> Option<String> {
    let id = num(t, "id");
    Some(match text(t, "type")? {
        "display" => tr!("display {id}", id = id?),
        "window" => match text(t, "title") {
            Some(title) => tr!("the window \"{title}\"", title = title),
            None => tr!("window {id}", id = id?),
        },
        "region" => tr!(
            "a {w} x {h} region of display {id}",
            w = num(t, "width")?,
            h = num(t, "height")?,
            id = num(t, "display_id")?
        ),
        _ => return None,
    })
}

/// The target row of the dialog.
fn target_row(t: &Value) -> Option<String> {
    let (w, h) = (num(t, "width")?, num(t, "height")?);
    Some(match text(t, "type")? {
        "display" => match text(t, "name") {
            Some(name) => tr!(
                "Display {id} \"{name}\", {w} x {h} px",
                id = num(t, "id")?,
                name = name,
                w = w,
                h = h
            ),
            None => tr!(
                "Display {id}, {w} x {h} px",
                id = num(t, "id")?,
                w = w,
                h = h
            ),
        },
        "window" => match (text(t, "title"), text(t, "app")) {
            (Some(title), Some(app)) => tr!(
                "Window \"{title}\" of {app}, {w} x {h} px (follows the window)",
                title = title,
                app = app,
                w = w,
                h = h
            ),
            _ => tr!("Window {id}", id = num(t, "id")?),
        },
        "region" => tr!(
            "Region {w} x {h} px at ({x}, {y}) on display {id}",
            w = w,
            h = h,
            x = num_or_zero(t, "x"),
            y = num_or_zero(t, "y"),
            id = num(t, "display_id")?
        ),
        _ => return None,
    })
}

fn num_or_zero(v: &Value, k: &str) -> i64 {
    v.get(k).and_then(Value::as_i64).unwrap_or(0)
}

fn start_summary(p: &Value, restart: bool) -> Option<String> {
    let target = target_phrase(p.get("target")?)?;
    let (w, h, fps) = (num(p, "width")?, num(p, "height")?, num(p, "fps")?);
    let folder = text(p, "save_to")?;
    let mut sensors = Vec::new();
    if p.get("microphone").is_some_and(|m| !m.is_null()) {
        sensors.push(tr!("the microphone"));
    }
    if p.get("camera").is_some_and(|c| !c.is_null()) {
        sensors.push(tr!("the camera"));
    }
    if flag(p, "system_audio") {
        sensors.push(tr!("system audio"));
    }
    let with = match sensors.as_slice() {
        [] => None,
        [a] => Some(a.clone()),
        [a, b] => Some(tr!("{a} and {b}", a = a, b = b)),
        [a, b, c] => Some(tr!("{a}, {b} and {c}", a = a, b = b, c = c)),
        _ => return None,
    };
    Some(match (restart, with) {
        (false, None) => tr!(
            "Record {target} at {w} x {h}, {fps} fps, and save it to {folder}",
            target = target,
            w = w,
            h = h,
            fps = fps,
            folder = folder
        ),
        (false, Some(with)) => tr!(
            "Record {target} at {w} x {h}, {fps} fps with {with}, and save it to {folder}",
            target = target,
            w = w,
            h = h,
            fps = fps,
            with = with,
            folder = folder
        ),
        (true, None) => tr!(
            "Discard the current take and start over: record {target} at {w} x {h}, {fps} fps, and save it to {folder}",
            target = target,
            w = w,
            h = h,
            fps = fps,
            folder = folder
        ),
        (true, Some(with)) => tr!(
            "Discard the current take and start over: record {target} at {w} x {h}, {fps} fps with {with}, and save it to {folder}",
            target = target,
            w = w,
            h = h,
            fps = fps,
            with = with,
            folder = folder
        ),
    })
}

fn selftest_summary(p: &Value) -> Option<String> {
    let t = p.get("target")?;
    // The daemon cuts the target at its first comma; a name with a comma keeps the daemon's text.
    let named = [text(t, "name"), text(t, "title"), text(t, "app")];
    if named.iter().flatten().any(|n| n.contains(',')) {
        return None;
    }
    let target = match text(t, "type")? {
        "display" => match text(t, "name") {
            Some(name) => tr!("Display {id} \"{name}\"", id = num(t, "id")?, name = name),
            None => tr!("Display {id}", id = num(t, "id")?),
        },
        "window" => match (text(t, "title"), text(t, "app")) {
            (Some(title), Some(app)) => {
                tr!("Window \"{title}\" of {app}", title = title, app = app)
            }
            _ => tr!("Window {id}", id = num(t, "id")?),
        },
        _ => return None,
    };
    Some(tr!(
        "Run the 60 fps self-test: record {target} at {w} x {h}, 60 fps, for {duration} to check that this machine keeps up. Nothing is saved: the test video is deleted when the test ends",
        target = target,
        w = num(p, "width")?,
        h = num(p, "height")?,
        duration = duration_text(float(p, "duration_s")?)
    ))
}

fn start_row(kind: &str, key: &str, p: &Value) -> Option<String> {
    let selftest = kind == "run_fps_selftest";
    Some(match key {
        "target" => return target_row(p.get("target")?),
        "output" => tr!(
            "{w} x {h} px, {fps} fps, {codec}, about {mb} MB per minute (an estimate: the real size varies with what is on screen)",
            w = num(p, "width")?,
            h = num(p, "height")?,
            fps = num(p, "fps")?,
            codec = text(p, "codec")?,
            mb = format!("{:.0}", float(p, "mb_per_minute")?)
        ),
        "screen" => tr!("On (always recorded)"),
        "microphone" => match p.get("microphone")? {
            Value::Null => tr!("Off"),
            m => {
                let device = text(m, "device")
                    .map_or_else(|| tr!("the default microphone"), str::to_owned);
                if flag(m, "denoise") {
                    tr!("On: {device}, noise suppression on", device = device)
                } else {
                    tr!("On: {device}", device = device)
                }
            }
        },
        "camera" => match p.get("camera")? {
            Value::Null => tr!("Off"),
            c => tr!(
                "On: {device}, {resolution} (the camera light turns on)",
                device = text(c, "device")
                    .map_or_else(|| tr!("the default camera"), str::to_owned),
                resolution = text(c, "resolution")?
            ),
        },
        "system_audio" => {
            if flag(p, "system_audio") {
                tr!("On")
            } else {
                tr!("Off")
            }
        }
        "cursor" => {
            if flag(p, "cursor") {
                tr!("Position, clicks and scroll direction (never keystrokes)")
            } else {
                tr!("Off")
            }
        }
        "save_to" if selftest => {
            tr!("Nothing is saved: the test video is deleted when the test ends")
        }
        "save_to" => {
            let folder = text(p, "save_to")?;
            match text(p, "title") {
                Some(title) => tr!(
                    "{folder} as \"{title}\"",
                    folder = folder,
                    title = title
                ),
                None => folder.to_owned(),
            }
        }
        "max_duration" if selftest => duration_text(float(p, "duration_s")?),
        "max_duration" => match float(p, "max_duration_s") {
            Some(s) => duration_text(s),
            None => tr!("No limit"),
        },
        "countdown" => match num(p, "countdown_s")? {
            0 => tr_ctx!("countdown", "None"),
            n => format!("{n} s"),
        },
        "excluded_windows" => {
            tr!("CaptureMann's own windows and your password managers never appear")
        }
        "discarded" => tr!("The current take is deleted and cannot be recovered"),
        "purpose" => tr!(
            "Measures late and dropped frames at 60 fps; only the result (pass or fail) is kept"
        ),
        "disk_space" => {
            let d = p.get("disk")?;
            let minutes = float(d, "minutes_left")?;
            let left = if minutes >= 600.0 {
                format!("{} h", fmt::decimal(minutes / 60.0, 0))
            } else if minutes >= 120.0 {
                format!("{} h", fmt::decimal(minutes / 60.0, 1))
            } else {
                format!("{} min", fmt::decimal(minutes, 0))
            };
            tr!(
                "{free} GB free, room for about {left} at this setting (an estimate)",
                free = fmt::decimal(float(d, "free_gb")?, 1),
                left = left
            )
        }
        _ => return None,
    })
}

// -- publish_video --

fn privacy_word(raw: &str, capital: bool) -> Option<String> {
    Some(match (raw, capital) {
        ("private", true) => tr_ctx!("privacy", "Private"),
        ("unlisted", true) => tr_ctx!("privacy", "Unlisted"),
        ("public", true) => tr_ctx!("privacy", "Public"),
        ("private", false) => tr_ctx!("privacy value", "private"),
        ("unlisted", false) => tr_ctx!("privacy value", "unlisted"),
        ("public", false) => tr_ctx!("privacy value", "public"),
        _ => return None,
    })
}

fn publish_summary(p: &Value) -> Option<String> {
    let platform = text(p, "platform_name");
    match (text(p, "title"), platform, text(p, "privacy")) {
        (Some(title), Some(platform), Some(privacy)) => Some(tr!(
            "Publish \"{title}\" to {platform} as {privacy}",
            title = title,
            platform = platform,
            privacy = privacy_word(privacy, true)?
        )),
        _ => {
            // A daemon whose publisher is external: only the capture and the platform are known.
            let id = text(p, "capture_id")?;
            let platform = text(p, "platform").map_or_else(|| tr!("a platform"), str::to_owned);
            Some(tr!(
                "Publish capture {id} to {platform}",
                id = id,
                platform = platform
            ))
        }
    }
}

fn publish_row(key: &str, p: &Value) -> Option<String> {
    Some(match key {
        "privacy" => return privacy_word(text(p, "privacy")?, false),
        "privacy_source" => tr!("not given: the account's default"),
        "file" => tr!("Exported first with the `{preset}` preset", preset = text(p, "preset")?),
        "size" => size_text(num(p, "size_bytes")?),
        "captions" => tr!("Uploads the captions as an SRT track"),
        "sends" => tr!(
            "Uploads this video over the internet to {platform}, channel \"{account}\"",
            platform = text(p, "platform_name")?,
            account = text(p, "account")?
        ),
        "uploads_today" => tr!(
            "{used} of {limit} used today",
            used = num(p, "uploads_used")?,
            limit = num(p, "uploads_limit")?
        ),
        "note" => tr!(
            "{platform} keeps videos from an unverified API project private; the result says so if it happens",
            platform = text(p, "platform_name")?
        ),
        _ => return None,
    })
}

// -- delete_capture --

fn delete_summary(p: &Value) -> Option<String> {
    let (Some(title), Some(bytes)) = (text(p, "title"), num(p, "size_bytes")) else {
        return Some(tr!("Delete capture {id}", id = text(p, "capture_id")?));
    };
    let size = size_text(bytes);
    Some(match num(p, "remote_copies").unwrap_or(0) {
        0 => tr!("Delete \"{title}\" ({size})", title = title, size = size),
        n => trn!(
            "Delete \"{title}\" ({size}) and permanently delete {n} remote copy",
            "Delete \"{title}\" ({size}) and permanently delete {n} remote copies",
            n,
            title = title,
            size = size
        ),
    })
}

fn delete_row(key: &str, p: &Value) -> Option<String> {
    use crate::remote_text as rt;
    let numbered = |prefix: &str| -> Option<usize> {
        key.strip_prefix(prefix)?
            .parse::<usize>()
            .ok()?
            .checked_sub(1)
    };
    let items = p.get("remote_items").unwrap_or(&Value::Null);
    if let Some(i) = numbered("remote_delete_") {
        return rt::item_text(rt::items_of(items, "delete").get(i)?);
    }
    if let Some(i) = numbered("remote_gone_") {
        return rt::gone_text(rt::items_of(items, "gone").get(i)?);
    }
    if let Some(i) = numbered("remote_left_") {
        return rt::left_row(rt::items_of(items, "left").get(i)?);
    }
    Some(match key {
        "remote" => tr!(
            "No remote copy is recorded for this capture, so nothing is deleted remotely"
        ),
        "permanent" => tr!(
            "Deleting remote copies is permanent and cannot be undone: Google Drive files skip the Drive Trash and a deleted YouTube video cannot be restored"
        ),
        "order" => tr!(
            "The remote copies are deleted first; if one fails the capture stays here and the result says what was and was not deleted"
        ),
        "size" => size_text(num(p, "size_bytes")?),
        "result" => {
            if num(p, "remote_copies").unwrap_or(0) > 0 {
                tr!("The remote copies are deleted, then the capture is moved to the Trash")
            } else {
                tr!("Moved to the Trash")
            }
        }
        _ => return None,
    })
}

// -- move_library --

fn move_library_summary(p: &Value) -> Option<String> {
    let (from, to) = (text(p, "from")?, text(p, "to")?);
    let n = num(p, "bundles").unwrap_or(0);
    Some(match text(p, "mode")? {
        "move" => tr!(
            "Move the library from {from} to {to} ({size}, {n} captures)",
            from = from,
            to = to,
            size = size_text(num(p, "size_bytes")?),
            n = n
        ),
        "fresh" => tr!(
            "Start a new empty library at {to}; the captures in {from} stay where they are",
            to = to,
            from = from
        ),
        "use_existing" => tr!(
            "Switch to the existing library at {to} ({n} captures); the captures in {from} stay where they are",
            to = to,
            from = from,
            n = n
        ),
        _ => return None,
    })
}

fn move_library_row(key: &str, p: &Value) -> Option<String> {
    Some(match key {
        "size" => size_text(num(p, "size_bytes")?),
        "result" => match text(p, "mode")? {
            "move" => {
                tr!("The captures are copied and checked; nothing is deleted from the old folder")
            }
            _ => tr!("Nothing is copied; the old files stay where they are"),
        },
        "restart" => tr!("CaptureMann starts using the new folder after it restarts"),
        _ => return None,
    })
}

// -- download_model, remove_model --

fn model_summary(p: &Value, download: bool) -> Option<String> {
    let (name, mb) = (text(p, "name")?, num(p, "size_mb")?);
    Some(if download {
        tr!(
            "Download the speech model \"{name}\" (about {mb} MB) from the internet",
            name = name,
            mb = mb
        )
    } else {
        tr!(
            "Delete the speech model \"{name}\" ({mb} MB) from this computer",
            name = name,
            mb = mb
        )
    })
}

/// A recording note (`recording.warning` event, a warning of the start result) worded in the
/// chosen language from its English `message`. The ids are `cm_core::message::WARNING_TEMPLATES`;
/// a message without an id (a new daemon, the cursor's own text, a tick error) is shown as sent,
/// and the OS or tool detail inside a sentence stays as it came.
pub fn warning_text(message: &str) -> String {
    message::describe_warning(message)
        .and_then(|p| warning_translated(&p))
        .unwrap_or_else(|| message.to_owned())
}

/// The name of a track kind (`screen`, `mic`, ...) as it reads inside a sentence; an unknown
/// name stays.
fn track_word(raw: &str) -> String {
    match raw {
        "screen" => tr_ctx!("track name", "screen"),
        "camera" => tr_ctx!("track name", "camera"),
        "mic" => tr_ctx!("track name", "mic"),
        "system" => tr_ctx!("track name", "system"),
        "cursor" => tr_ctx!("track name", "cursor"),
        other => other.to_owned(),
    }
}

/// One arm per `cm_core::message::WARNING_TEMPLATES` id (a test makes sure none is missing).
fn warning_translated(p: &MessageParts) -> Option<String> {
    let g = |k: &str| p.get(k).to_owned();
    Some(match p.id {
        "warn_disk_low" => tr!(
            "About {minutes} minutes of disk space left",
            minutes = g("minutes")
        ),
        "warn_disk_low_plan" => tr!(
            "About {minutes} minutes of disk space left at this setting",
            minutes = g("minutes")
        ),
        "warn_disk_full" => tr!("Recording stopped: only {mb} MB free", mb = g("mb")),
        "warn_screen_late" => tr!(
            "{pct}% of screen frames were late over the last 30 s",
            pct = g("pct")
        ),
        "warn_screen_healthy" => tr!("Screen capture is keeping up again"),
        "warn_capture_stalled" => tr!(
            "Screen capture delivered no frames for {secs} s (display asleep or capture stuck); the video repeats the last frame until it resumes",
            secs = g("secs")
        ),
        "warn_screen_ended" => tr!("Screen capture ended: {detail}", detail = g("detail")),
        "warn_screen_encoder" => tr!("Screen encoder failed: {detail}", detail = g("detail")),
        "warn_mic_fallback" => tr!(
            "Microphone {requested} is not available; using {using}",
            requested = g("requested"),
            using = g("using")
        ),
        "warn_audio_changed" => tr!(
            "Audio device changed from {from} to {to}",
            from = g("from"),
            to = g("to")
        ),
        "warn_audio_lost" => tr!(
            "Audio device {name} was lost; recording silence",
            name = g("name")
        ),
        // TRANSLATORS: FACT: owner review required (what happens when the microphone is silent)
        "warn_mic_silent" => tr!(
            "The microphone has delivered only silence for 15 s (muted, or its access was revoked); the recording continues"
        ),
        "warn_audio_write_failed" => tr!(
            "The audio file could not be written ({detail}); the track ends here and the recording continues",
            detail = g("detail")
        ),
        "warn_camera_lost" => {
            tr!("The camera was disconnected; the camera track has a gap")
        }
        "warn_camera_back" => tr!("The camera is back"),
        "warn_camera_write_failed" => tr!(
            "The camera file could not be written: {detail}",
            detail = g("detail")
        ),
        // TRANSLATORS: FACT: owner review required (a permission was removed during a recording)
        "warn_permission_removed_stop" => tr!(
            "{what} permission was removed; the recording is stopped and what was recorded is kept",
            what = g("what")
        ),
        // TRANSLATORS: FACT: owner review required (a permission was removed during a recording)
        "warn_permission_removed_go_on" => tr!(
            "{what} permission was removed; the recording continues without this track from here on",
            what = g("what")
        ),
        "warn_engine_failed" => tr!("The recording engine failed; saving what was recorded"),
        "warn_engine_stopped" => {
            tr!("The recording engine was stopped; saving what was recorded")
        }
        "warn_track_start_failed" => tr!(
            "{track} track could not start: {detail}",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_track_pause_failed" => tr!(
            "{track} track could not pause: {detail}",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_track_resume_failed" => tr!(
            "{track} track could not resume: {detail}",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_track_finish_failed" => tr!(
            "{track} track could not be finished: {detail}; its files were kept in the recording folder",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_track_stopped_early" => tr!(
            "The {track} track stopped early ({detail}); what was written is kept",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_file_ended_early" => tr!(
            "The {track} file ended early ({detail}); what was written is kept",
            track = track_word(p.get("track")),
            detail = g("detail")
        ),
        "warn_fps_fallback" => tr!("Recording at 30 fps. {detail}", detail = g("detail")),
        "warn_fps60_camera" => tr!(
            "The 60 fps check measured the screen alone. With the camera on, frames may arrive late; watch the health indicator or record at 30 fps"
        ),
        // TRANSLATORS: FACT: owner review required (what is recorded without Input Monitoring)
        "warn_cursor_clicks_off" => tr!(
            "Cursor clicks will not be recorded (only the position): Input Monitoring access is off."
        ),
        _ => return None,
    })
}

/// The parameters of a request as an object (an empty one when the daemon sent none).
pub fn params_of(v: &Value) -> Value {
    match v {
        Value::Object(_) => v.clone(),
        _ => Value::Object(Map::new()),
    }
}

#[cfg(test)]
mod tests;
