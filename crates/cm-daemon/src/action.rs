//! The typed action registry: one table of names, tiers and mutability, and one [`Action`]
//! enum holding the validated input of each action.
//!
//! JSON-RPC method names equal MCP tool names (AGT-1). Parsing is strict about shapes but
//! lenient about extra keys only where noted; error messages name the field (`invalid_input`).

use cm_capture::{CaptureTarget, RegionRect};
use cm_core::{AnnotationOp, CmError, PhysicalPoint, PhysicalRect, PhysicalSize};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Permission tier (specs/02-agent-interface.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tier {
    /// Runs immediately.
    Free,
    /// Waits for an on-screen confirmation (agents only).
    Confirm,
    /// Not exposed at all; see [`NEVER_TIER`].
    Never,
}

/// One row of the tier table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActionSpec {
    pub name: &'static str,
    pub tier: Tier,
    /// Changes state (so: read-only clients are refused, `request_id` is accepted).
    pub mutating: bool,
}

const fn spec(name: &'static str, tier: Tier, mutating: bool) -> ActionSpec {
    ActionSpec {
        name,
        tier,
        mutating,
    }
}

/// Every exposed action, as data. A test keeps this in step with [`Action`].
pub const ACTIONS: &[ActionSpec] = &[
    spec("list_targets", Tier::Free, false),
    spec("capture_screenshot", Tier::Free, true),
    spec("annotate", Tier::Free, true),
    spec("get_capture", Tier::Free, false),
    // Auto-redact (ANN-2.AC4): reads the original with the on-device text recognizer and proposes
    // blur rectangles. Read-only: accepting a proposal is an ordinary `annotate` with blur ops.
    spec("suggest_redactions", Tier::Free, false),
    spec("get_settings", Tier::Free, false),
    spec("set_settings", Tier::Free, true),
    spec("undo", Tier::Free, true),
    spec("redo", Tier::Free, true),
    spec("list_library", Tier::Free, false),
    // Library management: renaming changes the title and folder name of a capture (not an undo
    // step); `rescan_library` rebuilds the index from the folders (internal, see below).
    spec("rename_capture", Tier::Free, true),
    spec("rescan_library", Tier::Free, true),
    // Recording edits (EDT-1 to EDT-3): Free, one revision and one undo step each; they change
    // `manifest.edits` only and never touch the master tracks.
    spec("trim", Tier::Free, true),
    spec("cut_segments", Tier::Free, true),
    spec("restore_segments", Tier::Free, true),
    spec("split", Tier::Free, true),
    spec("set_speed", Tier::Free, true),
    spec("set_background", Tier::Free, true),
    spec("set_audio_mix", Tier::Free, true),
    spec("set_camera_layout", Tier::Free, true),
    spec("set_zooms", Tier::Free, true),
    spec("set_cursor_style", Tier::Free, true),
    spec("set_reframe", Tier::Free, true),
    spec("freeze_display", Tier::Free, true),
    spec("release_frame", Tier::Free, false),
    spec("window_thumbnail", Tier::Free, true),
    // Jobs (EXP-4, STO-3): Free per the spec's tool table, `cancel_job` included. Cancelling
    // changes state (so read-only clients are refused) but never reaches remote services.
    spec("get_job", Tier::Free, false),
    spec("list_jobs", Tier::Free, false),
    spec("cancel_job", Tier::Free, true),
    // `retry_job` puts a failed or cancelled job back in the queue and resumes one that paused
    // with `auth_expired`. Free: it only re-runs work the user already asked for.
    spec("retry_job", Tier::Free, true),
    // Export (EXP-1, EXP-4): Free per the spec's tool table. The job runs in the background and is
    // watched with `get_job` / `job.*` events. Only the interim preset `quick` exists for now.
    spec("export_video", Tier::Free, true),
    // Recording (REC-1..REC-5). Starting asks the user; the rest of the controls are Free because
    // the recording itself was already confirmed (REC-4). `discard_recording` and
    // `restart_recording` delete a take: Confirm for agents, direct for the user in the HUD.
    spec("start_recording", Tier::Confirm, true),
    spec("pause_recording", Tier::Free, true),
    spec("resume_recording", Tier::Free, true),
    // Mic mute (AUD-6): the track keeps running, the file gets digital silence for the muted
    // span. Free and mutating like pause and resume: the recording itself was confirmed.
    spec("set_mic_muted", Tier::Free, true),
    spec("stop_recording", Tier::Free, true),
    spec("get_recording_status", Tier::Free, false),
    spec("list_devices", Tier::Free, false),
    spec("discard_recording", Tier::Confirm, true),
    spec("restart_recording", Tier::Confirm, true),
    spec("skip_countdown", Tier::Free, true),
    // The 60 fps self-test (REC-3) records the screen for ~10 s and keeps nothing but the verdict.
    // It captures real pixels, so it is Confirm for agents like `start_recording` (NFR-11); the
    // user in process runs it directly. It shows the same indicator facts as a recording.
    spec("run_fps_selftest", Tier::Confirm, true),
    spec("publish_video", Tier::Confirm, true),
    spec("delete_capture", Tier::Confirm, true),
    // LIB-2: changing the library folder. Confirm for agents and not loosenable: no setting
    // changes a tier. The copy runs as the `library_move` job; the switch needs a restart.
    spec("move_library", Tier::Confirm, true),
    // Local AI (M5, specs/features/07-ai.md). Everything is Free except what uses the network or
    // deletes a model file: `download_model` (the only network traffic of the AI features, only
    // when asked) and `remove_model`. Suggestions only read; accepting one is `set_zooms` or
    // `cut_segments`.
    spec("list_models", Tier::Free, false),
    spec("download_model", Tier::Confirm, true),
    spec("import_model", Tier::Free, true),
    spec("select_model", Tier::Free, true),
    spec("remove_model", Tier::Confirm, true),
    spec("transcribe", Tier::Free, true),
    spec("get_transcript", Tier::Free, false),
    spec("get_captions", Tier::Free, false),
    spec("set_captions", Tier::Free, true),
    // AI-6a: find and replace over the caption lines, one undo step. The transcript is untouched.
    spec("replace_in_captions", Tier::Free, true),
    spec("export_captions", Tier::Free, true),
    spec("suggest_zooms", Tier::Free, false),
    spec("suggest_cuts", Tier::Free, false),
    // The caption switches (show in the preview, burn into exports): one revision, one undo step.
    spec("set_caption_options", Tier::Free, true),
    // Storage (M6, STO-1 to STO-5): copies go only to destinations the user set up in the app;
    // nothing is deleted or published, so both are Free.
    spec("list_destinations", Tier::Free, false),
    spec("upload", Tier::Free, true),
    // Publishing (M6, PUB-1, PUB-2): the accounts are set up in the app; an agent can read them
    // (label, defaults, quota; never a key or token). `publish_video` above is the Confirm one.
    spec("list_publish_accounts", Tier::Free, false),
];

/// Actions that are plumbing for first-party GUI clients and are deliberately **not** exposed
/// as a CLI command or an MCP tool (AGT-1.AC4 asks for parity "or explicitly internal"; this
/// list is the explicit part, and the CLI and MCP parity tests subtract it on purpose).
///
/// * `freeze_display` and `release_frame` serve the overlay's picker (CAP-1): an agent that
///   wants a screenshot calls `capture_screenshot`, which already covers every target.
///
/// * `window_thumbnail` serves the window picker (R19, R20): a downscaled PNG of one window. An
///   agent that wants to see a window calls `capture_screenshot`. It hands out screen pixels, so
///   it counts as mutating for read-only clients, like `freeze_display`.
///
/// * `discard_recording`, `restart_recording` and `skip_countdown` are the HUD's Discard, Restart
///   and "Space skips the countdown" (REC-5, REC-1.AC5). The spec gives Restart and Discard no MCP
///   tool or CLI command on purpose: an agent stops (`stop_recording`) and starts again
///   (`start_recording`, Confirm). Called in process as `Actor::User` they run directly (the HUD
///   asks "Discard this take?" itself); over the socket the caller is an agent, so the two that
///   delete a take are Confirm tier and show a dialog.
///
/// * `rescan_library` is the app's "Rescan library" (LIB-1.AC3): the desktop runs it in process on
///   the daemon's own library handle instead of opening a second one. Agents have `list_library`.
///
/// They remain callable over the socket by any client and obey the same policy as every other
/// action (`freeze_display` hands out screen pixels, so it counts as mutating: read-only
/// clients are refused).
pub const INTERNAL_ACTIONS: &[&str] = &[
    "freeze_display",
    "release_frame",
    "window_thumbnail",
    "discard_recording",
    "restart_recording",
    "skip_countdown",
    "rescan_library",
];

/// Whether `name` is in [`INTERNAL_ACTIONS`].
pub fn is_internal(name: &str) -> bool {
    INTERNAL_ACTIONS.contains(&name)
}

/// Never-tier operations (AGT-2.AC5): reading or exporting tokens and keys, changing privacy
/// of existing remote posts, disabling the recording indicator, removing default excluded
/// apps. These names are reserved: there is no action for them and calling one is
/// `method not found`. (The specs name no tool for them; the names here are the reserved
/// spellings used by the AC5 test.)
pub const NEVER_TIER: &[&str] = &[
    "get_oauth_token",
    "export_api_keys",
    "set_remote_privacy",
    "disable_recording_indicator",
    "remove_excluded_apps",
];

/// Looks up an exposed action. `None` for unknown and Never-tier names.
pub fn spec_for(name: &str) -> Option<&'static ActionSpec> {
    ACTIONS.iter().find(|s| s.name == name)
}

/// Tier of any name: `Some(Never)` for reserved Never-tier operations, `None` if unknown.
pub fn tier_of(name: &str) -> Option<Tier> {
    spec_for(name)
        .map(|s| s.tier)
        .or_else(|| NEVER_TIER.contains(&name).then_some(Tier::Never))
}

/// What `capture_screenshot` captures. Region rects are **physical px relative to the
/// display** (`rect: [x, y, w, h]`, CAP-1.AC2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum ScreenshotTarget {
    Display { display_id: u32 },
    Window { window_id: u32 },
    Region { display_id: u32, rect: [i64; 4] },
}

