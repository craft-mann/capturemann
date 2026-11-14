//! Boards (BRD-1, `create_board`) and Bulk export (BLK-1, `export_images`) through the shipped
//! CLI on imported test pictures. No screen, camera or microphone is involved.
//!
//! Both cases need a 0.3.4 build. With an older package they end SKIPPED with a message that says
//! so, never FAIL.

use super::common::{check_png, import_file, library_entry, manifest, seed_screenshot};
use crate::check;
use crate::ctx::Ctx;
use crate::fixtures;
use crate::model::{skip, Case, Stop, Tier, ALL, R};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "board.create",
            title: "a Pair board and a Free board of imported pictures are new screenshot captures with the right size and layout; the sources are not changed",
            area: "board",
            specs: &["BRD-1", "AGT-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 300,
            run: create,
        },
        Case {
            id: "bulk.export_images",
            title: "export-images resizes, converts and numbers files without overwriting; the Blueprint step changes the file and never the capture",
            area: "bulk",
            specs: &["BLK-1", "ANN-11", "AGT-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 300,
            run: export_images,
        },
    ]
}

fn generated(w: u32, h: u32, tint: u8) -> Vec<u8> {
    fixtures::make_png(w, h, |x, y| {
        [(x * 255 / w.max(1)) as u8, (y * 255 / h.max(1)) as u8, tint]
    })
}

/// Finds the `.cmshot` bundle of a capture id.
fn shot_bundle(ctx: &Ctx, id: &str) -> R<PathBuf> {
    for b in ctx.bundles("cmshot") {
        if manifest(&b)?["id"] == id {
            return Ok(b);
        }
    }
    Err(Stop::Fail(format!("no bundle holds {id}")))
}

fn sha_of(path: &Path) -> R<String> {
    let b = std::fs::read(path).map_err(|e| Stop::Fail(format!("{}: {e}", path.display())))?;
    Ok(crate::util::sha256_hex(&b))
}

/// The words an older build gives for a command or tool it does not have.
fn old_build(text: &str) -> bool {
    let t = text.to_lowercase();
    [
        "unrecognized subcommand",
        "unknown subcommand",
        "unknown method",
        "method not found",
        "create_board",
        "export_images",
    ]
    .iter()
    .any(|w| t.contains(w))
}

