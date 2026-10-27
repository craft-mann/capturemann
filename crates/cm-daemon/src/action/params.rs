use super::*;

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
    ExportAudio(crate::export_audio::ExportAudioParams),
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
    /// `list_text_models`, `download_text_model`, `select_text_model`, `remove_text_model`,
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
    /// `get_text_model` (see [`crate::text_model_ops`]).
    GetTextModel,
    /// `test_text_model` (see [`crate::text_model_ops`]).
    TestTextModel,
    /// Internal: rebuild the index from the bundle folders.
    RescanLibrary,
    /// `start_recording`. The payload stays pass-through JSON for compatibility; it was validated
    /// with [`crate::recording::StartParams::parse`] when the action was parsed.
    StartRecording(ExternalParams),
    PauseRecording(RecordingRefParams),
    ResumeRecording(RecordingRefParams),
    SetMicMuted(SetMicMutedParams),
    /// `add_marker` (VOC-1, see [`crate::markers`]).
    AddMarker(crate::markers::AddMarkerParams),
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
