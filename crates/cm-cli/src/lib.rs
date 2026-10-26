//! `capturemann`: the CaptureMann command-line client (AGT-1).
//!
//! The CLI is a thin JSON-RPC client of the daemon (`cm-daemon` protocol over the local
//! socket). It never calls an engine crate: every subcommand is one action (or two, for
//! `capture screenshot --annotate`) with the same name as the MCP tool, so GUI, CLI and
//! agents share state, undo and confirmation gates. When no daemon listens, the CLI starts a
//! headless `cm-daemon` and waits for it (AGT-1.AC2) unless `--no-start` is given.
//!
//! [`run`] is the whole program with injectable I/O ([`Env`]), so tests drive it without
//! spawning a process.
//!
//! # Output
//!
//! Human-readable by default; `--json` prints the daemon's result object (image bytes are
//! never printed: `image.data` is replaced by `image.bytes`, use `--out FILE` to get the
//! PNG). Errors go to stderr as `error[<code>]: <message>` (or `{"error": {...}}` with
//! `--json`); the exit code follows `specs/README.md` via [`error::exit_code_for`].

#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod args;
pub mod client;
mod commands;
pub mod error;
pub mod launch;

use std::io::{Read, Write};
use std::sync::{Arc, Mutex, PoisonError};

use clap::Parser;
use cm_ipc::Endpoint;

pub use args::Cli;
pub use error::{exit_code_for, CliError};
pub use launch::Launcher;

/// Parity table (AGT-1.AC4): every daemon action and the CLI command that calls it, or `None`
/// when the CLI command is not built yet (none today). Internal actions (`cm_daemon::INTERNAL_ACTIONS`: the overlay's frames and the
/// recording HUD's discard, restart and skip-countdown) are deliberately not here. A test fails
/// when an action is missing.
pub const CLI_COMMANDS: &[(&str, Option<&str>)] = &[
    ("list_targets", Some("targets")),
    ("capture_screenshot", Some("capture screenshot")),
    ("annotate", Some("annotate")),
    ("get_capture", Some("capture get")),
    ("suggest_redactions", Some("annotate --suggest-redactions")),
    ("get_settings", Some("settings get")),
    ("set_settings", Some("settings set")),
    ("undo", Some("undo")),
    ("redo", Some("redo")),
    ("list_library", Some("library list")),
    ("rename_capture", Some("library rename")),
    ("list_destinations", Some("destinations")),
    ("upload", Some("upload")),
    ("get_job", Some("jobs get")),
    ("list_jobs", Some("jobs list")),
    ("cancel_job", Some("jobs cancel")),
    ("retry_job", Some("jobs retry")),
    ("export_video", Some("export")),
    ("export_audio", Some("export --audio")),
    ("start_recording", Some("record start")),
    ("pause_recording", Some("record pause")),
    ("resume_recording", Some("record resume")),
    ("set_mic_muted", Some("record mic")),
    ("add_marker", Some("record marker")),
    ("stop_recording", Some("record stop")),
    ("get_recording_status", Some("record status")),
    ("list_devices", Some("devices")),
    ("run_fps_selftest", Some("record selftest")),
    ("trim", Some("edit trim")),
    ("cut_segments", Some("edit cut")),
    ("restore_segments", Some("edit restore")),
    ("split", Some("edit split")),
    ("set_speed", Some("edit speed")),
    ("set_background", Some("edit background")),
    ("set_audio_mix", Some("edit audio")),
    ("set_camera_layout", Some("edit camera")),
    ("set_zooms", Some("edit zoom")),
    ("set_cursor_style", Some("edit cursor")),
    ("set_reframe", Some("edit reframe")),
    ("list_publish_accounts", Some("publish accounts")),
    ("publish_video", Some("publish")),
    ("delete_capture", Some("library rm")),
    ("move_library", Some("library move")),
    ("list_models", Some("ai models")),
    ("download_model", Some("ai models download")),
    ("import_model", Some("ai models import")),
    ("select_model", Some("ai models use")),
    ("remove_model", Some("ai models rm")),
    ("transcribe", Some("ai transcribe")),
    ("get_transcript", Some("ai transcript")),
    ("get_captions", Some("ai captions get")),
    ("set_captions", Some("ai captions set")),
    ("replace_in_captions", Some("ai captions replace")),
    ("set_caption_options", Some("ai captions options")),
    ("export_captions", Some("export")),
    ("translate_captions", Some("ai translate")),
    ("suggest_zooms", Some("ai suggest zooms")),
    ("suggest_cuts", Some("ai suggest cuts")),
    ("get_text_model", Some("ai text-model")),
    ("test_text_model", Some("ai text-model test")),
    ("diarize", Some("ai speakers")),
    ("set_speakers", Some("ai speakers rename")),
    ("list_report_templates", Some("ai report templates")),
    ("generate_report", Some("ai report")),
    ("get_report", Some("ai report get")),
    ("export_report", Some("ai report export")),
    ("delete_report", Some("ai report delete")),
    ("edit_report", Some("ai report edit")),
];

/// Everything the CLI touches outside its arguments.
pub struct Env {
    /// Where the daemon listens (overridden by `--socket`).
    pub endpoint: Endpoint,
    /// Starts a daemon when none is running; `None` never starts one.
    pub launcher: Option<Launcher>,
    pub stdin: Box<dyn Read + Send + Sync>,
    pub out: Box<dyn Write + Send + Sync>,
    pub err: Box<dyn Write + Send + Sync>,
}

impl Env {
    /// The real process: default endpoint, `cm-daemon` launcher, std streams.
    pub fn process() -> Env {
        Env {
            endpoint: Endpoint::default_for_platform(),
            launcher: Some(launch::process_launcher(None)),
            stdin: Box::new(std::io::stdin()),
            out: Box::new(std::io::stdout()),
            err: Box::new(std::io::stderr()),
        }
    }
}

/// A cloneable in-memory writer for capturing output in tests.
#[derive(Clone, Default)]
pub struct SharedBuf(Arc<Mutex<Vec<u8>>>);

impl SharedBuf {
    pub fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap_or_else(PoisonError::into_inner)).into_owned()
    }
}

impl Write for SharedBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs one invocation (`args[0]` is the program name) and returns the exit code:
/// 0 success, 2 bad input (including usage errors), 3/4/5/6 per the shared error table, 1
/// for anything else.
pub async fn run<I, S>(args: I, env: &mut Env) -> i32
where
    I: IntoIterator<Item = S>,
    S: Into<std::ffi::OsString> + Clone,
{
    let cli = match Cli::try_parse_from(args) {
        Ok(c) => c,
        Err(e) => {
            let text = e.render().to_string();
            return if e.use_stderr() {
                let _ = env.err.write_all(text.as_bytes());
                2
            } else {
                let _ = env.out.write_all(text.as_bytes());
                0
            };
        }
    };
    let json = cli.json;
    match commands::execute(cli, env).await {
        Ok(()) => 0,
        Err(e) => {
            let text = if json {
                e.render_json()
            } else {
                e.render_human()
            };
            let _ = writeln!(env.err, "{text}");
            e.exit
        }
    }
}
