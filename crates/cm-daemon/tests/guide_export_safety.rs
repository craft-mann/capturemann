//! `export_guide` (release 0.3.5): BUG-2 (names with the local date and time, a guide with no step
//! is refused) and BUG-3 (a file that exists is never written over; an agent's export outside the
//! library and the user's export folders asks first, with the exact path). Scripted clicks, the
//! fake screen and a scripted confirmer: nothing real is clicked, shown or written outside a
//! temporary folder.

#![allow(clippy::unwrap_used)]

mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cm_core::CmError;
use cm_daemon::guide::ScriptedClicks;
use cm_daemon::{ConfirmationRequest, Confirmer, Daemon, Decision};
use cm_render::ocr::{TextLine, TextRecognizer};
use common::{builder, desktop, Client};
use image::RgbaImage;
use serde_json::{json, Value};

#[derive(Clone)]
struct NoText;

impl TextRecognizer for NoText {
    fn name(&self) -> &'static str {
        "none"
    }
    fn is_available(&self) -> bool {
        true
    }
    fn recognize(&self, _: &RgbaImage) -> Result<Vec<TextLine>, CmError> {
        Ok(Vec::new())
    }
}

/// Approves the start of a guide; answers an `export_guide` dialog with `export`, and keeps every
/// dialog it was shown.
struct Selective {
    export: Decision,
    shown: std::sync::Mutex<Vec<ConfirmationRequest>>,
}

impl Selective {
    fn shown(&self) -> Vec<ConfirmationRequest> {
        self.shown.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Confirmer for Selective {
    async fn confirm(&self, q: &ConfirmationRequest) -> Decision {
        self.shown.lock().unwrap().push(q.clone());
        if q.summary.contains("export_guide") {
            self.export
        } else {
            Decision::Approve
        }
    }
}

/// The handle the daemon owns while the test keeps its own.
struct Handle(Arc<Selective>);

#[async_trait::async_trait]
impl Confirmer for Handle {
    async fn confirm(&self, q: &ConfirmationRequest) -> Decision {
        self.0.confirm(q).await
    }
}

struct Rig {
    daemon: Daemon,
    clicks: ScriptedClicks,
    confirmer: Arc<Selective>,
    /// The library folder.
    dir: tempfile::TempDir,
}

fn rig(export: Decision) -> Rig {
    let (b, dir) = builder(desktop());
    let clicks = ScriptedClicks::new();
    let confirmer = Arc::new(Selective {
        export,
        shown: std::sync::Mutex::default(),
    });
    let daemon = b
        .guide_clicks(clicks.clone())
        .text_recognizer(NoText)
        .confirmer(Handle(confirmer.clone()))
        .build();
    Rig {
        daemon,
        clicks,
        confirmer,
        dir,
    }
}

fn ok(r: &Value) -> &Value {
    assert!(r.get("error").is_none(), "unexpected error: {r}");
    &r["result"]
}

/// Records a guide with `n` steps (the start asks; the rig approves it) and stops it.
async fn guide_with(r: &Rig, c: &mut Client, n: usize) -> String {
    let s = c
        .call(
            "start_guide",
            json!({"target": {"display_id": 1}, "countdown_s": 0, "title": "Set up"}),
        )
        .await;
    let id = ok(&s)["guide_id"].as_str().unwrap().to_owned();
    for i in 0..n {
        r.clicks.click(24.0 + i as f64 * 70.0, 28.0);
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }
    let done = c.call("stop_guide", json!({"guide_id": id})).await;
    assert_eq!(ok(&done)["steps"].as_array().unwrap().len(), n, "{done}");
    id
}

/// A folder outside the library, the home folder and every export folder.
fn elsewhere() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect()
}

/// The dialogs shown for `export_guide` (the start dialog is not counted).
fn export_dialogs(r: &Rig) -> Vec<ConfirmationRequest> {
    r.confirmer
        .shown()
        .into_iter()
        .filter(|q| q.summary.contains("export_guide"))
        .collect()
}

/// BUG-2: the Markdown and its pictures folder carry the local date and time, the links inside
/// point at that folder, and a second export leaves the first alone.
#[tokio::test]
async fn bug_2_markdown_names_carry_the_date_and_time_and_the_links_follow() {
    let r = rig(Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 2).await;
    let out = r.dir.path().join("exports");
    let params = json!({"guide_id": id, "format": "markdown", "dest": out});
    let first = c.call("export_guide", params.clone()).await;
    let first = ok(&first);
    let name = first["file"].as_str().unwrap().to_owned();
    // guide-YYYY-MM-DD-HHMMSS.md
    let b = name.as_bytes();
    assert_eq!(name.len(), 26, "{name}");
    assert!(
        name.starts_with("guide-") && name.ends_with(".md"),
        "{name}"
    );
    assert!(
        b[6..10].iter().all(u8::is_ascii_digit) && b[10] == b'-' && b[13] == b'-' && b[16] == b'-',
        "{name}"
    );
    let stamp = &name[6..name.len() - 3];
    let md = std::fs::read_to_string(out.join(&name)).unwrap();
    assert!(
        md.contains(&format!("(images-{stamp}/step-01.png)")),
        "{md}"
    );
    assert!(out.join(format!("images-{stamp}/step-02.png")).is_file());
    assert!(!out.join("images").exists() && !out.join("guide.md").exists());
    // Again, at once: other names, the first export untouched.
    let second = c.call("export_guide", params).await;
    let second = ok(&second)["file"].as_str().unwrap().to_owned();
    assert_ne!(second, name);
    assert_eq!(std::fs::read_to_string(out.join(&name)).unwrap(), md);
    assert_eq!(files_in(&out).len(), 4, "two files and two folders");
}

/// BUG-2: a guide with no step is refused, and nothing is written.
#[tokio::test]
async fn bug_2_a_guide_with_no_step_is_refused() {
    let r = rig(Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 1).await;
    let g = c.call("get_guide", json!({"guide_id": id})).await;
    let step = ok(&g)["steps"][0]["id"].as_str().unwrap().to_owned();
    ok(&c
        .call(
            "set_guide",
            json!({"guide_id": id, "steps": [{"id": step, "delete": true}]}),
        )
        .await);
    let out = r.dir.path().join("exports");
    for format in ["markdown", "html", "pdf"] {
        let dest = if format == "markdown" {
            out.clone()
        } else {
            out.join(format!("g.{format}"))
        };
        let res = c
            .call(
                "export_guide",
                json!({"guide_id": id, "format": format, "dest": dest}),
            )
            .await;
        assert!(res.get("error").is_some(), "{format}: {res}");
    }
    assert!(!out.exists() || files_in(&out).is_empty());
}

/// BUG-3: a destination that is a file already is refused, before any dialog, and the file stays.
#[tokio::test]
async fn bug_3_an_existing_file_is_never_written_over() {
    let r = rig(Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 1).await;
    let away = elsewhere();
    for (format, outside) in [("markdown", false), ("html", false), ("pdf", true)] {
        let base = if outside {
            away.path().to_owned()
        } else {
            r.dir.path().to_owned()
        };
        let file = base.join(format!("report-{format}.docx"));
        std::fs::write(&file, "mine").unwrap();
        let res = c
            .call(
                "export_guide",
                json!({"guide_id": id, "format": format, "dest": file}),
            )
            .await;
        assert!(res.get("error").is_some(), "{format}: {res}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    }
    assert!(
        export_dialogs(&r).is_empty(),
        "a refused call opens no dialog"
    );
}

/// BUG-3: inside the library an agent's export runs at once, with no dialog.
#[tokio::test]
async fn bug_3_inside_the_library_it_runs_without_a_dialog() {
    // Anything asked would be declined.
    let r = rig(Decision::Decline);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 1).await;
    let out = r.dir.path().join("exports/nested");
    for (format, dest) in [
        ("markdown", out.clone()),
        ("html", out.join("g.html")),
        ("pdf", out.join("g.pdf")),
    ] {
        let res = c
            .call(
                "export_guide",
                json!({"guide_id": id, "format": format, "dest": dest}),
            )
            .await;
        ok(&res);
    }
    assert!(export_dialogs(&r).is_empty(), "{:?}", r.confirmer.shown());
    // Rich text writes no file and never asks either.
    ok(&c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "rich_text"}),
        )
        .await);
    assert!(export_dialogs(&r).is_empty());
}

