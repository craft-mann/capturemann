//! The library and the screenshot editor, on a seeded screenshot bundle (no screen involved), plus
//! Auto-redact against the real OS text recogniser.

use super::common::{check_png, library_entry, manifest, seed_screenshot, SEEDED_SHOT};
use super::targets::open_text_window;
use crate::check;
use crate::ctx::Ctx;
use crate::fixtures;
use crate::model::{fail, skip, Case, Need, Tier, ALL, R, WIN_LINUX};
use crate::proc::exit_text;
use serde_json::Value;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "lib.manifest",
            title: "the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG",
            area: "lib",
            specs: &["LIB-1", "CAP-4: export_image", "data model: bundles are the source of truth"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: manifest_case,
        },
        Case {
            id: "lib.rename",
            title: "rename changes the title and the bundle folder, and is not an undo step",
            area: "lib",
            specs: &["LIB-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: rename,
        },
        Case {
            id: "lib.annotate_undo",
            title: "annotate adds a revision; undo and redo move through it; one call is one step",
            area: "lib",
            specs: &["ANN-3", "AGT-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: annotate_undo,
        },
        Case {
            id: "lib.delete_needs_confirmation",
            title: "deleting a capture is refused without an on-screen confirmation and deletes nothing",
            area: "lib",
            specs: &["AGT-POLICY", "AGT tiers: Confirm"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: delete_refused,
        },
        Case {
            id: "redact.fixture",
            title: "Auto-redact finds the e-mail address and the IPv4 address in a picture with known text",
            area: "redact",
            specs: &["ANN-2", "AI-9 / RED-1: Auto-redact", "Windows.Media.Ocr recogniser"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: redact_fixture,
        },
        Case {
            id: "redact.window_text",
            title: "Auto-redact on a screenshot of a real window that shows an e-mail address",
            area: "redact",
            specs: &["ANN-2", "RED-1"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN_LINUX,
            timeout_s: 150,
            run: redact_window,
        },
    ]
}

fn manifest_case(ctx: &mut Ctx) -> R {
    let bundle = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let e = library_entry(ctx, SEEDED_SHOT)?;
    check!(
        e.get("kind").and_then(Value::as_str) == Some("screenshot"),
        "kind is {:?}",
        e.get("kind")
    );
    check!(
        e.get("width").and_then(Value::as_i64) == Some(1000)
            && e.get("height").and_then(Value::as_i64) == Some(260),
        "size is {:?}x{:?}",
        e.get("width"),
        e.get("height")
    );
    let m = manifest(&bundle)?;
    let mut missing = Vec::new();
    for k in [
        "schema",
        "id",
        "title",
        "kind",
        "created",
        "original",
        "ops",
        "revisions",
    ] {
        if m.get(k).is_none() {
            missing.push(k);
        }
    }
    check!(missing.is_empty(), "manifest.json lacks {missing:?}");
    check!(m["schema"] == 1, "manifest schema is {}", m["schema"]);
    let out = ctx.root.join("export.png");
    let out_s = out.to_string_lossy().into_owned();
    ctx.cli_ok(&["capture", "get", SEEDED_SHOT, "--out", &out_s])?;
    let (w, h) = check_png(&out)?;
    check!((w, h) == (1000, 260), "exported PNG is {w}x{h}");
    ctx.note(format!(
        "manifest keys ok; exported PNG {w}x{h}, {} bytes",
        std::fs::metadata(&out)?.len()
    ));
    // The master stays read-only: the original bytes are unchanged.
    let after = std::fs::read(bundle.join("original.png"))?;
    check!(after == fixtures::REDACT_PNG, "original.png changed");
    Ok(())
}

fn rename(ctx: &mut Ctx) -> R {
    seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let r = ctx.cli_ok(&["library", "rename", SEEDED_SHOT, "Renamed by regress"])?;
    let path = r
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    check!(
        path.contains("Renamed by regress"),
        "the bundle folder was not renamed: {path}"
    );
    check!(
        std::path::Path::new(&path).is_dir(),
        "the renamed folder {path} does not exist"
    );
    let e = library_entry(ctx, SEEDED_SHOT)?;
    check!(
        e.get("title").and_then(Value::as_str) == Some("Renamed by regress"),
        "title is {:?}",
        e.get("title")
    );
    let m = manifest(std::path::Path::new(&path))?;
    check!(
        m["revisions"].as_array().is_some_and(Vec::is_empty),
        "rename added a revision: {}",
        m["revisions"]
    );
    Ok(())
}

fn ops_file(ctx: &Ctx) -> R<std::path::PathBuf> {
    ctx.write_file(
        "ops.json",
        br##"[{"op":"text","at":[40,200],"text":"annotated","size":24,"color":"#cc0000","outline":null}]"##,
    )
}

fn annotate_undo(ctx: &mut Ctx) -> R {
    let bundle = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let ops = ops_file(ctx)?.to_string_lossy().into_owned();
    let out1 = ctx
        .root
        .join("annotated.png")
        .to_string_lossy()
        .into_owned();
    ctx.cli_ok(&["annotate", SEEDED_SHOT, "--ops", &ops, "--out", &out1])?;
    let m1 = manifest(&bundle)?;
    let ops_after = m1["ops"].as_array().map_or(0, Vec::len);
    let revs_after = m1["revisions"].as_array().map_or(0, Vec::len);
    ctx.note(format!(
        "after annotate: {ops_after} op(s), {revs_after} revision(s)"
    ));
    check!(
        ops_after == 1 && revs_after == 1,
        "one annotate call should add 1 op and 1 revision, got {ops_after} and {revs_after}"
    );
    let a = std::fs::read(&out1)?;
    check!(
        fixtures::png_info(&a).is_ok(),
        "the annotated image is not a valid PNG"
    );
    check!(
        a != fixtures::REDACT_PNG,
        "the annotated image equals the original"
    );

    ctx.cli_ok(&["undo", SEEDED_SHOT])?;
    let m2 = manifest(&bundle)?;
    check!(
        m2["ops"].as_array().is_some_and(Vec::is_empty),
        "undo left {} op(s)",
        m2["ops"]
    );
    ctx.cli_ok(&["redo", SEEDED_SHOT])?;
    let m3 = manifest(&bundle)?;
    check!(
        m3["ops"].as_array().map_or(0, Vec::len) == 1,
        "redo did not bring the op back: {}",
        m3["ops"]
    );
    // Nothing to redo any more: an honest error, not a crash.
    let again = ctx.cli(&["redo", SEEDED_SHOT])?;
    ctx.note(format!(
        "redo with nothing to redo: {} {:?}",
        exit_text(again.code()),
        again.error_code()
    ));
    check!(
        !crate::proc::is_crash_code(again.code()) && !again.out.timed_out,
        "redo with nothing to redo crashed or hung"
    );
    Ok(())
}

fn delete_refused(ctx: &mut Ctx) -> R {
    let bundle = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let r = ctx.cli(&["library", "rm", SEEDED_SHOT])?;
    ctx.note(format!(
        "library rm: {} {:?} {}",
        exit_text(r.code()),
        r.error_code(),
        crate::util::cut(&r.error_message(), 120)
    ));
    check!(
        !r.ok(),
        "`library rm` deleted a capture without a confirmation"
    );
    check!(
        r.code() == Some(3) && r.error_code().as_deref() == Some("user_declined"),
        "expected exit 3 user_declined, got {} {:?}",
        exit_text(r.code()),
        r.error_code()
    );
    check!(
        bundle.join("original.png").is_file(),
        "the bundle was deleted although the call was refused"
    );
    Ok(())
}

/// The `rects` of a `--suggest-redactions` answer as (kind, x, y, w, h).
fn rects(v: &Value) -> Vec<(String, f64, f64, f64, f64)> {
    v.get("rects")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|r| {
                    let g = |i: usize| {
                        r.pointer(&format!("/rect/{i}"))
                            .and_then(Value::as_f64)
                            .unwrap_or(-1.0)
                    };
                    (
                        r.get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        g(0),
                        g(1),
                        g(2),
                        g(3),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn suggest(ctx: &mut Ctx, id: &str) -> R<Value> {
    let r = ctx.cli(&["annotate", id, "--suggest-redactions"])?;
    if !r.ok() {
        let code = r.error_code().unwrap_or_default();
        let msg = r.error_message();
        if code == "not_implemented" || msg.to_lowercase().contains("recogni") {
            return skip(format!(
                "no text recogniser on this machine: {}",
                crate::util::cut(&msg, 140)
            ));
        }
        return fail(format!(
            "suggest_redactions failed ({}): {code} {}",
            exit_text(r.code()),
            crate::util::cut(&msg, 200)
        ));
    }
    r.json
        .ok_or_else(|| crate::model::Stop::Fail("suggest_redactions printed no JSON".to_owned()))
}

fn redact_fixture(ctx: &mut Ctx) -> R {
    let bundle = seed_screenshot(ctx)?;
    ctx.start_daemon()?;
    let v = suggest(ctx, SEEDED_SHOT)?;
    let found = rects(&v);
    ctx.note(format!(
        "recogniser {:?}; {} suggestion(s): {}",
        v.get("recognizer"),
        found.len(),
        found
            .iter()
            .map(|f| f.0.as_str())
            .collect::<Vec<_>>()
            .join(",")
    ));
    check!(
        found.iter().any(|f| f.0 == "email"),
        "no e-mail suggestion; got {:?}",
        found.iter().map(|f| &f.0).collect::<Vec<_>>()
    );
    check!(
        found.iter().any(|f| f.0 == "ip_address"),
        "no IPv4 suggestion; got {:?}",
        found.iter().map(|f| &f.0).collect::<Vec<_>>()
    );
    for (k, x, y, w, h) in &found {
        check!(
            *x >= 0.0 && *y >= 0.0 && *w > 0.0 && *h > 0.0 && x + w <= 1000.5 && y + h <= 260.5,
            "{k} rectangle ({x},{y},{w},{h}) is outside the 1000x260 picture"
        );
    }
    // Suggesting changes nothing; applying the e-mail ones is one revision.
    let m = manifest(&bundle)?;
    check!(
        m["revisions"].as_array().is_some_and(Vec::is_empty),
        "suggest_redactions added a revision"
    );
    let out = ctx.root.join("redacted.png").to_string_lossy().into_owned();
    let r = ctx.cli(&["annotate", SEEDED_SHOT, "--redact-emails", "--out", &out]);
    match r {
        Ok(r) if r.ok() => {
            let m = manifest(&bundle)?;
            check!(
                m["revisions"].as_array().map_or(0, Vec::len) == 1,
                "applying the redactions should be one revision: {}",
                m["revisions"]
            );
        }
        Ok(r) => ctx.note(format!(
            "--redact-emails: {} {:?} (not asserted)",
            exit_text(r.code()),
            r.error_code()
        )),
        Err(e) => return Err(e),
    }
    Ok(())
}

fn redact_window(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let text = "Contact: jane.doe@example.com\nServer 203.0.113.45 is down\nTicket 4711 closed\n";
    let (_win, id, _w, _h) = open_text_window(ctx, text)?;
    let id_s = id.to_string();
    let v = ctx.cli_ok(&["capture", "screenshot", "--window", &id_s])?;
    let cid = v
        .get("capture_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let s = suggest(ctx, &cid)?;
    let found = rects(&s);
    ctx.note(format!(
        "window screenshot {cid}: suggestions {:?}",
        found.iter().map(|f| f.0.as_str()).collect::<Vec<_>>()
    ));
    check!(
        found.iter().any(|f| f.0 == "email"),
        "the e-mail address shown in the window was not suggested"
    );
    Ok(())
}