impl ScreenshotTarget {
    pub(crate) fn parse(v: &Value) -> Result<Self, CmError> {
        let obj = v
            .as_object()
            .ok_or_else(|| CmError::invalid_field("target", "must be an object"))?;
        for k in obj.keys() {
            if !matches!(k.as_str(), "display_id" | "window_id" | "rect") {
                return Err(CmError::invalid_field(
                    "target",
                    format!("unknown key `{k}`"),
                ));
            }
        }
        let id = |key: &str| -> Result<Option<u32>, CmError> {
            match obj.get(key) {
                None => Ok(None),
                Some(x) => x
                    .as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .map(Some)
                    .ok_or_else(|| {
                        CmError::invalid_field(
                            &format!("target.{key}"),
                            "must be a non-negative integer",
                        )
                    }),
            }
        };
        let (display, window) = (id("display_id")?, id("window_id")?);
        match (display, window, obj.get("rect")) {
            (Some(display_id), None, None) => Ok(Self::Display { display_id }),
            (None, Some(window_id), None) => Ok(Self::Window { window_id }),
            (Some(display_id), None, Some(r)) => {
                let out = parse_rect(r)?;
                Ok(Self::Region {
                    display_id,
                    rect: out,
                })
            }
            (None, None, _) => Err(CmError::invalid_field(
                "target",
                "needs `display_id`, `window_id`, or `display_id` with `rect`",
            )),
            _ => Err(CmError::invalid_field(
                "target",
                "use exactly one of `window_id`, `display_id`, or `display_id` with `rect`",
            )),
        }
    }

    /// The `cm-capture` target (region rects are taken as physical px).
    pub fn to_capture_target(self) -> CaptureTarget {
        match self {
            ScreenshotTarget::Display { display_id } => CaptureTarget::Display { display_id },
            ScreenshotTarget::Window { window_id } => CaptureTarget::Window { window_id },
            ScreenshotTarget::Region { display_id, rect } => CaptureTarget::Region {
                display_id,
                // Ranges were checked in `parse`.
                rect: RegionRect::Physical(PhysicalRect {
                    origin: PhysicalPoint {
                        x: rect[0] as i32,
                        y: rect[1] as i32,
                    },
                    size: PhysicalSize {
                        width: rect[2] as u32,
                        height: rect[3] as u32,
                    },
                }),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureScreenshotParams {
    pub target: ScreenshotTarget,
    /// 0..=30 s (CAP-5); validated when executed.
    pub delay_s: Option<f64>,
    pub copy_to_clipboard: bool,
    /// Bundle title; defaults to `Screenshot yyyy-MM-dd HH.mm`.
    pub title: Option<String>,
    pub request_id: Option<String>,
}

/// `capture_screenshot` from a frozen frame (see [`FreezeDisplayParams`]): the frame is
/// cropped, not captured again (CAP-1.AC2).
///
/// JSON: `target: { frame_id, rect? }`, `rect` as `[x, y, w, h]` in physical px relative to
/// the display (omitted: the whole frame, a display capture). `delay_s` is refused: the pixels
/// already exist.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureFromFrameParams {
    pub frame_id: String,
    pub rect: Option<[i64; 4]>,
    pub copy_to_clipboard: bool,
    pub title: Option<String>,
    pub request_id: Option<String>,
}

/// How `freeze_display` returns the pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FrameTransport {
    /// A raw RGBA8 file in the daemon's private temp directory (`path`).
    #[default]
    File,
    /// The same bytes, base64-encoded, in `data`.
    Inline,
}

/// `freeze_display { display_id, transport? }`: captures the display once into the frame
/// cache (no bundle, no `capture.created`). Returns `{ frame_id, display_id, width, height,
/// scale, format: "rgba8", stride, bytes, ttl_s, transport, path | data }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FreezeDisplayParams {
    pub display_id: u32,
    pub transport: FrameTransport,
}

/// `release_frame { frame_id }`: frees a frozen frame now. Returns `{ released }`, `false` if
/// it was already gone (releasing twice is not an error).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReleaseFrameParams {
    pub frame_id: String,
}

/// Default and clamp range of `window_thumbnail`'s `max_px`.
pub const THUMBNAIL_DEFAULT_PX: u32 = 360;
pub const THUMBNAIL_MIN_PX: u32 = 64;
pub const THUMBNAIL_MAX_PX: u32 = 1024;

/// `window_thumbnail { window_id, max_px? }`: one capture of the window, downscaled so the
/// longer edge is at most `max_px` (default 360, clamped to 64..=1024), as a PNG. Serves the
/// window picker; nothing is cached or stored. Returns `{ window_id, width, height, png_base64 }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WindowThumbnailParams {
    pub window_id: u32,
    pub max_px: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AnnotateParams {
    pub capture_id: String,
    pub ops: Vec<AnnotationOp>,
    /// Clear the existing ops first (still one undo step).
    pub replace: bool,
    /// Copy the **rendered** result (annotations and redactions applied) to the clipboard
    /// after the revision is saved (R1-1). Never the original.
    pub copy_to_clipboard: bool,
    /// Return the rendered PNG (base64) in the result; default true. The editor passes `false`:
    /// it already shows the image and would only decode and drop it.
    pub return_image: bool,
    pub request_id: Option<String>,
}

/// `rename_capture { capture_id, title, request_id? }`: sets the title and renames the bundle
/// folder to match (LIB-1.AC5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RenameCaptureParams {
    pub capture_id: String,
    pub title: String,
    pub request_id: Option<String>,
}

/// `suggest_redactions { capture_id, kinds? }` (ANN-2.AC4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SuggestRedactionsParams {
    pub capture_id: String,
    /// Kind names to look for (`email`, `token`, `ip_address`, `phone`); empty means all.
    pub kinds: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetCaptureParams {
    pub capture_id: String,
    /// Only the current revision is supported for now.
    pub revision: Option<u64>,
    /// Include the image (base64 PNG); default true.
    pub include_image: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetSettingsParams {
    /// `None` returns every setting.
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetSettingsParams {
    pub values: Map<String, Value>,
    pub request_id: Option<String>,
}

/// `undo` / `redo` input.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CaptureRefParams {
    pub capture_id: String,
    pub request_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListLibraryParams {
    pub query: Option<String>,
    /// `screenshot` or `recording`.
    pub kind: Option<String>,
    /// `uploaded`, `not_uploaded`, `published` or `not_published`.
    pub status: Option<String>,
    pub limit: usize,
    /// Opaque; the `next_cursor` of the previous page.
    pub cursor: Option<String>,
}

/// `get_job { job_id }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GetJobParams {
    pub job_id: String,
}

/// `list_jobs { state?, type?, limit? }`, newest first.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ListJobsParams {
    pub state: Option<cm_jobs::JobState>,
    #[serde(rename = "type")]
    pub job_type: Option<String>,
    pub limit: usize,
}

/// `cancel_job { job_id }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CancelJobParams {
    pub job_id: String,
    pub request_id: Option<String>,
}

/// `retry_job { job_id, request_id? }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RetryJobParams {
    pub job_id: String,
    pub request_id: Option<String>,
}

/// `export_video { capture_id, preset?, format?, width?, fps?, max_mb?, loudness_lufs?,
/// request_id? }`. `preset` defaults to `quick` (the interim export) and names a row of the preset
/// table otherwise (EXP-2 `youtube-1080p`, ... and EXP-3 `gif`, `webm`); `format: "gif"|"webm"`
/// is the same as that preset. Whether the id exists is checked against the table when the
/// request runs, because the table is data (`CAPTUREMANN_PLATFORM_RULES`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExportVideoParams {
    pub capture_id: String,
    pub preset: String,
    /// Integrated loudness target (AUD-4); `None` uses the preset's (-14 LUFS for the platform
    /// presets) or leaves the level alone.
    pub loudness_lufs: Option<f64>,
    /// GIF and WebM output width in px (EXP-3).
    pub width: Option<u32>,
    /// GIF and WebM frame rate (EXP-3).
    pub fps: Option<f64>,
    /// GIF and WebM "fit under N MB" (EXP-3).
    pub max_mb: Option<f64>,
    /// `burn`: burn the captions into the picture (fails without a transcript); `none`: leave
    /// them out; `None`: what the recording's `set_caption_options` says (AI-1).
    pub captions: Option<crate::export::CaptionChoice>,
    pub request_id: Option<String>,
}

/// Range of `export_video.loudness_lufs` (LUFS): quiet enough for speech, loud enough for music.
pub const LOUDNESS_LUFS_RANGE: (f64, f64) = (-40.0, -5.0);

/// `pause_recording`, `resume_recording`, `stop_recording` and the internal `discard_recording`,
/// `restart_recording`, `skip_countdown`: `{ recording_id?, request_id? }`. Without a
/// `recording_id` the action applies to the running recording.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordingRefParams {
    pub recording_id: Option<String>,
    pub request_id: Option<String>,
}

/// `set_mic_muted { muted, recording_id?, request_id? }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SetMicMutedParams {
    pub muted: bool,
    pub recording_id: Option<String>,
    pub request_id: Option<String>,
}

/// `get_recording_status { recording_id? }`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RecordingStatusParams {
    pub recording_id: Option<String>,
}

/// `move_library { path, mode?, request_id? }` (see [`crate::library_move`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MoveLibraryParams {
    pub path: String,
    pub mode: crate::library_move::MoveMode,
    pub request_id: Option<String>,
}

/// Confirm-tier actions whose engines live outside the daemon core (recording, publishing,
/// deleting). The daemon validates the envelope, runs the gate, then hands the action to
/// [`crate::ExternalActions`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ExternalParams {
    /// Target capture, when the action has one (`publish_video`, `delete_capture`).
    pub capture_id: Option<String>,
    pub request_id: Option<String>,
    /// Every other input field, passed through untouched.
    pub rest: Map<String, Value>,
}

