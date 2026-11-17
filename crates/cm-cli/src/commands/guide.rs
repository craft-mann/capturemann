//! `guide` subcommands (GDE-4): Click Guide from the command line. Each one is one action of the
//! same name (`start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide`), so the
//! confirmation, the undo history and the results are the ones agents and the app get.

use std::fmt::Write as _;
use std::io::Read;

use clap::{Args, Subcommand};
use serde_json::{json, Map, Value};

use super::capture::{parse_region, primary_display, resolve_window};
use super::{plain, Ctx};
use crate::error::CliError;

#[derive(Debug, Subcommand)]
pub enum GuideCmd {
    /// Start a Click Guide (`start_guide`): every mouse click in the target becomes a step with a
    /// screenshot. Asks for approval on screen when an agent calls it; never records keystrokes.
    /// Needs Input Monitoring on macOS. Stop it with `guide stop`.
    Start(GuideStartArgs),
    /// Stop the recording guide (`stop_guide`) and print it.
    Stop {
        /// The guide (default: the one recording).
        id: Option<String>,
    },
    /// Print a guide: its steps, titles, notes and click points (`get_guide`).
    Get {
        /// The guide (default: the one recording).
        id: Option<String>,
    },
    /// Change a guide (`set_guide`): title, intro, numbering, the order, titles and notes of the
    /// steps, deletions, duplicates and new note steps. One call is one undo step (`undo <id>`).
    Set(GuideSetArgs),
    /// Export a guide (`export_guide`): a Markdown folder, one HTML file, or rich text on the
    /// clipboard.
    Export(GuideExportArgs),
}

#[derive(Debug, Args)]
pub struct GuideStartArgs {
    /// Display id to follow (see `targets`; default: the primary display).
    #[arg(long, value_name = "ID", conflicts_with = "window")]
    pub display: Option<u32>,
    /// Follow one window: its id, or a case-insensitive substring of its title or app name. An exact title wins; several partial matches fail with `ambiguous_target` and list their ids.
    #[arg(long, value_name = "ID|TITLE", conflicts_with_all = ["display", "region"])]
    pub window: Option<String>,
    /// Follow a region `x,y,w,h` in physical px, relative to the display.
    #[arg(long, value_name = "X,Y,W,H", allow_hyphen_values = true)]
    pub region: Option<String>,
    /// The guide's title (default: `Click Guide <date time>`).
    #[arg(long)]
    pub title: Option<String>,
    /// Seconds before the first click is listened to (0 to 10, default 3).
    #[arg(long, value_name = "SECONDS")]
    pub countdown: Option<f64>,
}

#[derive(Debug, Args)]
pub struct GuideSetArgs {
    /// The guide (default: the one recording).
    pub id: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    /// The intro paragraph.
    #[arg(long)]
    pub intro: Option<String>,
    /// Badge numbering: `number` (1 2 3), `upper` (A B C) or `lower` (a b c).
    #[arg(long, value_parser = ["number", "upper", "lower"])]
    pub numbering: Option<String>,
    /// The new step list as JSON, `@file` or `-` (stdin): `[{"id":"step_2"},{"id":"step_1","title":"Open Settings","note":"..."},{"id":"step_3","delete":true},{"title":"Type your password"}]`.
    /// An entry with no `id` is a new note step; steps not listed keep their order after the listed ones.
    #[arg(long, value_name = "JSON|@FILE|-")]
    pub steps: Option<String>,
    /// While recording: pause.
    #[arg(long, conflicts_with = "resume")]
    pub pause: bool,
    /// While recording: resume.
    #[arg(long)]
    pub resume: bool,
    /// Add a note step (a step with no image) with this title.
    #[arg(long, value_name = "TITLE")]
    pub note_step: Option<String>,
    /// With `--note-step`: the note under the title.
    #[arg(long, requires = "note_step", value_name = "TEXT")]
    pub note: Option<String>,
    /// While recording: remove the newest step.
    #[arg(long)]
    pub remove_last: bool,
    /// Turn the focus crop `on` or `off` for the steps named with `--crop-step`.
    #[arg(long, value_name = "on|off", value_parser = ["on", "off"], requires = "crop_step")]
    pub focus_crop: Option<String>,
    /// A step for `--focus-crop` (repeatable).
    #[arg(long, value_name = "STEP")]
    pub crop_step: Vec<String>,
    /// Take a fresh frame of the followed target for this step (repeatable).
    #[arg(long, value_name = "STEP")]
    pub retake: Vec<String>,
}