fn create(ctx: &mut Ctx) -> R {
    let dir = ctx.dir("board-src");
    std::fs::write(dir.join("a.png"), generated(160, 100, 40))?;
    std::fs::write(dir.join("b.png"), generated(120, 120, 200))?;
    ctx.start_daemon()?;
    let (a, _) = import_file(ctx, &dir.join("a.png"))?;
    let (b, _) = import_file(ctx, &dir.join("b.png"))?;
    let before = (
        sha_of(&shot_bundle(ctx, &a)?.join("original.png"))?,
        sha_of(&shot_bundle(ctx, &b)?.join("original.png"))?,
    );

    // Pair: one row, same height, BEFORE and AFTER labels.
    let r = ctx.cli(&[
        "board",
        &a,
        &b,
        "--layout",
        "pair",
        "--gap",
        "20",
        "--margin",
        "10",
        "--before-after",
        "--name",
        "Regress pair",
    ])?;
    if !r.ok() {
        let text = format!(
            "{} {}",
            r.error_message(),
            r.error.as_ref().map(Value::to_string).unwrap_or_default()
        );
        if old_build(&text) || r.out.text().to_lowercase().contains("unrecognized") {
            return skip(format!(
                "this package has no boards (needs 0.3.4 or later): {}",
                crate::util::cut(&text, 200)
            ));
        }
        return Err(Stop::Fail(format!(
            "`capturemann board` failed ({}): {}",
            crate::proc::exit_text(r.code()),
            crate::util::cut(&text, 300)
        )));
    }
    let v = r.json.clone().unwrap_or(Value::Null);
    let id = v["capture_id"].as_str().unwrap_or("").to_owned();
    check!(!id.is_empty(), "the board call returned no capture id: {v}");
    let (w, h) = (
        v["width"].as_u64().unwrap_or(0),
        v["height"].as_u64().unwrap_or(0),
    );
    // Both pictures at one height (the shorter one's), a row with the gap and the margin.
    check!(
        w > 200 && h >= 100,
        "the pair board is {w}x{h}, too small for its pictures"
    );
    let entry = library_entry(ctx, &id)?;
    check!(
        entry["kind"] == "screenshot" && entry["board"] == "pair",
        "the board's library row: {entry}"
    );
    let bundle = shot_bundle(ctx, &id)?;
    let dims = check_png(&bundle.join("original.png"))?;
    check!(
        u64::from(dims.0) == w && u64::from(dims.1) == h,
        "original.png is {dims:?}, the call said {w}x{h}"
    );
    let m = manifest(&bundle)?;
    check!(
        m["source"] == "board"
            && m["board"]["layout"]["kind"] == "pair"
            && m["board"]["sources"].as_array().map_or(0, Vec::len) == 2,
        "the board manifest does not record its layout and sources: {}",
        m["board"]
    );
    ctx.note(format!(
        "Pair board {id}: {w}x{h}, layout and 2 sources stored"
    ));

    // Free: two rectangles; the canvas is their extent plus the margin.
    let r = ctx.cli_ok(&[
        "board",
        &a,
        &b,
        "--layout",
        "free",
        "--slot",
        "0,0,120,80",
        "--slot",
        "150,10,60,40",
        "--margin",
        "10",
        "--name",
        "Regress free",
    ])?;
    let (fw, fh) = (
        r["width"].as_u64().unwrap_or(0),
        r["height"].as_u64().unwrap_or(0),
    );
    check!(
        (fw, fh) == (220, 90),
        "the Free board is {fw}x{fh}, expected 220x90 (extent plus a 10 px margin)"
    );
    ctx.note("Free board with 2 slots: 220x90");

    // One picture is not a board; the sources are exactly as they were.
    let bad = ctx.cli(&["board", &a, "--layout", "grid"])?;
    check!(!bad.ok(), "a board of one picture was accepted");
    let after = (
        sha_of(&shot_bundle(ctx, &a)?.join("original.png"))?,
        sha_of(&shot_bundle(ctx, &b)?.join("original.png"))?,
    );
    check!(
        before == after,
        "a source picture changed while making boards"
    );
    Ok(())
}