/// A validated action request.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum Action {
    ListTargets,
    /// `list_targets { windows: false }`: displays only, no window enumeration.
    ListDisplays,
    CaptureScreenshot(CaptureScreenshotParams),
    /// `capture_screenshot` with `target: { frame_id }` (same method name and tier).
    CaptureFromFrame(CaptureFromFrameParams),
    FreezeDisplay(FreezeDisplayParams),
    ReleaseFrame(ReleaseFrameParams),
    WindowThumbnail(WindowThumbnailParams),
    GetJob(GetJobParams),
    ListJobs(ListJobsParams),
    CancelJob(CancelJobParams),
    RetryJob(RetryJobParams),
    ExportVideo(ExportVideoParams),
    Annotate(AnnotateParams),
    GetCapture(GetCaptureParams),
    SuggestRedactions(SuggestRedactionsParams),
    GetSettings(GetSettingsParams),
    SetSettings(SetSettingsParams),
    Undo(CaptureRefParams),
    Redo(CaptureRefParams),
    ListLibrary(ListLibraryParams),
    RenameCapture(RenameCaptureParams),
    /// `trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`,
    /// `set_audio_mix`, `set_camera_layout`, `set_zooms`, `set_cursor_style`, `set_reframe`.
    Edit(crate::edit_ops::EditParams),
    /// `list_models`, `download_model`, `import_model`, `select_model`, `remove_model`,
    /// `transcribe`, `get_transcript`, `set_captions`, `replace_in_captions`, `export_captions`,
    /// `suggest_zooms`, `suggest_cuts` (see [`crate::ai`]).
    Ai(crate::ai::AiParams),
    /// `set_caption_options` (see [`crate::caption_options`]).
    CaptionOptions(crate::caption_options::CaptionOptionsParams),
    /// `list_destinations` (see [`crate::storage_ops`]).
    ListDestinations,
    /// `upload` (see [`crate::storage_ops`]).
    Upload(crate::storage_ops::UploadParams),
    /// `list_publish_accounts` (see [`crate::publish_ops`]).
    ListPublishAccounts,
    /// Internal: rebuild the index from the bundle folders.
    RescanLibrary,
    /// `start_recording`. The payload stays pass-through JSON for compatibility; it was validated
    /// with [`crate::recording::StartParams::parse`] when the action was parsed.
    StartRecording(ExternalParams),
    PauseRecording(RecordingRefParams),
    ResumeRecording(RecordingRefParams),
    SetMicMuted(SetMicMutedParams),
    StopRecording(RecordingRefParams),
    GetRecordingStatus(RecordingStatusParams),
    ListDevices,
    DiscardRecording(RecordingRefParams),
    RestartRecording(RecordingRefParams),
    SkipCountdown(RecordingRefParams),
    /// `run_fps_selftest { target?, size?, codec? }`; validated with
    /// [`crate::recording::SelftestParams::parse`] when the action was parsed.
    RunFpsSelftest(ExternalParams),
    PublishVideo(ExternalParams),
    DeleteCapture(ExternalParams),
    /// `move_library` (see [`crate::library_move`]).
    MoveLibrary(MoveLibraryParams),
}

/// Largest accepted `request_id`.
pub const MAX_REQUEST_ID_LEN: usize = 256;
/// Default and maximum page size of `list_library`.
pub const DEFAULT_LIST_LIMIT: usize = 50;
pub const MAX_LIST_LIMIT: usize = 200;

pub(crate) struct Fields<'a> {
    pub(crate) obj: &'a Map<String, Value>,
}

impl<'a> Fields<'a> {
    pub(crate) fn new(params: &'a Value, empty: &'a Map<String, Value>) -> Result<Self, CmError> {
        match params {
            Value::Null => Ok(Fields { obj: empty }),
            Value::Object(m) => Ok(Fields { obj: m }),
            _ => Err(CmError::invalid_field("params", "must be an object")),
        }
    }

    /// Rejects keys outside `allowed` (typos would otherwise be silently ignored).
    pub(crate) fn only(&self, allowed: &[&str]) -> Result<(), CmError> {
        for k in self.obj.keys() {
            if !allowed.contains(&k.as_str()) {
                return Err(CmError::invalid_field(k, "unknown parameter"));
            }
        }
        Ok(())
    }

    pub(crate) fn str_opt(&self, key: &str) -> Result<Option<String>, CmError> {
        match self.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(CmError::invalid_field(key, "must be a string")),
        }
    }

    pub(crate) fn str_req(&self, key: &str) -> Result<String, CmError> {
        match self.str_opt(key)? {
            Some(s) if !s.trim().is_empty() => Ok(s),
            Some(_) => Err(CmError::invalid_field(key, "must not be empty")),
            None => Err(CmError::invalid_field(key, "is required")),
        }
    }

    pub(crate) fn bool_or(&self, key: &str, default: bool) -> Result<bool, CmError> {
        match self.obj.get(key) {
            None | Some(Value::Null) => Ok(default),
            Some(Value::Bool(b)) => Ok(*b),
            Some(_) => Err(CmError::invalid_field(key, "must be a boolean")),
        }
    }

    pub(crate) fn request_id(&self) -> Result<Option<String>, CmError> {
        match self.str_opt("request_id")? {
            Some(s) if s.is_empty() || s.len() > MAX_REQUEST_ID_LEN => Err(CmError::invalid_field(
                "request_id",
                format!("must be 1..={MAX_REQUEST_ID_LEN} characters"),
            )),
            other => Ok(other),
        }
    }
}

/// The preset id of an `export_video`: `preset`, or the one a `format` of `gif` or `webm` stands
/// for; `quick` when neither is given.
fn parse_export_preset(f: &Fields<'_>) -> Result<String, CmError> {
    let preset = f.str_opt("preset")?;
    if let Some(p) = &preset {
        let ok = !p.is_empty()
            && p.len() <= 40
            && p.chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !ok {
            return Err(CmError::invalid_field(
                "preset",
                "must be a preset id such as `youtube-1080p`, `vertical-1080`, `gif` or `webm`",
            ));
        }
    }
    let format = f.str_opt("format")?;
    match (format.as_deref(), preset) {
        (None, p) => Ok(p.unwrap_or_else(|| "quick".to_owned())),
        (Some("mp4"), Some(p)) if p == "gif" || p == "webm" => Err(CmError::invalid_field(
            "format",
            format!("`mp4` does not match the preset `{p}`"),
        )),
        (Some("mp4"), p) => Ok(p.unwrap_or_else(|| "quick".to_owned())),
        (Some(fmt @ ("gif" | "webm")), None) => Ok(fmt.to_owned()),
        (Some(fmt @ ("gif" | "webm")), Some(p)) if p == fmt => Ok(p),
        (Some(fmt @ ("gif" | "webm")), Some(p)) => Err(CmError::invalid_field(
            "format",
            format!("`{fmt}` does not match the preset `{p}`"),
        )),
        (Some(_), _) => Err(CmError::invalid_field(
            "format",
            "must be `mp4`, `gif` or `webm`",
        )),
    }
}

/// `width`, `fps` and `max_mb` of an `export_video` (EXP-3). Which presets take them is checked
/// where the preset is resolved.
#[allow(clippy::type_complexity)]
fn parse_export_options(
    f: &Fields<'_>,
) -> Result<(Option<u32>, Option<f64>, Option<f64>), CmError> {
    let number = |key: &str| -> Result<Option<f64>, CmError> {
        match f.obj.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_f64()
                .filter(|n| n.is_finite() && *n > 0.0)
                .map(Some)
                .ok_or_else(|| CmError::invalid_field(key, "must be a positive number")),
        }
    };
    let width = match number("width")? {
        None => None,
        Some(w) if w.fract() == 0.0 && (16.0..=7680.0).contains(&w) => Some(w as u32),
        Some(_) => {
            return Err(CmError::invalid_field(
                "width",
                "must be a whole number of pixels from 16 to 7680",
            ))
        }
    };
    let fps = number("fps")?;
    if fps.is_some_and(|f| f > 60.0) {
        return Err(CmError::invalid_field("fps", "must be from 1 to 60"));
    }
    Ok((width, fps, number("max_mb")?))
}

fn recording_id(f: &Fields<'_>) -> Result<Option<String>, CmError> {
    match f.str_opt("recording_id")? {
        Some(s) if s.trim().is_empty() => {
            Err(CmError::invalid_field("recording_id", "must not be empty"))
        }
        other => Ok(other),
    }
}

/// `capture_screenshot` whose `target` is `{ frame_id, rect? }`.
fn parse_from_frame(f: &Fields<'_>, target: &Value) -> Result<CaptureFromFrameParams, CmError> {
    let obj = target
        .as_object()
        .ok_or_else(|| CmError::invalid_field("target", "must be an object"))?;
    for k in obj.keys() {
        if !matches!(k.as_str(), "frame_id" | "rect") {
            return Err(CmError::invalid_field(
                "target",
                format!("unknown key `{k}` (with `frame_id` only `rect` is allowed)"),
            ));
        }
    }
    let frame_id = match obj.get("frame_id") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.clone(),
        _ => {
            return Err(CmError::invalid_field(
                "target.frame_id",
                "must be a non-empty string",
            ))
        }
    };
    let rect = match obj.get("rect") {
        None | Some(Value::Null) => None,
        Some(r) => Some(parse_rect(r)?),
    };
    if f.obj.get("delay_s").is_some_and(|v| !v.is_null()) {
        return Err(CmError::invalid_field(
            "delay_s",
            "is not allowed with `frame_id` (the frame is already captured)",
        ));
    }
    Ok(CaptureFromFrameParams {
        frame_id,
        rect,
        copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
        title: f.str_opt("title")?,
        request_id: f.request_id()?,
    })
}

/// `[x, y, w, h]`, whole physical px relative to the display, positive size.
fn parse_rect(r: &Value) -> Result<[i64; 4], CmError> {
    let bad = || {
        CmError::invalid_field(
            "target.rect",
            "must be [x, y, w, h] in whole physical px relative to the display",
        )
    };
    let arr = r.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
    let mut out = [0i64; 4];
    for (slot, n) in out.iter_mut().zip(arr) {
        *slot = n.as_i64().ok_or_else(bad)?;
    }
    if out[2] <= 0 || out[3] <= 0 {
        return Err(CmError::invalid_field(
            "target.rect",
            "width and height must be positive",
        ));
    }
    if out[0] < i64::from(i32::MIN)
        || out[0] > i64::from(i32::MAX)
        || out[1] < i64::from(i32::MIN)
        || out[1] > i64::from(i32::MAX)
        || out[2] > i64::from(u32::MAX)
        || out[3] > i64::from(u32::MAX)
    {
        return Err(bad());
    }
    Ok(out)
}