#[derive(Debug, Args)]
pub struct GuideExportArgs {
    /// The guide.
    pub id: String,
    /// `markdown` (a folder with guide.md and images/), `html` (one file), `pdf` (one file) or
    /// `rich-text` (clipboard).
    #[arg(long, value_parser = ["markdown", "html", "pdf", "rich-text"])]
    pub format: String,
    /// Where to write: a folder for markdown, a file or folder for html and pdf. Not used by rich-text.
    #[arg(long, value_name = "PATH")]
    pub dest: Option<String>,
    /// Step headings: `numbered` (`1. Title`), `step` (`Step 1 · Title`) or `title-only`.
    #[arg(long, value_parser = ["numbered", "step", "title-only"])]
    pub headings: Option<String>,
    /// PDF paper size (default `a4`).
    #[arg(long, value_parser = ["a4", "letter"])]
    pub paper: Option<String>,
    /// Image size: 1 (at most 800 px wide) or 2 (1600 px, never larger than the frame).
    #[arg(long, value_parser = ["1", "2"])]
    pub image_scale: Option<String>,
    /// Leave out the intro.
    #[arg(long)]
    pub no_intro: bool,
    /// Leave out the step notes.
    #[arg(long)]
    pub no_notes: bool,
    /// Leave out the note steps that have no image.
    #[arg(long)]
    pub no_note_steps: bool,
}

fn read_json_arg(cx: &mut Ctx<'_>, v: &str, what: &str) -> Result<Value, CliError> {
    let text = if v == "-" {
        let mut s = String::new();
        cx.env
            .stdin
            .read_to_string(&mut s)
            .map_err(|e| CliError::invalid_input(format!("cannot read stdin: {e}")))?;
        s
    } else if let Some(path) = v.strip_prefix('@') {
        std::fs::read_to_string(path)
            .map_err(|e| CliError::invalid_input(format!("cannot read `{path}`: {e}")))?
    } else {
        v.to_owned()
    };
    serde_json::from_str(super::strip_bom(&text))
        .map_err(|e| CliError::invalid_input(format!("{what} is not valid JSON: {e}")))
}

pub(super) async fn guide(cx: &mut Ctx<'_>, cmd: GuideCmd) -> Result<(), CliError> {
    match cmd {
        GuideCmd::Start(a) => start(cx, a).await,
        GuideCmd::Stop { id } => {
            let mut c = cx.client().await?;
            let p = cx.mutating(json!({ "guide_id": id }), "");
            let r = c.call("stop_guide", strip_null(p)).await?;
            cx.emit(&r, show);
            Ok(())
        }
        GuideCmd::Get { id } => {
            let mut c = cx.client().await?;
            let r = c
                .call("get_guide", strip_null(json!({ "guide_id": id })))
                .await?;
            cx.emit(&r, show);
            Ok(())
        }
        GuideCmd::Set(a) => set(cx, a).await,
        GuideCmd::Export(a) => export(cx, a).await,
    }
}

fn strip_null(mut v: Value) -> Value {
    if let Some(o) = v.as_object_mut() {
        o.retain(|_, x| !x.is_null());
    }
    v
}

async fn start(cx: &mut Ctx<'_>, a: GuideStartArgs) -> Result<(), CliError> {
    let region = a.region.as_deref().map(parse_region).transpose()?;
    let mut c = cx.client().await?;
    let target = if let Some(w) = &a.window {
        let t = c.call("list_targets", json!({})).await?;
        json!({ "window_id": resolve_window(&t, w)? })
    } else {
        let display = match a.display {
            Some(d) => u64::from(d),
            None => primary_display(&c.call("list_targets", json!({ "windows": false })).await?)?,
        };
        match region {
            Some(rect) => json!({ "display_id": display, "rect": rect }),
            None => json!({ "display_id": display }),
        }
    };
    let mut p = Map::new();
    p.insert("target".into(), target);
    if let Some(t) = &a.title {
        p.insert("title".into(), json!(t));
    }
    if let Some(s) = a.countdown {
        p.insert("countdown_s".into(), json!(s));
    }
    let p = cx.mutating(Value::Object(p), "");
    let r = c.call("start_guide", p).await?;
    cx.emit(&r, |r| {
        format!(
            "Click Guide {} is {}. Click through the task, then run `capturemann guide stop`.",
            plain(&r["guide_id"]),
            plain(&r["state"])
        )
    });
    Ok(())
}

