use super::*;

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
            "export_audio" => crate::export_audio::parse(&f)?,
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
            "get_diagnostics" => {
                f.only(&[])?;
                Action::GetDiagnostics
            }
            "get_text_model" => {
                f.only(&[])?;
                Action::GetTextModel
            }
            "test_text_model" => {
                f.only(&[])?;
                Action::TestTextModel
            }
            "list_publish_accounts" => {
                f.only(&[])?;
                Action::ListPublishAccounts
            }
            m if crate::ai::is_ai_method(m) => {
                return crate::ai::parse(m, &f);
            }
            crate::markers::METHOD => {
                return crate::markers::parse(&f);
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
                                    format!(
                                        "must be one of {}",
                                        cm_render::ocr::RedactionKind::names().join(", ")
                                    ),
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
                    if !matches!(k.as_str(), "screenshot" | "recording" | "audio") {
                        return Err(CmError::invalid_field(
                            "kind",
                            "must be `screenshot`, `recording` or `audio`",
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
}
