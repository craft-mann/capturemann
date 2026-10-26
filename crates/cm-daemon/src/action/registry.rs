use super::*;

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
    // VOC-2: the audio of a capture as M4A or WAV; the same `export` job type, `kind: "audio"`.
    spec("export_audio", Tier::Free, true),
    // Recording (REC-1..REC-5). Starting asks the user; the rest of the controls are Free because
    // the recording itself was already confirmed (REC-4). `discard_recording` and
    // `restart_recording` delete a take: Confirm for agents, direct for the user in the HUD.
    spec("start_recording", Tier::Confirm, true),
    spec("pause_recording", Tier::Free, true),
    spec("resume_recording", Tier::Free, true),
    // Mic mute (AUD-6): the track keeps running, the file gets digital silence for the muted
    // span. Free and mutating like pause and resume: the recording itself was confirmed.
    spec("set_mic_muted", Tier::Free, true),
    // VOC-1: a marker is a label at a time (`manifest.markers`); Free like pause. On a finished
    // recording it is one revision and one undo step.
    spec("add_marker", Tier::Free, true),
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
    // AI-5: a `translate` job writing `translations/<lang>.json`. Free with Whisper or a local
    // text model; a remote text model will make it Confirm (AI-7, not built).
    spec("translate_captions", Tier::Free, true),
    spec("suggest_zooms", Tier::Free, false),
    spec("suggest_cuts", Tier::Free, false),
    // AI-8 speaker labels: `diarize` is a background job on this computer (no network);
    // `set_speakers` renames, merges or reassigns as one undoable revision.
    spec("diarize", Tier::Free, true),
    spec("set_speakers", Tier::Free, true),
    // AI-9 reports. `generate_report` is a `report` job on the local text model (a non-local
    // provider is refused as `no_text_model`: remote providers are deferred); create, edit and
    // delete are one undoable revision each.
    spec("list_report_templates", Tier::Free, false),
    spec("generate_report", Tier::Free, true),
    spec("get_report", Tier::Free, false),
    spec("export_report", Tier::Free, true),
    spec("delete_report", Tier::Free, true),
    spec("edit_report", Tier::Free, true),
    // The caption switches (show in the preview, burn into exports): one revision, one undo step.
    spec("set_caption_options", Tier::Free, true),
    // Storage (M6, STO-1 to STO-5): copies go only to destinations the user set up in the app;
    // nothing is deleted or published, so both are Free.
    spec("list_destinations", Tier::Free, false),
    spec("upload", Tier::Free, true),
    // Publishing (M6, PUB-1, PUB-2): the accounts are set up in the app; an agent can read them
    // (label, defaults, quota; never a key or token). `publish_video` above is the Confirm one.
    spec("list_publish_accounts", Tier::Free, false),
    // AI-7: the text model provider. Both read the app-only `ai.text.*` settings; the key is never
    // returned. `test_text_model` sends one fixed short prompt to the configured provider only.
    spec("get_text_model", Tier::Free, false),
    spec("test_text_model", Tier::Free, false),
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
