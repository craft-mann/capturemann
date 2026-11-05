//! The daemon and the settings: start, stop, second instance, settings round trip, locked keys.

use crate::check;
use crate::ctx::Ctx;
use crate::hygiene;
use crate::model::{fail, Case, Stop, Tier, ALL, R};
use crate::proc::exit_text;
use crate::sys;
use serde_json::Value;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "daemon.lifecycle",
            title: "daemon starts on a private endpoint, answers, stops, and the endpoint is gone",
            area: "daemon",
            specs: &["AGT-1", "R2-8"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: lifecycle,
        },
        Case {
            id: "daemon.library_created",
            title: "a fresh library is created and lists no captures; no helper program runs while idle",
            area: "daemon",
            specs: &["LIB-1", "AI-7: the text engine starts only when asked"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: library_created,
        },
        Case {
            id: "daemon.second_instance",
            title: "a second daemon on a busy endpoint exits cleanly and leaves the first one working",
            area: "daemon",
            specs: &["AGT-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: second_instance,
        },
        Case {
            id: "daemon.autostart",
            title: "the CLI starts a daemon when none runs",
            area: "daemon",
            specs: &["AGT-1"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: autostart,
        },
        Case {
            id: "settings.roundtrip",
            title: "settings set and get round trip, and survive a daemon restart",
            area: "settings",
            specs: &["AGT-1"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: settings_roundtrip,
        },
        Case {
            id: "settings.app_only_refused",
            title: "app-only settings keys are refused to agents and change nothing",
            area: "settings",
            specs: &["AGT-POLICY", "OD-24"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 120,
            run: settings_app_only,
        },
    ]
}

fn lifecycle(ctx: &mut Ctx) -> R {
    let pid = ctx.start_daemon()?;
    let r = ctx.cli(&["daemon", "status"])?;
    let actions = r
        .json
        .as_ref()
        .and_then(|v| v.get("actions"))
        .and_then(Value::as_u64)
        .unwrap_or(0);
    check!(actions > 40, "daemon status lists only {actions} actions");
    ctx.note(format!("pid {pid}, {actions} actions"));
    let graceful = ctx.stop_daemon()?;
    ctx.note(format!(
        "stop: {}",
        if graceful {
            "clean exit on SIGTERM"
        } else {
            "killed (Windows has no signal for a hidden process)"
        }
    ));
    std::thread::sleep(std::time::Duration::from_millis(500));
    check!(
        !ctx.daemon_answers(),
        "the daemon still answers after it was stopped"
    );
    check!(
        !hygiene::product_pids().contains(&pid),
        "process {pid} is still running after the stop"
    );
    Ok(())
}

fn library_created(ctx: &mut Ctx) -> R {
    std::fs::remove_dir_all(&ctx.library)?;
    check!(
        !ctx.library.exists(),
        "could not remove the library folder first"
    );
    ctx.start_daemon()?;
    check!(
        ctx.library.is_dir(),
        "the daemon did not create the library folder {}",
        ctx.library.display()
    );
    let v = ctx.cli_ok(&["library", "list"])?;
    let n = v
        .get("captures")
        .and_then(Value::as_array)
        .map_or(usize::MAX, Vec::len);
    check!(n == 0, "a new library lists {n} captures");
    // The helpers start only when asked: after a diagnostics call there is no ffmpeg or llama-server.
    let _ = ctx.cli_text(&["diagnostics"])?;
    std::thread::sleep(std::time::Duration::from_millis(500));
    let helpers: Vec<String> = sys::list_processes()
        .into_iter()
        .filter(|(_, n)| {
            let l = n.to_lowercase();
            l.starts_with("llama-server") || l.starts_with("ffmpeg") || l.starts_with("cm-overlay")
        })
        .map(|(p, n)| format!("{n} ({p})"))
        .collect();
    check!(
        helpers.is_empty(),
        "helper programs run while the daemon is idle: {}",
        helpers.join(", ")
    );
    Ok(())
}

fn second_instance(ctx: &mut Ctx) -> R {
    let first = ctx.start_daemon()?;
    let mut c = ctx.product_cmd("cm-daemon");
    c.arg("--library")
        .arg(&ctx.library)
        .args(["--socket", &ctx.endpoint.clone()]);
    let out = ctx.run(c, 40)?;
    ctx.note(format!(
        "second daemon: {} in {} ms; output: {}",
        exit_text(out.code),
        out.elapsed_ms,
        crate::util::cut(&crate::util::one_line(&out.text()), 200)
    ));
    check!(
        !out.timed_out,
        "the second daemon did not exit within 40 s (it should say the endpoint is taken)"
    );
    check!(
        out.code == Some(0),
        "the second daemon exited with {} (expected 0, per the daemon's own doc)",
        exit_text(out.code)
    );
    check!(
        ctx.daemon_answers(),
        "the first daemon (pid {first}) no longer answers after a second one tried the endpoint"
    );
    check!(
        hygiene::product_pids().contains(&first),
        "the first daemon died"
    );
    Ok(())
}

fn autostart(ctx: &mut Ctx) -> R {
    let before = hygiene::product_pids();
    let ep = ctx.endpoint.clone();
    // Without --no-start: the CLI starts the daemon next to it. First with its output captured through
    // pipes, the way a script calls it: the CLI must return even though the daemon it started lives on.
    let mut c = ctx.product_cmd("capturemann");
    c.args(["--json", "--socket", &ep, "library", "list"]);
    let out = ctx.run(c, 30)?;
    let mut pipe_held = out.timed_out;
    let mut started: Vec<u32> = hygiene::strays_since(&before)
        .into_iter()
        .filter(|(_, n)| n.to_lowercase().starts_with("cm-daemon"))
        .map(|(p, _)| p)
        .collect();
    ctx.note(format!(
        "piped run: {}{}; new cm-daemon pids {started:?}",
        exit_text(out.code),
        if pipe_held {
            " (hung until killed: the started daemon keeps the caller's output pipe open)"
        } else {
            ""
        }
    ));
    if pipe_held {
        // Check the start itself with the output going to files instead of pipes.
        for p in &started {
            sys::kill_tree(*p);
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
        let log = std::fs::File::create(ctx.root.join("autostart.out.txt"))?;
        let mut c = ctx.product_cmd("capturemann");
        c.args(["--json", "--socket", &ep, "library", "list"])
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log);
        let mut child = c
            .spawn()
            .map_err(|e| Stop::Fail(format!("capturemann did not start: {e}")))?;
        let waited =
            crate::proc::wait_for(ctx.secs(40), std::time::Duration::from_millis(200), || {
                child.try_wait().ok().flatten()
            });
        if waited.is_none() {
            sys::kill_tree(child.id());
            let _ = child.kill();
        }
        ctx.note(format!(
            "file-redirected run: {:?}",
            waited.map(|s| s.code())
        ));
        started = hygiene::strays_since(&before)
            .into_iter()
            .filter(|(_, n)| n.to_lowercase().starts_with("cm-daemon"))
            .map(|(p, _)| p)
            .collect();
        pipe_held = true;
    }
    let answered = ctx.daemon_answers();
    for p in &started {
        sys::kill_tree(*p);
    }
    check!(
        !pipe_held,
        "PRODUCT: when `capturemann` starts the daemon, the daemon inherits the caller's output pipe, so a script that captures the output never gets EOF (the CLI hung for 30 s until killed); the daemon itself {}",
        if answered { "did start and answer" } else { "did not answer either" }
    );
    check!(
        out.success(),
        "the CLI did not start a daemon and answer: {}",
        crate::util::cut(&out.text(), 300)
    );
    check!(
        started.len() == 1,
        "expected the CLI to start exactly one cm-daemon, saw {}",
        started.len()
    );
    check!(answered, "the started daemon does not answer");
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

fn settings_roundtrip(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["settings", "set", "recording.fps", "60"])?;
    check!(
        v.pointer("/settings/recording.fps").and_then(Value::as_i64) == Some(60),
        "set did not echo 60: {v}"
    );
    let g = ctx.cli_ok(&["settings", "get", "recording.fps"])?;
    check!(
        g.pointer("/settings/recording.fps").and_then(Value::as_i64) == Some(60),
        "get did not return 60: {g}"
    );
    // Survives a restart of the daemon (settings.json in the config folder).
    ctx.stop_daemon()?;
    ctx.start_daemon()?;
    let g = ctx.cli_ok(&["settings", "get", "recording.fps"])?;
    check!(
        g.pointer("/settings/recording.fps").and_then(Value::as_i64) == Some(60),
        "60 fps was not kept over a restart: {g}"
    );
    let _ = ctx.cli_ok(&["settings", "set", "recording.fps", "30"])?;
    let g = ctx.cli_ok(&["settings", "get", "recording.fps"])?;
    check!(
        g.pointer("/settings/recording.fps").and_then(Value::as_i64) == Some(30),
        "30 fps was not stored: {g}"
    );
    // A nonsense value is refused with the shared invalid_input code, exit 2.
    let bad = ctx.cli(&["settings", "set", "recording.fps", "47"])?;
    ctx.note(format!(
        "recording.fps=47: {} {:?}",
        exit_text(bad.code()),
        bad.error_code()
    ));
    if bad.ok() {
        // Some builds accept any number; then it must not have corrupted the value type.
        let g = ctx.cli_ok(&["settings", "get", "recording.fps"])?;
        ctx.note(format!("47 was accepted: {g}"));
    } else {
        check!(
            bad.code() == Some(2) && bad.error_code().as_deref() == Some("invalid_input"),
            "a bad fps value gave {} {:?}, expected exit 2 invalid_input",
            exit_text(bad.code()),
            bad.error_code()
        );
    }
    let file = std::fs::read_to_string(ctx.config.join("settings.json")).unwrap_or_default();
    check!(
        file.contains("recording.fps"),
        "settings.json in the config folder does not hold the key"
    );
    Ok(())
}

fn settings_app_only(ctx: &mut Ctx) -> R {
    ctx.start_daemon()?;
    let mut problems = Vec::new();
    for (key, value) in [
        ("agents.tool_policy", r#"{"start_recording":"allow"}"#),
        ("library.path", "C:/nowhere"),
        ("privacy.recording_indicator", "false"),
        ("auth.token", "x"),
    ] {
        let r = ctx.cli(&["settings", "set", key, value])?;
        ctx.note(format!(
            "set {key}: {} {:?} {}",
            exit_text(r.code()),
            r.error_code(),
            crate::util::cut(&r.error_message(), 120)
        ));
        if r.ok() {
            problems.push(format!("`{key}` was accepted from an agent"));
        } else if r.error.is_none() {
            problems.push(format!(
                "`{key}` failed without a JSON error ({})",
                exit_text(r.code())
            ));
        }
    }
    let g = ctx.cli_ok(&["settings", "get"])?;
    let s = g
        .get("settings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for key in [
        "agents.tool_policy",
        "library.path",
        "privacy.recording_indicator",
        "auth.token",
    ] {
        if s.contains_key(key) {
            problems.push(format!(
                "`{key}` appears in the settings after a refused write"
            ));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        fail(problems.join("; "))
    }
}
