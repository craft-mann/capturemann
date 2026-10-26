//! Subcommand implementations: build the JSON-RPC params, call the daemon, print the result.

use std::io::Read;

use cm_ipc::Endpoint;
use serde_json::{json, Value};

mod ai;
mod capture;
mod edit;
mod export;
mod jobs;
mod library;
mod publish;
mod record;
mod settings;

use crate::args::{
    CaptureCmd, Cli, Command, DaemonCmd, JobsCmd, LibraryCmd, LibraryListArgs, RecordCmd,
    SettingsCmd,
};
use crate::client::Client;
use crate::error::CliError;
use crate::Env;
use capture::{annotate, get_capture, screenshot, targets, undo_redo};
use edit::edit;
use export::export_video;
use jobs::{jobs_cancel, jobs_get, jobs_list, jobs_retry, jobs_watch};
use library::{library_list, library_move, library_rename, library_rm};
use publish::{destinations, publish, upload};
use record::{
    record_control, record_marker, record_mic, record_selftest, record_start, record_status,
    record_watch,
};
use settings::{daemon_start, daemon_status, devices, mcp, settings_get, settings_set};

struct Ctx<'a> {
    json: bool,
    request_id: Option<String>,
    env: &'a mut Env,
    endpoint: Endpoint,
    no_start: bool,
}

pub async fn execute(cli: Cli, env: &mut Env) -> Result<(), CliError> {
    let endpoint = match &cli.socket {
        None => env.endpoint.clone(),
        Some(s) if cfg!(windows) => Endpoint::Pipe(s.clone()),
        Some(s) => Endpoint::Path(s.into()),
    };
    let mut cx = Ctx {
        json: cli.json,
        request_id: cli.request_id,
        env,
        endpoint,
        no_start: cli.no_start,
    };
    match cli.command {
        Command::Targets { displays_only, .. } => targets(&mut cx, displays_only).await,
        Command::Capture { cmd } => match cmd {
            CaptureCmd::Screenshot(a) => screenshot(&mut cx, a).await,
            CaptureCmd::Get(a) => get_capture(&mut cx, a).await,
        },
        Command::Annotate(a) => annotate(&mut cx, a).await,
        Command::Undo(a) => undo_redo(&mut cx, a, true).await,
        Command::Redo(a) => undo_redo(&mut cx, a, false).await,
        Command::Export(a) => export_video(&mut cx, a).await,
        Command::Library { cmd } => match cmd {
            LibraryCmd::List(a) => library_list(&mut cx, a).await,
            LibraryCmd::Rename { id, title } => library_rename(&mut cx, id, title).await,
            LibraryCmd::Rm { id, also_remote } => library_rm(&mut cx, id, also_remote).await,
            LibraryCmd::Move(a) => library_move(&mut cx, a).await,
            LibraryCmd::Search { text, paging } => {
                library_list(
                    &mut cx,
                    LibraryListArgs {
                        query: Some(text),
                        paging,
                    },
                )
                .await
            }
        },
        Command::Settings { cmd } => match cmd {
            SettingsCmd::Get { keys } => settings_get(&mut cx, keys).await,
            SettingsCmd::Set { pairs } => settings_set(&mut cx, pairs).await,
        },
        Command::Record { cmd } => match cmd {
            RecordCmd::Start(a) => record_start(&mut cx, *a).await,
            RecordCmd::Pause(a) => record_control(&mut cx, "pause_recording", a).await,
            RecordCmd::Resume(a) => record_control(&mut cx, "resume_recording", a).await,
            RecordCmd::Mic(a) => record_mic(&mut cx, a).await,
            RecordCmd::Marker(a) => record_marker(&mut cx, a).await,
            RecordCmd::Stop(a) => record_control(&mut cx, "stop_recording", a).await,
            RecordCmd::Status(a) => record_status(&mut cx, a).await,
            RecordCmd::Selftest(a) => record_selftest(&mut cx, a).await,
            RecordCmd::Watch { topics, max_events } => {
                record_watch(&mut cx, topics, max_events).await
            }
        },
        Command::Devices => devices(&mut cx).await,
        Command::Destinations => destinations(&mut cx).await,
        Command::Upload(a) => upload(&mut cx, a).await,
        Command::Publish(a) => publish(&mut cx, a).await,
        Command::Mcp { http, port } => mcp(&cx, http, port),
        Command::Edit { cmd } => edit(&mut cx, cmd).await,
        Command::Ai { cmd } => ai::ai(&mut cx, cmd).await,
        Command::Daemon { cmd } => match cmd {
            DaemonCmd::Start { library } => daemon_start(&mut cx, library).await,
            DaemonCmd::Status => daemon_status(&mut cx).await,
        },
        Command::Jobs { cmd } => match cmd {
            JobsCmd::Get { id } => jobs_get(&mut cx, id).await,
            JobsCmd::List {
                state,
                job_type,
                limit,
            } => jobs_list(&mut cx, state, job_type, limit).await,
            JobsCmd::Cancel { id } => jobs_cancel(&mut cx, id).await,
            JobsCmd::Retry { id } => jobs_retry(&mut cx, id).await,
            JobsCmd::Watch { topics, max_events } => jobs_watch(&mut cx, topics, max_events).await,
        },
    }
}

