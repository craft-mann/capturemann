//! Click Guide (GDE-1 to GDE-4) through the shipped CLI.
//!
//! `guide.edit_export` and `guide.refusals` need no screen, no pointer and no keyboard: the first
//! writes a guide bundle by hand (two image steps and a note step), then edits it (`guide set`,
//! `undo`, `redo`) and exports it in the three file formats; the second checks the error codes
//! and that a refused start leaves no guide behind. `guide.real_clicks` (Windows, hardware tier)
//! injects two real pointer clicks and checks that each one became a step with its click point.
//!
//! A package older than 0.3.4 has no `guide` command: the cases end SKIPPED with a message that
//! says so, never FAIL.

use crate::check;
use crate::ctx::{CliOut, Ctx};
use crate::fixtures;
use crate::model::{fail, skip, Case, Need, Stop, Tier, ALL, R, WIN};
use serde_json::Value;
use std::path::{Path, PathBuf};

const GUIDE_ID: &str = "guide_REGRESSFIXTURE01";

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "guide.edit_export",
            title: "a hand-written guide: get, reorder, retitle, undo and redo are one step each; Markdown (relative links, PNGs in order), HTML (self-contained) and PDF export",
            area: "guide",
            specs: &["GDE-2", "GDE-3", "GDE-3.AC1", "GDE-4"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: edit_export,
        },
        Case {
            id: "guide.refusals",
            title: "an unknown guide is not_found, a bad numbering or format is refused, and a start on a display that does not exist leaves no guide in the library",
            area: "guide",
            specs: &["GDE-4", "GDE-1.AC5", "AGT-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 150,
            run: refusals,
        },
        Case {
            id: "guide.real_clicks",
            title: "two injected pointer clicks become two steps with their click points in physical px; no keystroke is recorded; Markdown export holds two images",
            area: "guide",
            specs: &["GDE-1", "GDE-1.AC1", "GDE-1.AC2", "GDE-3.AC1", "NFR-9"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN,
            timeout_s: 300,
            run: real_clicks,
        },
    ]
}

/// Writes `Guide.cmguide` into the sandbox library: steps `a` and `b` carry the committed text
/// picture, `c` is a note step. Call before `start_daemon`.
fn seed_guide(ctx: &Ctx) -> R<PathBuf> {
    let dir = ctx.library.join("Fixture guide.cmguide");
    let steps = dir.join("steps");
    let sha = crate::util::sha256_hex(fixtures::REDACT_PNG);
    for (name, id) in [
        ("step_1", "shot_REGRESSGUIDESTEP1"),
        ("step_2", "shot_REGRESSGUIDESTEP2"),
    ] {
        let b = steps.join(format!("{name}.cmshot"));
        std::fs::create_dir_all(&b)?;
        std::fs::write(b.join("original.png"), fixtures::REDACT_PNG)?;
        let m = format!(
            r#"{{"schema":1,"id":"{id}","title":"{name}","kind":"screenshot","created":"2026-11-14T07:00:00Z","original":{{"file":"original.png","size":[1000,260],"bytes":{},"sha256":"{sha}"}},"render":null,"ops":[],"op_seq":0,"revisions":[],"history":{{"undo":[],"redo":[]}}}}"#,
            fixtures::REDACT_PNG.len()
        );
        std::fs::write(b.join("manifest.json"), m)?;
    }
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            r#"{{"schema":1,"id":"{GUIDE_ID}","title":"Fixture guide","kind":"guide","created":"2026-11-14T07:00:00Z"}}"#
        ),
    )?;
    let doc = r#"{"schema":1,"title":"Fixture guide","intro":"Three steps.","numbering":"number","target":{"kind":"display","display_id":1},"step_seq":3,"steps":[
      {"id":"step_1","type":"image","title":"Open the page","note":"","image":"step_1.cmshot","click":{"x":300,"y":120,"button":"left","count":1},"focus_crop":false,"has_proposals":false},
      {"id":"step_2","type":"image","title":"Press Save","note":"Wait for the check mark.","image":"step_2.cmshot","click":{"x":640,"y":200,"button":"left","count":1},"focus_crop":false,"has_proposals":false},
      {"id":"step_3","type":"note","title":"Sign out","note":""}],"undo":[],"redo":[]}"#;
    std::fs::write(dir.join("guide.json"), doc)?;
    Ok(dir)
}

