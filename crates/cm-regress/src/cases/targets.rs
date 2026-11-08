//! Displays, windows and screenshots: sizes, names and the saved file against the listing.

use super::common::{check_png, library_entry};
use crate::check;
use crate::ctx::{displays, Ctx};
use crate::model::{fail, skip, Case, Need, Tier, ALL, R, WIN_LINUX};
use crate::sys;
use serde_json::Value;
use std::collections::BTreeSet;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "targets.list",
            title: "targets lists displays and windows with readable names and real sizes",
            area: "targets",
            specs: &["CAP-1", "CAP-3", "bug: Windows Terminal Host name", "bug: window name garbled"],
            tier: Tier::Smoke,
            needs: &[Need::DesktopSession],
            platforms: ALL,
            timeout_s: 90,
            run: list,
        },
        Case {
            id: "shot.display",
            title: "a display screenshot is a valid PNG of the display's physical size",
            area: "shot",
            specs: &["CAP-1.AC1"],
            tier: Tier::Smoke,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 90,
            run: display,
        },
        Case {
            id: "shot.region",
            title: "a region screenshot has the requested size",
            area: "shot",
            specs: &["CAP-1.AC2"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 90,
            run: region,
        },
        Case {
            id: "shot.window",
            title: "a window screenshot is saved at the size the window is listed with",
            area: "shot",
            specs: &["CAP-1.AC4", "bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware)"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: WIN_LINUX,
            timeout_s: 120,
            run: window,
        },
        Case {
            id: "shot.second_monitor",
            title: "every extra monitor lists and captures at its own physical size",
            area: "shot",
            specs: &["CAP-1.AC1", "DPI-WIN layout A"],
            tier: Tier::Hardware,
            needs: &[Need::DesktopSession, Need::Screen, Need::SecondMonitor],
            platforms: ALL,
            timeout_s: 120,
            run: second_monitor,
        },
        Case {
            id: "shot.invalid",
            title: "bad targets are refused with the shared error codes and create no capture",
            area: "shot",
            specs: &["CAP-1", "errors: invalid_input, not_found"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: ALL,
            timeout_s: 90,
            run: invalid,
        },
    ]
}

fn list(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["targets"])?;
    let ds = displays(&v);
    check!(!ds.is_empty(), "no display listed");
    check!(
        ds.iter().filter(|d| d.primary).count() == 1,
        "expected exactly one primary display, got {}",
        ds.iter().filter(|d| d.primary).count()
    );
    let mut ids = BTreeSet::new();
    for d in &ds {
        ctx.note(format!(
            "display {} '{}' {}x{} at ({},{}) scale {}{}",
            d.id,
            d.name,
            d.w,
            d.h,
            d.x,
            d.y,
            d.scale,
            if d.primary { " primary" } else { "" }
        ));
        check!(ids.insert(d.id), "display id {} is listed twice", d.id);
        check!(
            d.w > 0 && d.h > 0,
            "display {} has size {}x{}",
            d.id,
            d.w,
            d.h
        );
        check!(d.scale > 0.0, "display {} has scale {}", d.id, d.scale);
        check!(
            !d.name.trim().is_empty() && !d.name.contains('\u{FFFD}'),
            "display {} has an unreadable name '{}'",
            d.id,
            d.name
        );
    }
    let windows = v
        .get("windows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let caps = v.get("capabilities").cloned().unwrap_or_default();
    ctx.note(format!("{} windows; capabilities {caps}", windows.len()));
    let mut problems = Vec::new();
    for w in windows
        .iter()
        .filter(|w| w.get("minimized").and_then(Value::as_bool) != Some(true))
    {
        let id = w.get("id").and_then(Value::as_u64).unwrap_or(0);
        let app = w.get("app").and_then(Value::as_str).unwrap_or("");
        let title = w.get("title").and_then(Value::as_str).unwrap_or("");
        let size = w.pointer("/bounds/size").and_then(Value::as_array);
        let (ww, wh) = (
            size.and_then(|s| s.first())
                .and_then(Value::as_i64)
                .unwrap_or(0),
            size.and_then(|s| s.get(1))
                .and_then(Value::as_i64)
                .unwrap_or(0),
        );
        if id == 0 {
            problems.push("a window has no id".to_owned());
        }
        if ww <= 0 || wh <= 0 {
            problems.push(format!(
                "window {id} '{}' has size {ww}x{wh}",
                crate::util::cut(title, 30)
            ));
        }
        if app.trim().is_empty() {
            problems.push(format!(
                "window {id} '{}' has no app name",
                crate::util::cut(title, 30)
            ));
        }
        if app.contains('\u{FFFD}') || title.contains('\u{FFFD}') {
            problems.push(format!("window {id} has an unreadable name"));
        }
        if app.contains("Terminal Host")
            || app.eq_ignore_ascii_case("WindowsTerminal")
            || app.eq_ignore_ascii_case("OpenConsole")
        {
            problems.push(format!(
                "window {id} is named '{app}' (Windows Terminal should read as 'Windows Terminal')"
            ));
        }
    }
    let apps: BTreeSet<&str> = windows
        .iter()
        .filter_map(|w| w.get("app").and_then(Value::as_str))
        .collect();
    ctx.note(format!(
        "apps: {}",
        crate::util::cut(&apps.iter().copied().collect::<Vec<_>>().join(", "), 300)
    ));
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn display(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let out = ctx.root.join("display.png");
    let out_s = out.to_string_lossy().into_owned();
    let id = d.id.to_string();
    let v = ctx.cli_ok(&[
        "capture",
        "screenshot",
        "--display",
        &id,
        "--out",
        &out_s,
        "--title",
        "regress display",
    ])?;
    let (w, h) = check_png(&out)?;
    ctx.note(format!(
        "display {} {}x{}; PNG {w}x{h}; capture {}",
        d.id,
        d.w,
        d.h,
        v.get("capture_id").and_then(Value::as_str).unwrap_or("?")
    ));
    check!(
        i64::from(w) == d.w && i64::from(h) == d.h,
        "the PNG is {w}x{h} but display {} is {}x{} physical",
        d.id,
        d.w,
        d.h
    );
    let cid = v
        .get("capture_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let e = library_entry(ctx, &cid)?;
    check!(
        e.get("kind").and_then(Value::as_str) == Some("screenshot"),
        "the library lists the capture as {:?}",
        e.get("kind")
    );
    check!(
        e.get("width").and_then(Value::as_i64) == Some(d.w)
            && e.get("height").and_then(Value::as_i64) == Some(d.h),
        "the library size differs from the PNG: {e}"
    );
    Ok(())
}

fn region(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let (rw, rh) = (400.min(d.w - 100), 300.min(d.h - 100));
    check!(
        rw > 50 && rh > 50,
        "the display is too small for a test region"
    );
    let id = d.id.to_string();
    let region = format!("100,100,{rw},{rh}");
    let out = ctx.root.join("region.png");
    let out_s = out.to_string_lossy().into_owned();
    ctx.cli_ok(&[
        "capture",
        "screenshot",
        "--display",
        &id,
        "--region",
        &region,
        "--out",
        &out_s,
    ])?;
    let (w, h) = check_png(&out)?;
    ctx.note(format!("region {region}: PNG {w}x{h}"));
    check!(
        i64::from(w) == rw && i64::from(h) == rh,
        "the region PNG is {w}x{h}, expected {rw}x{rh}"
    );
    Ok(())
}

/// A text window opened by the suite itself (Notepad on Windows), closed when dropped.
pub struct TestWindow {
    pub pids: Vec<u32>,
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        for p in &self.pids {
            sys::kill_tree(*p);
        }
    }
}

/// Opens a text file in a small window and returns the window as `targets` lists it:
/// (window id, width, height).
pub fn open_text_window(ctx: &mut Ctx, text: &str) -> R<(TestWindow, u64, i64, i64)> {
    if let Some(why) = sys::window_cases_unavailable() {
        return skip(why);
    }
    let stem = format!("regress-window-{}", std::process::id());
    let path = ctx.root.join(format!("{stem}.txt"));
    std::fs::write(&path, text)?;
    let Some(cmd) = sys::text_window_command(&path) else {
        return skip("no text window program for this system");
    };
    let child = {
        let mut cmd = cmd;
        sys::hide_console(&mut cmd);
        cmd.spawn()
            .map_err(|e| crate::model::Stop::Fail(format!("cannot open the test window: {e}")))?
    };
    let mut guard = TestWindow {
        pids: vec![child.id()],
    };
    let found = crate::proc::wait_for(ctx.secs(25), std::time::Duration::from_millis(700), || {
        let v = ctx.cli(&["targets"]).ok()?.json?;
        v.get("windows")?
            .as_array()?
            .iter()
            .find(|w| {
                w.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.contains(&stem))
            })
            .cloned()
    });
    let Some(w) = found else {
        return fail(format!(
            "the test window '{stem}' never showed up in `targets`"
        ));
    };
    if let Some(pid) = w.get("pid").and_then(Value::as_u64) {
        guard.pids.push(pid as u32);
    }
    // Let it settle (a new Notepad animates in).
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let v = ctx.cli_ok(&["targets"])?;
    let w = v
        .get("windows")
        .and_then(Value::as_array)
        .and_then(|a| {
            a.iter().find(|w| {
                w.get("title")
                    .and_then(Value::as_str)
                    .is_some_and(|t| t.contains(&stem))
            })
        })
        .cloned()
        .ok_or_else(|| {
            crate::model::Stop::Fail("the test window disappeared from `targets`".to_owned())
        })?;
    let id = w.get("id").and_then(Value::as_u64).unwrap_or(0);
    let size = w
        .pointer("/bounds/size")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let (ww, wh) = (
        size.first().and_then(Value::as_i64).unwrap_or(0),
        size.get(1).and_then(Value::as_i64).unwrap_or(0),
    );
    Ok((guard, id, ww, wh))
}

fn window(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let (_win, id, lw, lh) = open_text_window(ctx, "CaptureMann regression test window\n")?;
    check!(
        lw > 0 && lh > 0,
        "the test window is listed with size {lw}x{lh}"
    );
    let out = ctx.root.join("window.png");
    let out_s = out.to_string_lossy().into_owned();
    let id_s = id.to_string();
    ctx.cli_ok(&["capture", "screenshot", "--window", &id_s, "--out", &out_s])?;
    let (w, h) = check_png(&out)?;
    ctx.note(format!("window {id}: listed {lw}x{lh}, saved {w}x{h}"));
    check!(
        i64::from(w) == lw && i64::from(h) == lh,
        "the window is listed {lw}x{lh} but saved {w}x{h}"
    );
    Ok(())
}

fn invalid(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let id = d.id.to_string();
    let out = ctx.root.join("never.png");
    let out_s = out.to_string_lossy().into_owned();
    let mut problems = Vec::new();
    let cases: [(&str, Vec<&str>, &str); 4] = [
        (
            "unknown display",
            vec![
                "capture",
                "screenshot",
                "--display",
                "987654",
                "--out",
                &out_s,
            ],
            "not_found",
        ),
        (
            "empty region",
            vec![
                "capture",
                "screenshot",
                "--display",
                &id,
                "--region",
                "0,0,0,0",
                "--out",
                &out_s,
            ],
            "invalid_input",
        ),
        (
            "region off the display",
            vec![
                "capture",
                "screenshot",
                "--display",
                &id,
                "--region",
                "99999,99999,50,50",
                "--out",
                &out_s,
            ],
            "invalid_input",
        ),
        (
            "unknown window",
            vec![
                "capture",
                "screenshot",
                "--window",
                "987654321",
                "--out",
                &out_s,
            ],
            "not_found",
        ),
    ];
    for (what, args, want) in cases {
        let r = ctx.cli(&args)?;
        ctx.note(format!(
            "{what}: {} {:?}",
            crate::proc::exit_text(r.code()),
            r.error_code()
        ));
        if r.ok() {
            problems.push(format!("{what}: the command succeeded"));
        } else if what == "unknown window"
            && sys::is_linux_wayland()
            && r.error_code().as_deref() == Some("not_implemented")
        {
            // A stated gap: a Wayland session has no window ids, so a window is only picked through the desktop's
            // own picker (`--pick`); the refusal is still an error with a shared code and no capture.
        } else if r.code() != Some(2) {
            problems.push(format!("{what}: exit {:?}, expected 2", r.code()));
        } else if r.error_code().as_deref() != Some(want) {
            // The shared error table allows either of the two for a rectangle that does not fit.
            let alt = matches!(
                (want, r.error_code().as_deref()),
                ("invalid_input", Some("not_found"))
            );
            if !alt {
                problems.push(format!(
                    "{what}: error code {:?}, expected {want}",
                    r.error_code()
                ));
            }
        }
    }
    check!(
        !out.exists(),
        "a refused capture still wrote {}",
        out.display()
    );
    let v = ctx.cli_ok(&["library", "list"])?;
    let n = v
        .get("captures")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    if n != 0 {
        problems.push(format!("{n} capture(s) exist after only refused captures"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}

fn second_monitor(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["targets", "--displays-only"])?;
    let ds = displays(&v);
    if ds.len() < 2 {
        return skip("only one display is connected");
    }
    let mut problems = Vec::new();
    for d in &ds {
        let out = ctx.root.join(format!("display-{}.png", d.id));
        let out_s = out.to_string_lossy().into_owned();
        ctx.cli_ok(&[
            "capture",
            "screenshot",
            "--display",
            &d.id.to_string(),
            "--out",
            &out_s,
        ])?;
        let (w, h) = check_png(&out)?;
        ctx.note(format!(
            "display {} '{}' at ({},{}) listed {}x{}, saved {w}x{h}",
            d.id, d.name, d.x, d.y, d.w, d.h
        ));
        if i64::from(w) != d.w || i64::from(h) != d.h {
            problems.push(format!(
                "display {} listed {}x{} but saved {w}x{h}",
                d.id, d.w, d.h
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}
