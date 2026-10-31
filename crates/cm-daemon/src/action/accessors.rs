use super::*;

impl Action {
    /// The method / tool name.
    pub fn method(&self) -> &'static str {
        match self {
            Action::ListTargets | Action::ListDisplays => "list_targets",
            Action::CaptureScreenshot(_) | Action::CaptureFromFrame(_) | Action::CapturePick(_) => {
                "capture_screenshot"
            }
            Action::FreezeDisplay(_) => "freeze_display",
            Action::ReleaseFrame(_) => "release_frame",
            Action::WindowThumbnail(_) => "window_thumbnail",
            Action::GetJob(_) => "get_job",
            Action::ListJobs(_) => "list_jobs",
            Action::CancelJob(_) => "cancel_job",
            Action::RetryJob(_) => "retry_job",
            Action::ExportVideo(_) => "export_video",
            Action::ExportAudio(_) => "export_audio",
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
            Action::GetTextModel => "get_text_model",
            Action::GetDiagnostics => "get_diagnostics",
            Action::TestTextModel => "test_text_model",
            Action::RescanLibrary => "rescan_library",
            Action::StartRecording(_) => "start_recording",
            Action::PauseRecording(_) => "pause_recording",
            Action::ResumeRecording(_) => "resume_recording",
            Action::SetMicMuted(_) => "set_mic_muted",
            Action::AddMarker(_) => crate::markers::METHOD,
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
            Action::CapturePick(p) => p.request_id.as_deref(),
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
            Action::ExportAudio(p) => p.request_id.as_deref(),
            Action::SetMicMuted(p) => p.request_id.as_deref(),
            Action::AddMarker(p) => p.request_id.as_deref(),
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
            Action::AddMarker(p) => p.capture_id.as_deref(),
            Action::ExportVideo(p) => Some(&p.capture_id),
            Action::ExportAudio(p) => Some(&p.capture_id),
            Action::PublishVideo(p) | Action::DeleteCapture(p) => p.capture_id.as_deref(),
            _ => None,
        }
    }

    /// One sentence for the confirmation dialog and the activity log.
    pub fn summary(&self) -> String {
        match self {
            Action::StartRecording(p)
                if p.rest.get("mode").and_then(Value::as_str) == Some("audio") =>
            {
                "Start an audio recording".to_owned()
            }
            Action::StartRecording(_) => "Start a screen recording".to_owned(),
            Action::AddMarker(p) => crate::markers::summary(p),
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
            Action::GetTextModel => "Show the text model settings".to_owned(),
            Action::GetDiagnostics => "Show the diagnostics".to_owned(),
            Action::TestTextModel => "Test the text model".to_owned(),
            Action::Upload(p) => crate::storage_ops::summary(p),
            Action::CaptionOptions(p) => crate::caption_options::summary(p),
            Action::ExportAudio(p) => crate::export_audio::summary(p),
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
            Action::ExportAudio(p) => json!({
                "capture_id": p.capture_id, "format": p.format.as_str(),
                "tracks": p.tracks.as_str(), "loudness_lufs": p.loudness_lufs,
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