/// BUG-3: outside the library and the export folders it asks and names the exact path (a `..`
/// hop is judged by where it ends); an approval writes.
#[tokio::test]
async fn bug_3_outside_the_library_it_asks_with_the_exact_path() {
    let r = rig(Decision::Approve);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 1).await;
    let away = elsewhere();
    // Starts in the library, climbs out of it.
    let dest = r
        .dir
        .path()
        .join("inside")
        .join("..")
        .join("..")
        .join(away.path().file_name().unwrap());
    let res = c
        .call(
            "export_guide",
            json!({"guide_id": id, "format": "markdown", "dest": dest}),
        )
        .await;
    ok(&res);
    let asked = export_dialogs(&r);
    assert_eq!(asked.len(), 1, "{:?}", r.confirmer.shown());
    let shown = asked[0].details["path"].as_str().unwrap();
    assert!(!shown.contains(".."), "{shown}");
    assert_eq!(
        std::fs::canonicalize(shown).unwrap(),
        std::fs::canonicalize(away.path()).unwrap()
    );
    assert!(
        asked[0].summary.contains("always asks"),
        "{}",
        asked[0].summary
    );
    assert_eq!(files_in(away.path()).len(), 2, "approved: written");
}

/// BUG-3: a declined dialog writes nothing.
#[tokio::test]
async fn bug_3_a_declined_export_writes_nothing() {
    let r = rig(Decision::Decline);
    let mut c = Client::connect(&r.daemon, "agent");
    let id = guide_with(&r, &mut c, 1).await;
    let away = elsewhere();
    for format in ["markdown", "html"] {
        let dest = if format == "markdown" {
            away.path().to_owned()
        } else {
            away.path().join("g.html")
        };
        let res = c
            .call(
                "export_guide",
                json!({"guide_id": id, "format": format, "dest": dest}),
            )
            .await;
        assert!(res.get("error").is_some(), "{format}: {res}");
    }
    assert_eq!(export_dialogs(&r).len(), 2);
    assert!(files_in(away.path()).is_empty(), "nothing was written");
}