impl Ctx<'_> {
    async fn client(&self) -> Result<Client, CliError> {
        let launcher = if self.no_start {
            None
        } else {
            self.env.launcher.as_ref()
        };
        Client::connect_or_start(&self.endpoint, launcher).await
    }

    fn print(&mut self, text: &str) {
        let _ = writeln!(self.env.out, "{text}");
    }

    fn print_json(&mut self, v: &Value) {
        let s = serde_json::to_string_pretty(v).unwrap_or_default();
        self.print(&s);
    }

    /// Prints `v` as JSON or via `human`.
    fn emit(&mut self, v: &Value, human: impl FnOnce(&Value) -> String) {
        if self.json {
            self.print_json(v);
        } else {
            let t = human(v);
            self.print(t.trim_end());
        }
    }

    /// `params` plus `request_id` when `--request-id` was given (mutating actions only).
    fn mutating(&self, mut params: Value, suffix: &str) -> Value {
        if let (Some(id), Some(o)) = (&self.request_id, params.as_object_mut()) {
            o.insert("request_id".into(), json!(format!("{id}{suffix}")));
        }
        params
    }
}

/// Reads annotation ops: `-` is stdin, text starting with `[` or `{` is inline JSON, anything
/// else is a file. Accepts an array or `{"ops": [...]}` (ANN-1, `--annotate ops.json`).
fn load_ops(spec: &str, flag: &str, stdin: &mut dyn Read) -> Result<Value, CliError> {
    let text = if spec == "-" {
        let mut s = String::new();
        stdin
            .read_to_string(&mut s)
            .map_err(|e| CliError::invalid_input(format!("{flag}: cannot read stdin: {e}")))?;
        s
    } else if spec.trim_start().starts_with(['[', '{']) {
        spec.to_owned()
    } else {
        std::fs::read_to_string(spec)
            .map_err(|e| CliError::invalid_input(format!("{flag}: cannot read `{spec}`: {e}")))?
    };
    let v: Value = serde_json::from_str(&text)
        .map_err(|e| CliError::invalid_input(format!("{flag}: invalid JSON: {e}")))?;
    let ops = match v {
        Value::Object(mut o) if o.contains_key("ops") => o.remove("ops").unwrap_or_default(),
        other => other,
    };
    if !ops.is_array() {
        return Err(CliError::invalid_input(format!(
            "{flag}: expected a JSON array of ops (or an object with an `ops` array)"
        )));
    }
    Ok(ops)
}

fn plain(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), str::to_owned)
}