/// The step ids of a `guide get` answer, in order.
fn step_ids(v: &Value) -> Vec<String> {
    v.get("steps")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The titles of a `guide get` answer, in order.
fn step_titles(v: &Value) -> Vec<String> {
    v.get("steps")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|s| s.get("title").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// A package with no `guide` command (older than 0.3.4).
fn no_guide_command(r: &CliOut) -> bool {
    let t = r.out.text().to_lowercase();
    !r.ok() && (t.contains("unrecognized subcommand") || t.contains("unknown command"))
}

fn edit_export(ctx: &mut Ctx) -> R {
    seed_guide(ctx)?;
    ctx.start_daemon()?;
    let first = ctx.cli(&["guide", "get", GUIDE_ID])?;
    if no_guide_command(&first) {
        return skip("this package has no `guide` command (needs 0.3.4 or later)");
    }
    check!(
        first.ok(),
        "`guide get` failed ({}): {}",
        crate::proc::exit_text(first.code()),
        crate::util::cut(&crate::util::one_line(&first.out.text()), 300)
    );
    let v = first.json.clone().unwrap_or(Value::Null);
    check!(
        step_ids(&v) == ["step_1", "step_2", "step_3"],
        "the seeded guide lists {:?}",
        step_ids(&v)
    );

    // Reorder and retitle in one call: one undo step.
    let steps = r#"[{"id":"step_2","title":"Save the page"},{"id":"step_1"},{"id":"step_3"}]"#;
    ctx.cli_ok(&["guide", "set", GUIDE_ID, "--steps", steps])?;
    let v = ctx.cli_ok(&["guide", "get", GUIDE_ID])?;
    check!(
        step_ids(&v) == ["step_2", "step_1", "step_3"],
        "after the reorder the steps are {:?}",
        step_ids(&v)
    );
    check!(
        step_titles(&v)[0] == "Save the page",
        "the retitle was not kept: {:?}",
        step_titles(&v)
    );
    // One undo brings back both the order and the title.
    ctx.cli_ok(&["undo", GUIDE_ID])?;
    let v = ctx.cli_ok(&["guide", "get", GUIDE_ID])?;
    check!(
        step_ids(&v) == ["step_1", "step_2", "step_3"] && step_titles(&v)[1] == "Press Save",
        "one undo did not restore the order and the title: {:?} {:?}",
        step_ids(&v),
        step_titles(&v)
    );
    ctx.cli_ok(&["redo", GUIDE_ID])?;
    let v = ctx.cli_ok(&["guide", "get", GUIDE_ID])?;
    check!(
        step_ids(&v) == ["step_2", "step_1", "step_3"],
        "redo did not bring the reorder back: {:?}",
        step_ids(&v)
    );

    // Markdown: guide.md and images/, relative links, the two PNGs in the guide's order.
    let md_dir = ctx.root.join("guide-md");
    let md = md_dir.to_string_lossy().into_owned();
    ctx.cli_ok(&[
        "guide", "export", GUIDE_ID, "--format", "markdown", "--dest", &md,
    ])?;
    let guide_md = find_file(&md_dir, "guide.md")
        .ok_or_else(|| Stop::Fail(format!("no guide.md under {}", md_dir.display())))?;
    let text = std::fs::read_to_string(&guide_md)?;
    let images = images_under(guide_md.parent().unwrap_or(&md_dir));
    check!(
        images.len() == 2,
        "the Markdown export has {} images, expected 2 (the note step has none): {images:?}",
        images.len()
    );
    for i in &images {
        crate::cases::common::check_png(i)?;
    }
    check!(
        text.matches("](images/").count() == 2,
        "guide.md does not link its two images with relative `images/` paths"
    );
    check!(
        !text.contains(":\\") && !text.contains("](/") && !text.contains("file://"),
        "guide.md holds an absolute path"
    );
    let (a, b) = (text.find("Save the page"), text.find("Open the page"));
    check!(
        matches!((a, b), (Some(a), Some(b)) if a < b),
        "guide.md does not list the steps in the new order"
    );

    // HTML: one file, images embedded, light and dark styles.
    let html_path = ctx.root.join("guide.html");
    let html_arg = html_path.to_string_lossy().into_owned();
    ctx.cli_ok(&[
        "guide", "export", GUIDE_ID, "--format", "html", "--dest", &html_arg,
    ])?;
    let html = std::fs::read_to_string(&html_path)?;
    check!(
        html.matches("data:image/png;base64,").count() >= 2,
        "the HTML export does not embed its two images"
    );
    check!(
        html.contains("prefers-color-scheme"),
        "the HTML export has no dark style"
    );
    check!(
        !html.contains("src=\"images/") && !html.contains("src=\"file:"),
        "the HTML export links an image file instead of embedding it"
    );

    // PDF: a PDF file with a page.
    let pdf_path = ctx.root.join("guide.pdf");
    let pdf_arg = pdf_path.to_string_lossy().into_owned();
    ctx.cli_ok(&[
        "guide", "export", GUIDE_ID, "--format", "pdf", "--dest", &pdf_arg,
    ])?;
    let pdf = std::fs::read(&pdf_path)?;
    check!(
        pdf.starts_with(b"%PDF-") && pdf.len() > 2000,
        "the PDF export is not a PDF ({} bytes)",
        pdf.len()
    );
    Ok(())
}

fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    Ctx::files_under(dir)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == name))
}

fn images_under(dir: &Path) -> Vec<PathBuf> {
    Ctx::files_under(dir)
        .into_iter()
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .collect()
}