impl Action {
    /// Validates `params` for the exposed action `method`.
    ///
    /// Unknown and Never-tier names are not handled here (the caller reports
    /// `method not found`); this returns `invalid_input` only for known actions.
    pub fn parse(method: &str, params: &Value) -> Result<Action, CmError> {
        let empty = Map::new();
        let f = Fields::new(params, &empty)?;
        Ok(match method {
            "list_targets" => {
                f.only(&["windows"])?;
                if f.bool_or("windows", true)? {
                    Action::ListTargets
                } else {
                    Action::ListDisplays
                }
            }
            "capture_screenshot" => {
                f.only(&[
                    "target",
                    "delay_s",
                    "copy_to_clipboard",
                    "title",
                    "request_id",
                ])?;
                let target_value = f
                    .obj
                    .get("target")
                    .ok_or_else(|| CmError::invalid_field("target", "is required"))?;
                if target_value.get("frame_id").is_some() {
                    return Ok(Action::CaptureFromFrame(parse_from_frame(
                        &f,
                        target_value,
                    )?));
                }
                let target = ScreenshotTarget::parse(target_value)?;
                let delay_s = match f.obj.get("delay_s") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(
                        v.as_f64()
                            .ok_or_else(|| CmError::invalid_field("delay_s", "must be a number"))?,
                    ),
                };
                Action::CaptureScreenshot(CaptureScreenshotParams {
                    target,
                    delay_s,
                    copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
                    title: f.str_opt("title")?,
                    request_id: f.request_id()?,
                })
            }
            "freeze_display" => {
                f.only(&["display_id", "transport"])?;
                let display_id = f
                    .obj
                    .get("display_id")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| {
                        CmError::invalid_field("display_id", "must be a non-negative integer")
                    })?;
                let transport = match f.str_opt("transport")?.as_deref() {
                    None | Some("file") => FrameTransport::File,
                    Some("inline") => FrameTransport::Inline,
                    Some(_) => {
                        return Err(CmError::invalid_field(
                            "transport",
                            "must be `file` or `inline`",
                        ))
                    }
                };
                Action::FreezeDisplay(FreezeDisplayParams {
                    display_id,
                    transport,
                })
            }
            "release_frame" => {
                f.only(&["frame_id"])?;
                Action::ReleaseFrame(ReleaseFrameParams {
                    frame_id: f.str_req("frame_id")?,
                })
            }
            "window_thumbnail" => {
                f.only(&["window_id", "max_px"])?;
                let window_id = f
                    .obj
                    .get("window_id")
                    .and_then(Value::as_u64)
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| {
                        CmError::invalid_field("window_id", "must be a non-negative integer")
                    })?;
                let max_px = match f.obj.get("max_px") {
                    None | Some(Value::Null) => THUMBNAIL_DEFAULT_PX,
                    Some(v) => v
                        .as_u64()
                        .ok_or_else(|| {
                            CmError::invalid_field("max_px", "must be a non-negative integer")
                        })?
                        .clamp(u64::from(THUMBNAIL_MIN_PX), u64::from(THUMBNAIL_MAX_PX))
                        as u32,
                };
                Action::WindowThumbnail(WindowThumbnailParams { window_id, max_px })
            }
            "get_job" => {
                f.only(&["job_id"])?;
                Action::GetJob(GetJobParams {
                    job_id: f.str_req("job_id")?,
                })
            }
            "list_jobs" => {
                f.only(&["state", "type", "limit"])?;
                let state = match f.str_opt("state")? {
                    None => None,
                    Some(s) => Some(cm_jobs::JobState::parse(&s).ok_or_else(|| {
                        CmError::invalid_field(
                            "state",
                            "must be queued, running, paused, failed, done or cancelled",
                        )
                    })?),
                };
                let limit = match f.obj.get("limit") {
                    None | Some(Value::Null) => DEFAULT_LIST_LIMIT,
                    Some(v) => v
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|n| (1..=MAX_LIST_LIMIT).contains(n))
                        .ok_or_else(|| {
                            CmError::invalid_field(
                                "limit",
                                format!("must be an integer in 1..={MAX_LIST_LIMIT}"),
                            )
                        })?,
                };
                Action::ListJobs(ListJobsParams {
                    state,
                    job_type: f.str_opt("type")?,
                    limit,
                })
            }
            "cancel_job" => {
                f.only(&["job_id", "request_id"])?;
                Action::CancelJob(CancelJobParams {
                    job_id: f.str_req("job_id")?,
                    request_id: f.request_id()?,
                })
            }
            "retry_job" => {
                f.only(&["job_id", "request_id"])?;
                Action::RetryJob(RetryJobParams {
                    job_id: f.str_req("job_id")?,
                    request_id: f.request_id()?,
                })
            }
            "export_video" => {
                // The spec's full input (EXP-1..3). Only `capture_id` and the preset `quick` work
                // for now; the rest is refused by name instead of being silently ignored.
                const LATER: &[&str] = &["size", "codec", "quality", "out_dir"];
                let mut allowed = vec![
                    "capture_id",
                    "preset",
                    "format",
                    "width",
                    "fps",
                    "max_mb",
                    "loudness_lufs",
                    "captions",
                    "request_id",
                ];
                allowed.extend_from_slice(LATER);
                f.only(&allowed)?;
                if let Some(k) = LATER
                    .iter()
                    .find(|k| f.obj.get(**k).is_some_and(|v| !v.is_null()))
                {
                    return Err(CmError::invalid_field(
                        k,
                        "is not available yet: export_video takes a `preset` (or `format` gif or webm with `width`, `fps`, `max_mb`)",
                    ));
                }
                let preset = parse_export_preset(&f)?;
                let (width, fps, max_mb) = parse_export_options(&f)?;
                let loudness_lufs = match f.obj.get("loudness_lufs") {
                    None | Some(Value::Null) => None,
                    Some(v) => {
                        let n = v.as_f64().filter(|n| {
                            n.is_finite()
                                && (LOUDNESS_LUFS_RANGE.0..=LOUDNESS_LUFS_RANGE.1).contains(n)
                        });
                        Some(n.ok_or_else(|| {
                            CmError::invalid_field(
                                "loudness_lufs",
                                format!(
                                    "must be a number from {} to {} LUFS",
                                    LOUDNESS_LUFS_RANGE.0, LOUDNESS_LUFS_RANGE.1
                                ),
                            )
                        })?)
                    }
                };
                let captions = match f.str_opt("captions")? {
                    None => None,
                    Some(s) => Some(crate::export::CaptionChoice::parse(&s).ok_or_else(|| {
                        CmError::invalid_field(
                            "captions",
                            "must be `burn` (burn the captions into the video) or `none`",
                        )
                    })?),
                };
                Action::ExportVideo(ExportVideoParams {
                    capture_id: f.str_req("capture_id")?,
                    preset,
                    loudness_lufs,
                    width,
                    fps,
                    max_mb,
                    captions,
                    request_id: f.request_id()?,
                })
            }
            "annotate" => {
                f.only(&[
                    "capture_id",
                    "ops",
                    "replace",
                    "copy_to_clipboard",
                    "return_image",
                    "request_id",
                ])?;
                let ops = cm_core::parse_ops(
                    f.obj
                        .get("ops")
                        .ok_or_else(|| CmError::invalid_field("ops", "is required"))?,
                )?;
                let replace = f.bool_or("replace", false)?;
                // `replace: true` with no ops clears all annotations ("Revert to original",
                // ANN-3); without `replace` an empty list is meaningless.
                if ops.is_empty() && !replace {
                    return Err(CmError::invalid_field("ops", "must not be empty"));
                }
                Action::Annotate(AnnotateParams {
                    capture_id: f.str_req("capture_id")?,
                    ops,
                    replace,
                    copy_to_clipboard: f.bool_or("copy_to_clipboard", false)?,
                    return_image: f.bool_or("return_image", true)?,
                    request_id: f.request_id()?,
                })
            }
            "rename_capture" => {
                f.only(&["capture_id", "title", "request_id"])?;
                Action::RenameCapture(RenameCaptureParams {
                    capture_id: f.str_req("capture_id")?,
                    title: f.str_req("title")?,
                    request_id: f.request_id()?,
                })
            }
            m if crate::edit_ops::EDIT_METHODS.contains(&m) => {
                return crate::edit_ops::parse(m, &f);
            }
            "list_destinations" => {
                f.only(&[])?;
                Action::ListDestinations
            }
            "upload" => return crate::storage_ops::parse_upload(&f),
            "list_publish_accounts" => {
                f.only(&[])?;
                Action::ListPublishAccounts
            }
            m if crate::ai::is_ai_method(m) => {
                return crate::ai::parse(m, &f);
            }
            crate::caption_options::METHOD => {
                return crate::caption_options::parse(&f);
            }
            "rescan_library" => {
                f.only(&[])?;
                Action::RescanLibrary
            }
            "move_library" => {
                f.only(&["path", "mode", "request_id"])?;
                let mode = match f.str_opt("mode")? {
                    None => crate::library_move::MoveMode::Move,
                    Some(m) => crate::library_move::MoveMode::parse(&m).ok_or_else(|| {
                        CmError::invalid_field("mode", "must be `move`, `fresh` or `use_existing`")
                    })?,
                };
                Action::MoveLibrary(MoveLibraryParams {
                    path: f.str_req("path")?,
                    mode,
                    request_id: f.request_id()?,
                })
            }
            "get_capture" => {
                f.only(&["capture_id", "revision", "include_image"])?;
                let revision = match f.obj.get("revision") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_u64().ok_or_else(|| {
                        CmError::invalid_field("revision", "must be a non-negative integer")
                    })?),
                };
                Action::GetCapture(GetCaptureParams {
                    capture_id: f.str_req("capture_id")?,
                    revision,
                    include_image: f.bool_or("include_image", true)?,
                })
            }
            "suggest_redactions" => {
                f.only(&["capture_id", "kinds"])?;
                let kinds = match f.obj.get("kinds") {
                    None | Some(Value::Null) => Vec::new(),
                    Some(Value::Array(a)) => {
                        let mut out = Vec::new();
                        for (i, v) in a.iter().enumerate() {
                            let key = format!("kinds[{i}]");
                            let name = v
                                .as_str()
                                .ok_or_else(|| CmError::invalid_field(&key, "must be a string"))?;
                            if cm_render::ocr::RedactionKind::parse(name).is_none() {
                                return Err(CmError::invalid_field(
                                    &key,
                                    "must be one of email, token, ip_address, phone",
                                ));
                            }
                            if !out.iter().any(|k| k == name) {
                                out.push(name.to_owned());
                            }
                        }
                        out
                    }
                    Some(_) => return Err(CmError::invalid_field("kinds", "must be an array")),
                };
                Action::SuggestRedactions(SuggestRedactionsParams {
                    capture_id: f.str_req("capture_id")?,
                    kinds,
                })
            }
            "get_settings" => {
                f.only(&["keys"])?;
                let keys = match f.obj.get("keys") {
                    None | Some(Value::Null) => None,
                    Some(Value::Array(a)) => Some(
                        a.iter()
                            .map(|k| {
                                k.as_str().map(str::to_owned).ok_or_else(|| {
                                    CmError::invalid_field("keys", "must be an array of strings")
                                })
                            })
                            .collect::<Result<Vec<_>, _>>()?,
                    ),
                    Some(_) => {
                        return Err(CmError::invalid_field(
                            "keys",
                            "must be an array of strings",
                        ))
                    }
                };
                Action::GetSettings(GetSettingsParams { keys })
            }
            "set_settings" => {
                f.only(&["values", "request_id"])?;
                let values = match f.obj.get("values") {
                    Some(Value::Object(m)) if !m.is_empty() => m.clone(),
                    Some(Value::Object(_)) => {
                        return Err(CmError::invalid_field("values", "must not be empty"))
                    }
                    Some(_) => return Err(CmError::invalid_field("values", "must be an object")),
                    None => return Err(CmError::invalid_field("values", "is required")),
                };
                Action::SetSettings(SetSettingsParams {
                    values,
                    request_id: f.request_id()?,
                })
            }
            "undo" | "redo" => {
                f.only(&["capture_id", "request_id"])?;
                let p = CaptureRefParams {
                    capture_id: f.str_req("capture_id")?,
                    request_id: f.request_id()?,
                };
                if method == "undo" {
                    Action::Undo(p)
                } else {
                    Action::Redo(p)
                }
            }
            "list_library" => {
                f.only(&["query", "kind", "status", "limit", "cursor"])?;
                let status = f.str_opt("status")?;
                if let Some(s) = &status {
                    if cm_library::index::StatusFilter::parse(s).is_none() {
                        return Err(CmError::invalid_field(
                            "status",
                            "must be `uploaded`, `not_uploaded`, `published` or `not_published`",
                        ));
                    }
                }
                let kind = f.str_opt("kind")?;
                if let Some(k) = &kind {
                    if !matches!(k.as_str(), "screenshot" | "recording") {
                        return Err(CmError::invalid_field(
                            "kind",
                            "must be `screenshot` or `recording`",
                        ));
                    }
                }
                let limit = match f.obj.get("limit") {
                    None | Some(Value::Null) => DEFAULT_LIST_LIMIT,
                    Some(v) => v
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|n| (1..=MAX_LIST_LIMIT).contains(n))
                        .ok_or_else(|| {
                            CmError::invalid_field(
                                "limit",
                                format!("must be an integer in 1..={MAX_LIST_LIMIT}"),
                            )
                        })?,
                };
                Action::ListLibrary(ListLibraryParams {
                    query: f.str_opt("query")?,
                    kind,
                    status,
                    limit,
                    cursor: f.str_opt("cursor")?,
                })
            }
            "pause_recording" | "resume_recording" | "stop_recording" | "discard_recording"
            | "restart_recording" | "skip_countdown" => {
                f.only(&["recording_id", "request_id"])?;
                let p = RecordingRefParams {
                    recording_id: recording_id(&f)?,
                    request_id: f.request_id()?,
                };
                match method {
                    "pause_recording" => Action::PauseRecording(p),
                    "resume_recording" => Action::ResumeRecording(p),
                    "stop_recording" => Action::StopRecording(p),
                    "discard_recording" => Action::DiscardRecording(p),
                    "restart_recording" => Action::RestartRecording(p),
                    _ => Action::SkipCountdown(p),
                }
            }
            "set_mic_muted" => {
                f.only(&["muted", "recording_id", "request_id"])?;
                let muted = match f.obj.get("muted") {
                    Some(Value::Bool(b)) => *b,
                    None | Some(Value::Null) => {
                        return Err(CmError::invalid_field("muted", "is required"))
                    }
                    Some(_) => return Err(CmError::invalid_field("muted", "must be a boolean")),
                };
                Action::SetMicMuted(SetMicMutedParams {
                    muted,
                    recording_id: recording_id(&f)?,
                    request_id: f.request_id()?,
                })
            }
            "get_recording_status" => {
                f.only(&["recording_id"])?;
                Action::GetRecordingStatus(RecordingStatusParams {
                    recording_id: recording_id(&f)?,
                })
            }
            "list_devices" => {
                f.only(&[])?;
                Action::ListDevices
            }
            "run_fps_selftest" => {
                let request_id = f.request_id()?;
                let rest: Map<String, Value> = f
                    .obj
                    .iter()
                    .filter(|(k, _)| k.as_str() != "request_id")
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                // Typed validation: bad input is refused before any dialog opens.
                crate::recording::SelftestParams::parse(&rest)?;
                Action::RunFpsSelftest(ExternalParams {
                    capture_id: None,
                    request_id,
                    rest,
                })
            }
            "start_recording" | "publish_video" | "delete_capture" => {
                let needs_capture = method != "start_recording";
                let capture_id = if needs_capture {
                    Some(f.str_req("capture_id")?)
                } else {
                    f.str_opt("capture_id")?
                };
                let request_id = f.request_id()?;
                if method == "delete_capture" {
                    // Typed validation: bad input is refused before any dialog opens.
                    f.only(&["capture_id", "also_delete_remote", "request_id"])?;
                    f.bool_or("also_delete_remote", false)?;
                }
                let rest = f
                    .obj
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_str(), "capture_id" | "request_id"))
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                if method == "start_recording" {
                    if capture_id.is_some() {
                        return Err(CmError::invalid_field("capture_id", "unknown parameter"));
                    }
                    // Typed validation: bad input is refused before any dialog opens.
                    crate::recording::StartParams::parse(&rest)?;
                }
                let p = ExternalParams {
                    capture_id,
                    request_id,
                    rest,
                };
                if method == "publish_video" {
                    // Typed validation: bad input is refused before any dialog opens.
                    crate::publish_ops::PublishVideoParams::parse(&p)?;
                }
                match method {
                    "start_recording" => Action::StartRecording(p),
                    "publish_video" => Action::PublishVideo(p),
                    _ => Action::DeleteCapture(p),
                }
            }
            other => {
                return Err(CmError::invalid_input(format!("unknown action `{other}`")));
            }
        })
    }

    /// The method / tool name.
    pub fn method(&self) -> &'static str {
        match self {
            Action::ListTargets | Action::ListDisplays => "list_targets",
            Action::CaptureScreenshot(_) | Action::CaptureFromFrame(_) => "capture_screenshot",
            Action::FreezeDisplay(_) => "freeze_display",
            Action::ReleaseFrame(_) => "release_frame",
            Action::WindowThumbnail(_) => "window_thumbnail",
            Action::GetJob(_) => "get_job",
            Action::ListJobs(_) => "list_jobs",
            Action::CancelJob(_) => "cancel_job",
            Action::RetryJob(_) => "retry_job",
            Action::ExportVideo(_) => "export_video",
            Action::Annotate(_) => "annotate",
            Action::GetCapture(_) => "get_capture",
            Action::SuggestRedactions(_) => "suggest_redactions",
            Action::GetSettings(_) => "get_settings",
            Action::SetSettings(_) => "set_settings",
            Action::Undo(_) => "undo",
            Action::Redo(_) => "redo",
            Action::ListLibrary(_) => "list_library",
            Action::RenameCapture(_) => "rename_capture",
            Action::Edit(p) => p.op.method(),
            Action::Ai(p) => p.op.method(),
            Action::CaptionOptions(_) => crate::caption_options::METHOD,
            Action::ListDestinations => "list_destinations",
            Action::Upload(_) => "upload",
            Action::ListPublishAccounts => "list_publish_accounts",
            Action::RescanLibrary => "rescan_library",
            Action::StartRecording(_) => "start_recording",
            Action::PauseRecording(_) => "pause_recording",
            Action::ResumeRecording(_) => "resume_recording",
            Action::SetMicMuted(_) => "set_mic_muted",
            Action::StopRecording(_) => "stop_recording",
            Action::GetRecordingStatus(_) => "get_recording_status",
            Action::ListDevices => "list_devices",
            Action::DiscardRecording(_) => "discard_recording",
            Action::RestartRecording(_) => "restart_recording",
            Action::SkipCountdown(_) => "skip_countdown",
            Action::RunFpsSelftest(_) => "run_fps_selftest",
            Action::PublishVideo(_) => "publish_video",
            Action::DeleteCapture(_) => "delete_capture",
            Action::MoveLibrary(_) => "move_library",
        }
    }

    /// The registry row (always present: [`ACTIONS`] is tested to cover every variant).
    pub fn spec(&self) -> ActionSpec {
        spec_for(self.method()).copied().unwrap_or(ActionSpec {
            name: self.method(),
            tier: Tier::Confirm,
            mutating: true,
        })
    }

    pub fn request_id(&self) -> Option<&str> {
        match self {
            Action::CaptureScreenshot(p) => p.request_id.as_deref(),
            Action::CaptureFromFrame(p) => p.request_id.as_deref(),
            Action::Annotate(p) => p.request_id.as_deref(),
            Action::SetSettings(p) => p.request_id.as_deref(),
            Action::Undo(p) | Action::Redo(p) => p.request_id.as_deref(),
            Action::RenameCapture(p) => p.request_id.as_deref(),
            Action::Edit(p) => p.request_id.as_deref(),
            Action::Ai(p) => p.request_id.as_deref(),
            Action::Upload(p) => p.request_id.as_deref(),
            Action::CaptionOptions(p) => p.request_id.as_deref(),
            Action::CancelJob(p) => p.request_id.as_deref(),
            Action::RetryJob(p) => p.request_id.as_deref(),
            Action::ExportVideo(p) => p.request_id.as_deref(),
            Action::SetMicMuted(p) => p.request_id.as_deref(),
            Action::MoveLibrary(p) => p.request_id.as_deref(),
            Action::PauseRecording(p)
            | Action::ResumeRecording(p)
            | Action::StopRecording(p)
            | Action::DiscardRecording(p)
            | Action::RestartRecording(p)
            | Action::SkipCountdown(p) => p.request_id.as_deref(),
            Action::StartRecording(p)
            | Action::RunFpsSelftest(p)
            | Action::PublishVideo(p)
            | Action::DeleteCapture(p) => p.request_id.as_deref(),
            _ => None,
        }
    }

    /// The capture this action edits; used to serialize actions per capture (AGT-3).
    pub fn capture_id(&self) -> Option<&str> {
        match self {
            Action::Annotate(p) => Some(&p.capture_id),
            Action::Undo(p) | Action::Redo(p) => Some(&p.capture_id),
            Action::RenameCapture(p) => Some(&p.capture_id),
            Action::Edit(p) => Some(&p.capture_id),
            Action::Ai(p) => p.op.capture_id(),
            Action::Upload(p) => p.capture_id.as_deref(),
            Action::CaptionOptions(p) => Some(&p.capture_id),
            Action::ExportVideo(p) => Some(&p.capture_id),
            Action::PublishVideo(p) | Action::DeleteCapture(p) => p.capture_id.as_deref(),
            _ => None,
        }
    }

    /// One sentence for the confirmation dialog and the activity log.
    pub fn summary(&self) -> String {
        match self {
            Action::StartRecording(_) => "Start a screen recording".to_owned(),
            Action::RunFpsSelftest(_) => {
                "Record the screen for about 10 s to test 60 fps (nothing is saved)".to_owned()
            }
            Action::DiscardRecording(_) => {
                "Discard the current recording and delete its files".to_owned()
            }
            Action::RestartRecording(_) => {
                "Discard the current take and start over with the same settings".to_owned()
            }
            Action::PublishVideo(p) => {
                let platform = p
                    .rest
                    .get("platform")
                    .and_then(Value::as_str)
                    .unwrap_or("a platform");
                format!(
                    "Publish capture {} to {platform}",
                    p.capture_id.as_deref().unwrap_or("?")
                )
            }
            Action::DeleteCapture(p) => {
                format!("Delete capture {}", p.capture_id.as_deref().unwrap_or("?"))
            }
            Action::RenameCapture(p) => format!("Rename {} to \"{}\"", p.capture_id, p.title),
            Action::MoveLibrary(p) => match p.mode {
                crate::library_move::MoveMode::Move => {
                    format!("Move the library to {}", p.path)
                }
                crate::library_move::MoveMode::Fresh => {
                    format!("Start a new empty library at {}", p.path)
                }
                crate::library_move::MoveMode::UseExisting => {
                    format!("Switch to the existing library at {}", p.path)
                }
            },
            Action::Edit(p) => crate::edit_ops::summary(p),
            Action::Ai(p) => crate::ai::summary(p),
            Action::ListDestinations => "List the destinations".to_owned(),
            Action::ListPublishAccounts => "List the publish accounts".to_owned(),
            Action::Upload(p) => crate::storage_ops::summary(p),
            Action::CaptionOptions(p) => crate::caption_options::summary(p),
            Action::ExportVideo(p) => match p.loudness_lufs {
                Some(l) => format!("Export {} as `{}` at {l} LUFS", p.capture_id, p.preset),
                None => format!("Export {} as `{}`", p.capture_id, p.preset),
            },
            other => format!("Run {}", other.method()),
        }
    }

    /// The typed parameters of the confirmation for an action that has no plan of its own
    /// (P3): what [`Action::summary`] names, so a client can word the sentence itself.
    pub fn confirm_params(&self) -> Value {
        match self {
            Action::PublishVideo(p) => json!({
                "capture_id": p.capture_id,
                "platform": p.rest.get("platform").and_then(Value::as_str),
            }),
            Action::DeleteCapture(p) => json!({ "capture_id": p.capture_id }),
            Action::MoveLibrary(p) => json!({ "to": p.path, "mode": p.mode.as_str() }),
            Action::RenameCapture(p) => json!({ "capture_id": p.capture_id, "title": p.title }),
            Action::Ai(p) => crate::ai::confirm_params(p),
            Action::ExportVideo(p) => json!({
                "capture_id": p.capture_id, "preset": p.preset, "loudness_lufs": p.loudness_lufs,
            }),
            _ => json!({}),
        }
    }

    /// The input as shown in the dialog's details (`request_id` left out).
    pub fn details(&self) -> Value {
        let mut v = serde_json::to_value(self)
            .ok()
            .and_then(|mut v| v.get_mut("params").map(Value::take))
            .unwrap_or(Value::Null);
        if let Some(o) = v.as_object_mut() {
            o.remove("request_id");
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::BTreeSet;

    fn samples() -> Vec<(&'static str, Value)> {
        vec![
            ("list_targets", json!({})),
            ("capture_screenshot", json!({"target": {"display_id": 1}})),
            (
                "annotate",
                json!({"capture_id": "shot_1", "ops": [{"op":"rect","rect":[1,2,3,4]}]}),
            ),
            ("get_capture", json!({"capture_id": "shot_1"})),
            ("suggest_redactions", json!({"capture_id": "shot_1"})),
            (
                "suggest_redactions",
                json!({"capture_id": "shot_1", "kinds": ["email", "token"]}),
            ),
            ("get_settings", json!({})),
            ("set_settings", json!({"values": {"recording.fps": 60}})),
            ("undo", json!({"capture_id": "shot_1"})),
            ("redo", json!({"capture_id": "shot_1"})),
            ("list_library", json!({})),
            (
                "rename_capture",
                json!({"capture_id": "shot_1", "title": "New", "request_id": "r"}),
            ),
            ("rescan_library", json!({})),
            (
                "trim",
                json!({"capture_id": "rec_1", "start": 2, "end": 58}),
            ),
            (
                "cut_segments",
                json!({"capture_id": "rec_1", "ranges": [[10, 15]]}),
            ),
            (
                "restore_segments",
                json!({"capture_id": "rec_1", "ranges": [[10, 15]]}),
            ),
            ("split", json!({"capture_id": "rec_1", "at": 30})),
            (
                "set_speed",
                json!({"capture_id": "rec_1", "range": [10, 30], "factor": 2}),
            ),
            (
                "set_background",
                json!({"capture_id": "rec_1", "kind": "color", "value": "#112233"}),
            ),
            (
                "set_audio_mix",
                json!({"capture_id": "rec_1", "system_gain_db": -6}),
            ),
            (
                "set_camera_layout",
                json!({"capture_id": "rec_1", "range": [0, 15], "layout": "camera"}),
            ),
            (
                "set_zooms",
                json!({"capture_id": "rec_1", "zooms": [{"at": 15, "length": 3, "focus": [0.5, 0.4]}]}),
            ),
            (
                "set_cursor_style",
                json!({"capture_id": "rec_1", "size": 2, "click_effect": "ripple"}),
            ),
            (
                "set_reframe",
                json!({"capture_id": "rec_1", "aspect": "9:16", "mode": "follow"}),
            ),
            ("freeze_display", json!({"display_id": 1})),
            ("release_frame", json!({"frame_id": "frame_1"})),
            ("window_thumbnail", json!({"window_id": 5, "max_px": 200})),
            ("get_job", json!({"job_id": "job_1"})),
            (
                "list_jobs",
                json!({"state": "running", "type": "export", "limit": 5}),
            ),
            ("cancel_job", json!({"job_id": "job_1"})),
            ("retry_job", json!({"job_id": "job_1"})),
            (
                "export_video",
                json!({"capture_id": "rec_1", "preset": "quick", "request_id": "r"}),
            ),
            ("start_recording", json!({"target": {"display_id": 1}})),
            ("pause_recording", json!({})),
            ("resume_recording", json!({"recording_id": "rec_1"})),
            ("set_mic_muted", json!({"muted": true, "request_id": "r"})),
            ("stop_recording", json!({"request_id": "r"})),
            ("get_recording_status", json!({})),
            ("list_devices", json!({})),
            ("discard_recording", json!({})),
            ("restart_recording", json!({})),
            ("skip_countdown", json!({})),
            ("run_fps_selftest", json!({"target": {"display_id": 1}})),
            (
                "publish_video",
                json!({"capture_id": "rec_1", "platform": "youtube"}),
            ),
            ("delete_capture", json!({"capture_id": "rec_1"})),
            (
                "move_library",
                json!({"path": "/Volumes/X/CaptureMann", "mode": "fresh", "request_id": "r"}),
            ),
            ("list_models", json!({})),
            (
                "download_model",
                json!({"model": "small", "request_id": "r"}),
            ),
            (
                "import_model",
                json!({"model": "small", "path": "/tmp/ggml-small.bin"}),
            ),
            ("select_model", json!({"model": "small"})),
            ("remove_model", json!({"model": "small"})),
            (
                "transcribe",
                json!({"capture_id": "rec_1", "language": "en", "model": "small"}),
            ),
            ("get_transcript", json!({"capture_id": "rec_1"})),
            ("get_captions", json!({"capture_id": "rec_1"})),
            (
                "set_captions",
                json!({"capture_id": "rec_1", "lines": [{"start": 1.0, "end": 2.5, "text": "Hi"}]}),
            ),
            (
                "replace_in_captions",
                json!({"capture_id": "rec_1", "replacements": [{"find": "a", "replace": "b"}]}),
            ),
            (
                "export_captions",
                json!({"capture_id": "rec_1", "format": "vtt"}),
            ),
            ("suggest_zooms", json!({"capture_id": "rec_1"})),
            ("suggest_cuts", json!({"capture_id": "rec_1"})),
            (
                "set_caption_options",
                json!({"capture_id": "rec_1", "enabled": true, "burn_in": false}),
            ),
            ("list_destinations", json!({})),
            ("list_publish_accounts", json!({})),
            (
                "upload",
                json!({"capture_id": "rec_1", "destination_id": "nas", "what": "both",
                       "folder": "Lessons", "request_id": "r"}),
            ),
        ]
    }

    /// CAP-1.AC1: `list_targets` stays the same action; `windows: false` selects the
    /// displays-only listing the overlay uses to freeze without waiting for window enumeration.
    #[test]
    fn cap1_ac1_list_targets_windows_flag() {
        for p in [json!({}), json!(null), json!({"windows": true})] {
            assert_eq!(
                Action::parse("list_targets", &p).unwrap(),
                Action::ListTargets
            );
        }
        let a = Action::parse("list_targets", &json!({"windows": false})).unwrap();
        assert_eq!(a, Action::ListDisplays);
        assert_eq!(a.method(), "list_targets");
        assert_eq!(a.spec().name, "list_targets");
        assert!(!a.spec().mutating);
    }

    #[test]
    fn registry_and_enum_agree() {
        let table: BTreeSet<_> = ACTIONS.iter().map(|s| s.name).collect();
        let sampled: BTreeSet<_> = samples().iter().map(|(n, _)| *n).collect();
        assert_eq!(
            table, sampled,
            "every action needs a sample and a table row"
        );
        for (name, params) in samples() {
            let a = Action::parse(name, &params).unwrap();
            assert_eq!(a.method(), name);
            assert_eq!(a.spec().name, name);
        }
    }

    #[test]
    fn agt_2_tier_table() {
        for s in ACTIONS {
            let confirm = matches!(
                s.name,
                "start_recording"
                    | "run_fps_selftest"
                    | "publish_video"
                    | "delete_capture"
                    | "move_library"
                    | "discard_recording"
                    | "restart_recording"
                    | "download_model"
                    | "remove_model"
            );
            assert_eq!(s.tier == Tier::Confirm, confirm, "{}", s.name);
            assert_ne!(s.tier, Tier::Never, "{} must not be exposed", s.name);
        }
    }

    #[test]
    fn agt_2_ac5_never_tier_has_no_action() {
        for n in NEVER_TIER {
            assert!(spec_for(n).is_none());
            assert_eq!(tier_of(n), Some(Tier::Never));
            assert!(Action::parse(n, &json!({})).is_err());
        }
    }

    /// CAP-1.AC2: `target: {frame_id, rect}` selects the frame crop; it is still the
    /// `capture_screenshot` action, with the same tier and the display-relative physical rect.
    #[test]
    fn cap1_ac2_frame_target_parses_to_a_crop_of_the_frame() {
        let a = Action::parse(
            "capture_screenshot",
            &json!({"target": {"frame_id": "frame_1", "rect": [2207, 669, 420, 649]},
                    "title": "t", "request_id": "r"}),
        )
        .unwrap();
        assert_eq!(a.method(), "capture_screenshot");
        assert_eq!(a.request_id(), Some("r"));
        assert_eq!(a.spec().tier, Tier::Free);
        let Action::CaptureFromFrame(p) = a else {
            panic!()
        };
        assert_eq!(p.frame_id, "frame_1");
        assert_eq!(p.rect, Some([2207, 669, 420, 649]));
        let whole =
            Action::parse("capture_screenshot", &json!({"target": {"frame_id": "f"}})).unwrap();
        assert!(matches!(whole, Action::CaptureFromFrame(p) if p.rect.is_none()));
        for (params, field) in [
            (
                json!({"target": {"frame_id": "f"}, "delay_s": 1}),
                "delay_s",
            ),
            (
                json!({"target": {"frame_id": "f", "display_id": 1}}),
                "target",
            ),
            (json!({"target": {"frame_id": ""}}), "target.frame_id"),
            (
                json!({"target": {"frame_id": "f", "rect": [0, 0, 0, 1]}}),
                "target.rect",
            ),
        ] {
            let e = Action::parse("capture_screenshot", &params).unwrap_err();
            assert!(e.to_string().starts_with(field), "{params}: {e}");
        }
    }

    #[test]
    fn window_thumbnail_params_are_validated() {
        let parse = |p: Value| Action::parse("window_thumbnail", &p);
        assert!(matches!(
            parse(json!({"window_id": 7})).unwrap(),
            Action::WindowThumbnail(WindowThumbnailParams {
                window_id: 7,
                max_px: 360
            })
        ));
        // max_px is clamped to 64..=1024, not refused.
        for (given, want) in [(10, 64), (64, 64), (500, 500), (1024, 1024), (99_999, 1024)] {
            match parse(json!({"window_id": 1, "max_px": given})).unwrap() {
                Action::WindowThumbnail(p) => assert_eq!(p.max_px, want, "{given}"),
                other => panic!("{other:?}"),
            }
        }
        for (p, field) in [
            (json!({}), "window_id"),
            (json!({"window_id": -1}), "window_id"),
            (json!({"window_id": "7"}), "window_id"),
            (json!({"window_id": 4_294_967_296u64}), "window_id"),
            (json!({"window_id": 1, "max_px": "big"}), "max_px"),
            (json!({"window_id": 1, "max_px": -5}), "max_px"),
            (json!({"window_id": 1, "max_px": 1.5}), "max_px"),
            (json!({"window_id": 1, "request_id": "r"}), "request_id"),
        ] {
            let e = parse(p.clone()).unwrap_err();
            assert!(e.to_string().starts_with(field), "{p}: {e}");
        }
        assert_eq!(
            spec_for("window_thumbnail").map(|s| (s.tier, s.mutating)),
            Some((Tier::Free, true))
        );
        assert!(is_internal("window_thumbnail"));
    }

    #[test]
    fn freeze_and_release_params_are_validated() {
        let a = Action::parse(
            "freeze_display",
            &json!({"display_id": 2, "transport": "inline"}),
        )
        .unwrap();
        assert!(matches!(
            a,
            Action::FreezeDisplay(FreezeDisplayParams {
                display_id: 2,
                transport: FrameTransport::Inline
            })
        ));
        for (m, p, field) in [
            ("freeze_display", json!({}), "display_id"),
            (
                "freeze_display",
                json!({"display_id": 1, "transport": "x"}),
                "transport",
            ),
            (
                "freeze_display",
                json!({"display_id": 1, "request_id": "r"}),
                "request_id",
            ),
            ("release_frame", json!({}), "frame_id"),
        ] {
            let e = Action::parse(m, &p).unwrap_err();
            assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
        }
    }

    /// AGT-1.AC4: the internal list is explicit, short, and every entry is a real action.
    #[test]
    fn agt_1_ac4_internal_actions_are_explicit() {
        for n in INTERNAL_ACTIONS {
            assert!(spec_for(n).is_some(), "{n}");
            assert!(is_internal(n));
        }
        assert!(!is_internal("capture_screenshot"));
        // The recording actions the spec exposes are public; only the HUD's are internal.
        for public in [
            "start_recording",
            "pause_recording",
            "resume_recording",
            "stop_recording",
            "get_recording_status",
            "list_devices",
        ] {
            assert!(!is_internal(public), "{public}");
        }
        assert_eq!(INTERNAL_ACTIONS.len(), 7);
    }

    #[test]
    fn region_rect_is_physical_xywh() {
        let a = Action::parse(
            "capture_screenshot",
            &json!({"target": {"display_id": 1, "rect": [2207, 669, 420, 649]}, "delay_s": 2}),
        )
        .unwrap();
        let Action::CaptureScreenshot(p) = a else {
            panic!()
        };
        assert_eq!(
            p.target,
            ScreenshotTarget::Region {
                display_id: 1,
                rect: [2207, 669, 420, 649]
            }
        );
        assert_eq!(p.delay_s, Some(2.0));
    }

    #[test]
    fn bad_inputs_name_the_field() {
        let cases = [
            ("capture_screenshot", json!({}), "target"),
            (
                "capture_screenshot",
                json!({"target": {"display_id": 1, "window_id": 2}}),
                "target",
            ),
            (
                "capture_screenshot",
                json!({"target": {"display_id": 1, "rect": [0, 0, 0, 5]}}),
                "target.rect",
            ),
            (
                "capture_screenshot",
                json!({"target": {"display_id": 1, "rect": [0.5, 0, 5, 5]}}),
                "target.rect",
            ),
            (
                "capture_screenshot",
                json!({"target": {"display_id": 1}, "bogus": 1}),
                "bogus",
            ),
            ("annotate", json!({"capture_id": "x", "ops": []}), "ops"),
            (
                "annotate",
                json!({"capture_id": "x", "ops": [{"op": "zig"}]}),
                "ops[0]",
            ),
            ("undo", json!({}), "capture_id"),
            ("suggest_redactions", json!({}), "capture_id"),
            (
                "suggest_redactions",
                json!({"capture_id": "x", "kinds": ["face"]}),
                "kinds[0]",
            ),
            (
                "suggest_redactions",
                json!({"capture_id": "x", "kinds": "email"}),
                "kinds",
            ),
            (
                "suggest_redactions",
                json!({"capture_id": "x", "request_id": "r"}),
                "request_id",
            ),
            (
                "undo",
                json!({"capture_id": "x", "request_id": ""}),
                "request_id",
            ),
            ("list_library", json!({"limit": 0}), "limit"),
            ("set_settings", json!({"values": {}}), "values"),
            ("delete_capture", json!({}), "capture_id"),
            (
                "delete_capture",
                json!({"capture_id": "x", "also_delete_remote": "yes"}),
                "also_delete_remote",
            ),
            ("rename_capture", json!({"capture_id": "x"}), "title"),
            (
                "rename_capture",
                json!({"capture_id": "x", "title": " "}),
                "title",
            ),
            ("rescan_library", json!({"x": 1}), "x"),
            ("list_destinations", json!({"x": 1}), "x"),
            ("list_publish_accounts", json!({"x": 1}), "x"),
            ("upload", json!({}), "capture_id"),
            (
                "upload",
                json!({"capture_id": "a", "file": "b"}),
                "capture_id",
            ),
            ("upload", json!({"capture_id": "a", "what": "all"}), "what"),
            ("upload", json!({"file": "b", "what": "export"}), "what"),
            (
                "upload",
                json!({"capture_id": "a", "folder": " "}),
                "folder",
            ),
            ("upload", json!({"capture_id": "a", "path": "/x"}), "path"),
            ("get_job", json!({}), "job_id"),
            (
                "cancel_job",
                json!({"job_id": "j", "request_id": ""}),
                "request_id",
            ),
            ("retry_job", json!({}), "job_id"),
            ("retry_job", json!({"job_id": "j", "force": true}), "force"),
            ("list_jobs", json!({"state": "stuck"}), "state"),
            ("list_jobs", json!({"limit": 0}), "limit"),
            ("list_jobs", json!({"checkpoint": true}), "checkpoint"),
            ("list_targets", json!({"x": 1}), "x"),
            ("list_targets", json!({"windows": "no"}), "windows"),
            ("export_video", json!({}), "capture_id"),
            (
                "export_video",
                json!({"capture_id": "r", "preset": "YouTube 1080p"}),
                "preset",
            ),
            (
                "export_video",
                json!({"capture_id": "r", "size": "1080p"}),
                "size",
            ),
            (
                "export_video",
                json!({"capture_id": "r", "bogus": 1}),
                "bogus",
            ),
            (
                "export_video",
                json!({"capture_id": "r", "request_id": ""}),
                "request_id",
            ),
        ];
        for (m, p, field) in cases {
            let e = Action::parse(m, &p).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
            assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
        }
        assert!(Action::parse("undo", &json!([1])).is_err());
    }

    /// LIB-1: `status` takes the four filter values; anything else is `invalid_input`.
    #[test]
    fn lib_1_list_library_status_values() {
        for s in ["uploaded", "not_uploaded", "published", "not_published"] {
            assert!(
                Action::parse("list_library", &json!({"status": s})).is_ok(),
                "{s}"
            );
        }
        for bad in [json!("all"), json!(3), json!("")] {
            let e = Action::parse("list_library", &json!({"status": bad})).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{bad}");
        }
        assert!(Action::parse("list_library", &json!({"status": null})).is_ok());
    }

    /// R1-1: `annotate` takes `copy_to_clipboard`.
    #[test]
    fn r1_1_annotate_accepts_copy_to_clipboard() {
        let a = Action::parse(
            "annotate",
            &json!({"capture_id": "x", "ops": [{"op": "rect", "rect": [20, 20, 50, 30], "color": "#2E8BFF", "width": 3}], "copy_to_clipboard": true}),
        );
        match a {
            Ok(Action::Annotate(p)) => assert!(p.copy_to_clipboard),
            other => panic!("{other:?}"),
        }
        let e = Action::parse(
            "annotate",
            &json!({"capture_id": "x", "ops": [], "replace": true, "copy_to_clipboard": "yes"}),
        )
        .unwrap_err();
        assert!(e.to_string().starts_with("copy_to_clipboard"), "{e}");
    }

    /// REC-4 / REC-5 / AGT-1: the recording actions parse `recording_id` and `request_id` only,
    /// `list_devices` takes nothing, and the typed `start_recording` input is validated at parse
    /// time (before any dialog).
    #[test]
    fn recording_actions_parse_and_validate() {
        for m in [
            "pause_recording",
            "resume_recording",
            "stop_recording",
            "discard_recording",
            "restart_recording",
            "skip_countdown",
        ] {
            let a = Action::parse(m, &json!({"recording_id": "rec_1", "request_id": "r"})).unwrap();
            assert_eq!(a.method(), m);
            assert_eq!(a.request_id(), Some("r"));
            assert!(a.spec().mutating, "{m}");
            for (p, field) in [
                (json!({"recording_id": ""}), "recording_id"),
                (json!({"recording_id": 5}), "recording_id"),
                (json!({"x": 1}), "x"),
            ] {
                let e = Action::parse(m, &p).unwrap_err();
                assert!(e.to_string().starts_with(field), "{m} {p}: {e}");
            }
        }
        let s = Action::parse("get_recording_status", &json!({"recording_id": "rec_1"})).unwrap();
        assert!(!s.spec().mutating);
        assert!(Action::parse("get_recording_status", &json!({"request_id": "r"})).is_err());
        assert_eq!(
            Action::parse("list_devices", &json!({})).unwrap(),
            Action::ListDevices
        );
        assert!(Action::parse("list_devices", &json!({"x": 1})).is_err());

        // start_recording: typed validation, field-named errors, still pass-through payload.
        let a = Action::parse(
            "start_recording",
            &json!({"target": {"display_id": 1}, "fps": 60, "request_id": "r",
                    "camera": {"shape": "circle"}, "mic": "default"}),
        )
        .unwrap();
        assert_eq!(a.request_id(), Some("r"));
        assert_eq!(a.spec().tier, Tier::Confirm);
        let Action::StartRecording(p) = &a else {
            panic!()
        };
        assert_eq!(p.rest["fps"], 60);
        for (p, field) in [
            (json!({}), "target"),
            (json!({"target": {"display_id": 1}, "fps": 50}), "fps"),
            (
                json!({"target": {"display_id": 1}, "capture_id": "x"}),
                "capture_id",
            ),
            (json!({"target": {"frame_id": "f"}}), "target"),
        ] {
            let e = Action::parse("start_recording", &p).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
            assert!(e.to_string().starts_with(field), "{p}: {e}");
        }
    }

    /// EXP-1 / EXP-4: `export_video` defaults to the `quick` preset, is Free and mutating, and
    /// refuses presets and options that do not exist yet by name.
    #[test]
    fn export_video_parses_the_quick_preset_only() {
        let a = Action::parse("export_video", &json!({"capture_id": "rec_1"})).unwrap();
        let Action::ExportVideo(p) = &a else { panic!() };
        assert_eq!(
            (p.capture_id.as_str(), p.preset.as_str()),
            ("rec_1", "quick")
        );
        assert_eq!(a.spec().tier, Tier::Free);
        assert!(a.spec().mutating);
        assert_eq!(a.capture_id(), Some("rec_1"));
        assert!(!is_internal("export_video"));
        // EXP-2 / EXP-3: any well-formed preset id parses (the table decides if it exists), and
        // `format` is the same as the gif and webm presets.
        let Action::ExportVideo(p) = Action::parse(
            "export_video",
            &json!({"capture_id": "r", "preset": "youtube-4k"}),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(p.preset, "youtube-4k");
        let Action::ExportVideo(p) = Action::parse(
            "export_video",
            &json!({"capture_id": "r", "format": "gif", "width": 640, "fps": 10, "max_mb": 10}),
        )
        .unwrap() else {
            panic!()
        };
        assert_eq!(
            (p.preset.as_str(), p.width, p.fps, p.max_mb),
            ("gif", Some(640), Some(10.0), Some(10.0))
        );
        for bad in [
            json!({"capture_id": "r", "preset": "YouTube 4K"}),
            json!({"capture_id": "r", "format": "avi"}),
            json!({"capture_id": "r", "format": "gif", "preset": "webm"}),
            json!({"capture_id": "r", "format": "mp4", "preset": "gif"}),
            json!({"capture_id": "r", "width": 15}),
            json!({"capture_id": "r", "width": 640.5}),
            json!({"capture_id": "r", "fps": 61}),
            json!({"capture_id": "r", "max_mb": -1}),
            json!({"capture_id": "r", "codec": "hevc"}),
        ] {
            let e = Action::parse("export_video", &bad).unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput, "{bad}");
        }
        assert!(Action::parse("export_video", &json!({"capture_id": "r", "size": null})).is_ok());
        // AUD-4: the loudness target parses and is range checked.
        let a = Action::parse(
            "export_video",
            &json!({"capture_id": "r", "loudness_lufs": -14}),
        )
        .unwrap();
        let Action::ExportVideo(p) = &a else { panic!() };
        assert_eq!(p.loudness_lufs, Some(-14.0));
        for bad in [json!(-3), json!(-41), json!("x"), json!(true)] {
            let e = Action::parse(
                "export_video",
                &json!({"capture_id": "r", "loudness_lufs": bad}),
            )
            .unwrap_err();
            assert_eq!(e.code(), cm_core::ErrorCode::InvalidInput);
        }
    }

    /// REC-5: Discard and Restart are Confirm tier (for agents) and internal; their summaries say
    /// what they do.
    #[test]
    fn rec5_discard_and_restart_are_internal_confirm_actions() {
        for m in ["discard_recording", "restart_recording"] {
            assert_eq!(spec_for(m).unwrap().tier, Tier::Confirm);
            assert!(is_internal(m));
        }
        let d = Action::parse("discard_recording", &json!({})).unwrap();
        assert!(d.summary().contains("delete its files"));
        let r = Action::parse("restart_recording", &json!({})).unwrap();
        assert!(r.summary().contains("start over"));
    }

    #[test]
    fn external_params_pass_through() {
        let a = Action::parse(
            "publish_video",
            &json!({"capture_id":"rec_1","platform":"youtube","privacy":"unlisted","request_id":"r"}),
        )
        .unwrap();
        assert_eq!(a.request_id(), Some("r"));
        assert_eq!(a.capture_id(), Some("rec_1"));
        assert_eq!(a.summary(), "Publish capture rec_1 to youtube");
        assert_eq!(
            a.details(),
            json!({"capture_id":"rec_1","rest":{"platform":"youtube","privacy":"unlisted"}})
        );
    }
}