fn export_images(ctx: &mut Ctx) -> R {
    let dir = ctx.dir("bulk-src");
    std::fs::write(dir.join("wide.png"), generated(240, 120, 90))?;
    std::fs::write(dir.join("tall.png"), generated(100, 200, 160))?;
    let seeded = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let (a, _) = import_file(ctx, &dir.join("wide.png"))?;
    let (b, _) = import_file(ctx, &dir.join("tall.png"))?;
    let originals: Vec<(PathBuf, String)> = [&a, &b]
        .iter()
        .map(|id| {
            let p = shot_bundle(ctx, id)?.join("original.png");
            let s = sha_of(&p)?;
            Ok((p, s))
        })
        .collect::<R<_>>()?;
    let out = ctx.dir("bulk-out");
    let out_s = out.to_string_lossy().into_owned();

    let run = |ctx: &mut Ctx, extra: &[&str], ids: &[&str]| -> R<crate::ctx::CliOut> {
        let mut args = vec!["export-images"];
        args.extend_from_slice(ids);
        args.extend_from_slice(&["--dest", &out_s, "--wait", "--timeout", "120"]);
        args.extend_from_slice(extra);
        ctx.cli_long(&args, 150)
    };
    let r = run(
        ctx,
        &[
            "--longest",
            "60",
            "--format",
            "jpeg",
            "--name-pattern",
            "{name}-{n}",
        ],
        &[&a, &b],
    )?;
    if !r.ok() {
        let text = format!(
            "{} {}",
            r.error_message(),
            r.error.as_ref().map(Value::to_string).unwrap_or_default()
        );
        if old_build(&text) || r.out.text().to_lowercase().contains("unrecognized") {
            return skip(format!(
                "this package cannot export images in bulk (needs 0.3.4 or later): {}",
                crate::util::cut(&text, 200)
            ));
        }
        return Err(Stop::Fail(format!(
            "`capturemann export-images` failed ({}): {}",
            crate::proc::exit_text(r.code()),
            crate::util::cut(&text, 300)
        )));
    }
    let job = r.json.clone().unwrap_or(Value::Null);
    check!(
        job["result"]["exported"] == 2 && job["result"]["skipped"] == 0,
        "expected 2 exported and 0 skipped: {}",
        crate::util::cut(&job.to_string(), 400)
    );
    let mut dims = Vec::new();
    for f in job["result"]["files"].as_array().into_iter().flatten() {
        let p = PathBuf::from(f["path"].as_str().unwrap_or(""));
        let bytes = std::fs::read(&p).map_err(|e| Stop::Fail(format!("{}: {e}", p.display())))?;
        check!(
            bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
            "{} is not a JPEG",
            p.display()
        );
        dims.push((
            f["width"].as_u64().unwrap_or(0),
            f["height"].as_u64().unwrap_or(0),
        ));
    }
    check!(
        dims == [(60, 30), (30, 60)],
        "the longest side should be 60 px with the proportions kept, got {dims:?}"
    );
    ctx.note("2 JPEG files, longest side 60 px, proportions kept");

    // The same call again: nothing is overwritten, the names are numbered.
    let before_names = file_names(&out);
    let r = run(
        ctx,
        &[
            "--longest",
            "60",
            "--format",
            "jpeg",
            "--name-pattern",
            "{name}-{n}",
        ],
        &[&a, &b],
    )?;
    check!(r.ok(), "the second export failed");
    let after_names = file_names(&out);
    check!(
        before_names.len() == 2 && after_names.len() == 4,
        "a second export should add 2 numbered files, have {before_names:?} then {after_names:?}"
    );
    ctx.note(format!("second run kept the first files: {after_names:?}"));

    // Blueprint: the seeded picture holds text lines. With a text recognizer the file differs from
    // the plain export and has the same size; without one the call fails before a file is written.
    let plain = run(ctx, &["--name-pattern", "plain"], &[common_seed_id()])?;
    check!(plain.ok(), "the plain export of the text picture failed");
    let files = file_names(&out).len();
    let bp = run(
        ctx,
        &["--blueprint", "--name-pattern", "bars"],
        &[common_seed_id()],
    )?;
    if bp.ok() {
        let (p, q) = (out.join("plain.png"), out.join("bars.png"));
        let dp = check_png(&p)?;
        let dq = check_png(&q)?;
        check!(
            dp == dq,
            "the Blueprint file is {dq:?}, the plain one {dp:?}"
        );
        check!(
            sha_of(&p)? != sha_of(&q)?,
            "the Blueprint step left the text picture unchanged: no text line was replaced"
        );
        ctx.note("Blueprint export differs from the plain export, same size");
    } else {
        let text = format!(
            "{} {}",
            bp.error_message(),
            bp.error.as_ref().map(Value::to_string).unwrap_or_default()
        );
        check!(
            text.to_lowercase().contains("recogni") && file_names(&out).len() == files,
            "Blueprint without a text recognizer should fail with that reason and write nothing: {}",
            crate::util::cut(&text, 300)
        );
        ctx.note("no text recognizer on this machine: Blueprint refused before writing a file");
    }

    // The captures themselves never change.
    for (p, sha) in &originals {
        check!(&sha_of(p)? == sha, "{} changed", p.display());
    }
    check!(
        manifest(&shot_bundle(ctx, &a)?)?["ops"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "an export left ops in the capture"
    );
    let _ = seeded;
    Ok(())
}

fn common_seed_id() -> &'static str {
    super::common::SEEDED_SHOT
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}