async fn set(cx: &mut Ctx<'_>, a: GuideSetArgs) -> Result<(), CliError> {
    let mut p = Map::new();
    let mut put = |k: &str, v: Value| {
        p.insert(k.to_owned(), v);
    };
    put("guide_id", json!(a.id));
    put("title", json!(a.title));
    put("intro", json!(a.intro));
    put("numbering", json!(a.numbering));
    if let Some(s) = &a.steps {
        let v = read_json_arg(cx, s, "--steps")?;
        put("steps", v);
    }
    if a.pause {
        put("paused", json!(true));
    }
    if a.resume {
        put("paused", json!(false));
    }
    if let Some(t) = &a.note_step {
        put(
            "note_step",
            json!({ "title": t, "note": a.note.clone().unwrap_or_default() }),
        );
    }
    if a.remove_last {
        put("remove_last", json!(true));
    }
    if let Some(on) = &a.focus_crop {
        put("focus_crop", json!(on == "on"));
        put("crop_steps", json!(a.crop_step));
    }
    if !a.retake.is_empty() {
        put("retake", json!(a.retake));
    }
    let p = cx.mutating(strip_null(Value::Object(p)), "");
    let mut c = cx.client().await?;
    let r = c.call("set_guide", p).await?;
    cx.emit(&r, show);
    Ok(())
}

async fn export(cx: &mut Ctx<'_>, a: GuideExportArgs) -> Result<(), CliError> {
    let mut p = Map::new();
    p.insert("guide_id".into(), json!(a.id));
    p.insert("format".into(), json!(a.format.replace('-', "_")));
    if let Some(d) = &a.dest {
        p.insert("dest".into(), json!(d));
    }
    if let Some(h) = &a.headings {
        p.insert("headings".into(), json!(h.replace('-', "_")));
    }
    if let Some(s) = &a.paper {
        p.insert("paper".into(), json!(s));
    }
    if let Some(s) = &a.image_scale {
        p.insert("image_scale".into(), json!(s.parse::<u32>().unwrap_or(1)));
    }
    if a.no_intro {
        p.insert("intro".into(), json!(false));
    }
    if a.no_notes {
        p.insert("notes".into(), json!(false));
    }
    if a.no_note_steps {
        p.insert("note_steps".into(), json!(false));
    }
    let p = cx.mutating(Value::Object(p), "");
    let mut c = cx.client().await?;
    let r = c.call("export_guide", p).await?;
    if !cx.json {
        for w in r["warnings"].as_array().into_iter().flatten() {
            let _ = writeln!(cx.env.err, "warning: {}", plain(w));
        }
    }
    cx.emit(&r, |r| match r["format"].as_str() {
        Some("rich_text") => format!(
            "{} step(s) {} the clipboard",
            plain(&r["steps"]),
            if r["copied_to_clipboard"] == true {
                "copied to"
            } else {
                "could not be copied to"
            }
        ),
        _ => format!(
            "{} step(s) written to {}",
            plain(&r["steps"]),
            plain(&r["path"])
        ),
    });
    Ok(())
}

/// A guide as text: title, state, then one line per step.
fn show(r: &Value) -> String {
    let mut s = String::new();
    if r["discarded"] == true {
        return "No step was taken, so no guide was kept.".to_owned();
    }
    let _ = writeln!(
        s,
        "{} ({}, {})  {}",
        plain(&r["title"]),
        plain(&r["state"]),
        plain(&r["guide_id"]),
        match r["step_count"].as_u64() {
            Some(1) => "1 step".to_owned(),
            Some(n) => format!("{n} steps"),
            None => String::new(),
        }
    );
    for st in r["steps"].as_array().into_iter().flatten() {
        let kind = if st["type"] == "note" { " (note)" } else { "" };
        let _ = writeln!(
            s,
            "  {}. {}{}  {}",
            plain(&st["label"]),
            plain(&st["title"]),
            kind,
            plain(&st["id"])
        );
    }
    s
}