fn refusals(ctx: &mut Ctx) -> R {
    seed_guide(ctx)?;
    ctx.start_daemon()?;
    let probe = ctx.cli(&["guide", "get", GUIDE_ID])?;
    if no_guide_command(&probe) {
        return skip("this package has no `guide` command (needs 0.3.4 or later)");
    }
    let r = ctx.cli(&["guide", "get", "guide_DOESNOTEXIST0000000"])?;
    check!(
        !r.ok() && r.error_code().as_deref() == Some("not_found"),
        "`guide get` on an unknown id answered {} / {:?}",
        crate::proc::exit_text(r.code()),
        r.error_code()
    );
    let r = ctx.cli(&["guide", "set", GUIDE_ID, "--numbering", "roman"])?;
    check!(!r.ok(), "`guide set --numbering roman` was accepted");
    let r = ctx.cli(&["guide", "export", GUIDE_ID, "--format", "docx"])?;
    check!(!r.ok(), "`guide export --format docx` was accepted");
    let r = ctx.cli(&[
        "guide",
        "set",
        GUIDE_ID,
        "--steps",
        "[{\"id\":\"step_99\"}]",
    ])?;
    check!(
        !r.ok()
            && matches!(
                r.error_code().as_deref(),
                Some("invalid_input" | "not_found")
            ),
        "`guide set` naming a step that does not exist answered {:?}",
        r.error_code()
    );
    // The guide is unchanged by every refusal.
    let v = ctx.cli_ok(&["guide", "get", GUIDE_ID])?;
    check!(
        step_ids(&v) == ["step_1", "step_2", "step_3"],
        "a refused call changed the guide: {:?}",
        step_ids(&v)
    );
    // A start on a display that does not exist is refused before anything listens, and no
    // new guide folder is left in the library.
    let before = ctx.bundles("cmguide").len();
    let r = ctx.cli(&["guide", "start", "--display", "987654", "--countdown", "0"])?;
    if r.ok() {
        // It should never get here; do not leave a listener behind.
        let _ = ctx.cli(&["guide", "stop"]);
        return fail(
            "`guide start --display 987654` started a guide on a display that does not exist",
        );
    }
    let after = ctx.bundles("cmguide").len();
    check!(
        after == before,
        "a refused start left a guide folder behind ({before} before, {after} after)"
    );
    Ok(())
}

/// Windows, hardware tier: two real clicks (injected input) on the primary display.
fn real_clicks(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let probe = ctx.cli(&["guide", "get"])?;
    if no_guide_command(&probe) {
        return skip("this package has no `guide` command (needs 0.3.4 or later)");
    }
    let d = ctx.primary_display()?;
    let id = d.id.to_string();
    let started = ctx.cli(&[
        "guide",
        "start",
        "--display",
        &id,
        "--countdown",
        "0",
        "--title",
        "Regress guide",
    ])?;
    check!(
        started.ok(),
        "`guide start` failed ({}): {}",
        crate::proc::exit_text(started.code()),
        crate::util::cut(&crate::util::one_line(&started.out.text()), 300)
    );
    let guide_id = started
        .json
        .as_ref()
        .and_then(|v| v.get("guide_id"))
        .and_then(Value::as_str)
        .ok_or_else(|| Stop::Fail("`guide start` printed no guide_id".to_owned()))?
        .to_owned();
    // Two points well inside the display, away from its edges and the taskbar.
    let p1 = (d.x as i32 + (d.w as i32) / 2, d.y as i32 + (d.h as i32) / 3);
    let p2 = (p1.0 + 160, p1.1 + 90);
    std::thread::sleep(std::time::Duration::from_secs(1));
    for p in [p1, p2] {
        if let Err(e) = crate::sys::drag_mouse(p, p) {
            let _ = ctx.cli(&["guide", "stop"]);
            return skip(format!("a pointer click cannot be injected: {e}"));
        }
        std::thread::sleep(std::time::Duration::from_millis(1200));
    }
    let stopped = ctx.cli_ok(&["guide", "stop"])?;
    let v = ctx.cli_ok(&["guide", "get", &guide_id])?;
    let n = v.get("steps").and_then(Value::as_array).map_or(0, Vec::len);
    check!(
        n == 2,
        "two clicks made {n} steps (stop said {})",
        crate::util::cut(&stopped.to_string(), 200)
    );
    let steps = v["steps"].as_array().cloned().unwrap_or_default();
    for (s, p) in steps.iter().zip([p1, p2]) {
        let (Some(x), Some(y)) = (
            s.pointer("/click/x").and_then(Value::as_f64),
            s.pointer("/click/y").and_then(Value::as_f64),
        ) else {
            return fail(format!("a step has no click point: {s}"));
        };
        // The click point is in physical px of the step's frame (the display origin removed).
        let (ex, ey) = (f64::from(p.0) - d.x as f64, f64::from(p.1) - d.y as f64);
        check!(
            (x - ex).abs() <= 4.0 && (y - ey).abs() <= 4.0,
            "a step's click point is ({x:.0}, {y:.0}), the injected click was at ({ex:.0}, {ey:.0}) physical px"
        );
    }
    let md_dir = ctx.root.join("guide-real-md");
    let md = md_dir.to_string_lossy().into_owned();
    ctx.cli_ok(&[
        "guide", "export", &guide_id, "--format", "markdown", "--dest", &md,
    ])?;
    let images = images_under(&md_dir);
    check!(
        images.len() == 2,
        "the Markdown export of the two-click guide has {} images",
        images.len()
    );
    Ok(())
}
