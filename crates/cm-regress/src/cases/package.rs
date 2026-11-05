//! Package sanity: the files are there, the programs start, the diagnostics report is complete.

use super::common::wrong_platform_words;
use crate::check;
use crate::ctx::Ctx;
use crate::model::{fail, Case, Tier, ALL, R};
use crate::proc::{exit_text, is_crash_code};
use crate::sys;
use std::path::Path;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "pkg.files",
            title: "the package holds the programs, FFmpeg, the helper and the licences",
            area: "pkg",
            specs: &["NFR-licence", "PKG-contents"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 30,
            run: files,
        },
        Case {
            id: "pkg.startup",
            title: "every program starts without a crash (illegal instruction, fast fail)",
            area: "pkg",
            specs: &[
                "bug: Transcribe illegal instruction (AVX-512 in the build)",
                "bug: 0xC0000409 at start",
            ],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: startup,
        },
        Case {
            id: "pkg.diagnostics",
            title: "diagnostics reports app, system, models and engines, FFmpeg and the helper",
            area: "pkg",
            specs: &["DIAG-1"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 90,
            run: diagnostics,
        },
    ]
}

fn find_named(dir: &Path, depth: u32, pred: &dyn Fn(&str) -> bool) -> Option<String> {
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if p.is_file() && pred(&name.to_lowercase()) {
            return Some(p.display().to_string());
        }
        if p.is_dir() && depth > 0 {
            if let Some(f) = find_named(&p, depth - 1, pred) {
                return Some(f);
            }
        }
    }
    None
}

fn files(ctx: &mut Ctx) -> R {
    let mut missing = Vec::new();
    for base in [
        "capturemann",
        "cm-mcp",
        "cm-daemon",
        "cm-desktop",
        "cm-overlay",
    ] {
        if ctx.opts.dev && matches!(base, "cm-desktop" | "cm-overlay") && !ctx.pkg.has(base) {
            ctx.note(format!(
                "--dev: {} not built, not required",
                sys::exe_name(base)
            ));
            continue;
        }
        if base == "cm-daemon" && sys::PLATFORM == "macos" && !ctx.pkg.dir.join(base).is_file() {
            // The macOS app embeds the daemon in cm-desktop; the headless cases use `--daemon`.
            ctx.note(match &ctx.opts.daemon {
                Some(d) => format!(
                    "cm-daemon is not in the app (embedded in cm-desktop); using --daemon {}",
                    d.display()
                ),
                None => "cm-daemon is not in the app (embedded in cm-desktop); no --daemon given"
                    .to_owned(),
            });
            continue;
        }
        let p = ctx.pkg.exe(base);
        match std::fs::metadata(&p) {
            Ok(m) if m.len() > 100_000 => {
                ctx.note(format!("{}: {} bytes", sys::exe_name(base), m.len()))
            }
            Ok(m) => missing.push(format!("{} is only {} bytes", sys::exe_name(base), m.len())),
            Err(_) => missing.push(format!("{} is missing", sys::exe_name(base))),
        }
    }
    if !ctx.opts.dev {
        for base in ["ffmpeg", "ffprobe"] {
            if !ctx.pkg.has(base) {
                missing.push(format!(
                    "{} is missing (bundled FFmpeg)",
                    sys::exe_name(base)
                ));
            }
        }
        if sys::PLATFORM == "windows" && !ctx.pkg.has("llama-server") {
            missing.push(
                "llama-server.exe is missing (the Windows package ships the text engine)"
                    .to_owned(),
            );
        }
        let mut dir = ctx.pkg.dir.clone();
        if sys::PLATFORM == "macos" && dir.ends_with("Contents/MacOS") {
            // Inside CaptureMann.app the licences sit in Contents/Resources.
            dir = dir.join("../Resources");
        }
        let lic = find_named(&dir, 2, &|n| {
            n.starts_with("license") || n.starts_with("licence")
        });
        let third = find_named(&dir, 2, &|n| n.starts_with("third_party_licenses"));
        let notice = find_named(&dir, 2, &|n| n.starts_with("notice"));
        if lic.is_none() {
            missing.push("no LICENSE file".to_owned());
        }
        if third.is_none() {
            missing.push("no THIRD_PARTY_LICENSES file".to_owned());
        }
        ctx.note(format!(
            "licence: {lic:?}, third party: {third:?}, notice: {notice:?}"
        ));
        if find_named(&dir, 2, &|n| n.starts_with("readme")).is_none() {
            ctx.note("note: no README in the package folder");
        }
        if sys::PLATFORM == "windows" {
            for rel in ["FFmpeg/SOURCE-OFFER.txt", "FFmpeg/BUILD-INFO.txt"] {
                if !dir.join(rel).is_file() {
                    missing.push(format!("{rel} is missing (LGPL source offer)"));
                }
            }
        }
    } else {
        ctx.note("--dev: bundled FFmpeg, helper and licence files not required");
    }
    check!(missing.is_empty(), "{}", missing.join("; "));
    Ok(())
}

fn startup(ctx: &mut Ctx) -> R {
    // Programs without --version get a harmless bad flag: they print their usage and exit 2.
    let mut runs: Vec<(&str, Vec<&str>, i32)> = vec![
        ("capturemann", vec!["--version"], 0),
        ("cm-mcp", vec!["--bogus"], 2),
    ];
    if ctx.pkg.has("cm-daemon") {
        runs.push(("cm-daemon", vec!["--bogus"], 2));
    }
    if ctx.pkg.has("cm-overlay") {
        runs.push(("cm-overlay", vec!["--help"], 0));
    }
    if ctx.pkg.has("ffmpeg") {
        runs.push(("ffmpeg", vec!["-hide_banner", "-version"], 0));
        runs.push(("ffprobe", vec!["-hide_banner", "-version"], 0));
    }
    if ctx.pkg.has("llama-server") {
        runs.push(("llama-server", vec!["--version"], 0));
    }
    let mut problems = Vec::new();
    for (base, args, want) in runs {
        let mut c = ctx.product_cmd(base);
        c.args(&args);
        let out = ctx.run(c, 30)?;
        ctx.note(format!(
            "{} {}: {} in {} ms",
            base,
            args.join(" "),
            exit_text(out.code),
            out.elapsed_ms
        ));
        if out.timed_out {
            problems.push(format!("{base} did not finish within 30 s"));
        } else if is_crash_code(out.code) {
            problems.push(format!("{base} crashed at start: {}", exit_text(out.code)));
        } else if out.code != Some(want) {
            problems.push(format!(
                "{base} {} ended with {}, expected exit {want}",
                args.join(" "),
                exit_text(out.code)
            ));
        }
    }
    check!(problems.is_empty(), "{}", problems.join("; "));
    Ok(())
}

fn diagnostics(ctx: &mut Ctx) -> R {
    let v = {
        let mut c = ctx.product_cmd("capturemann");
        c.arg("--version");
        ctx.run(c, 20)?
    };
    check!(
        v.success(),
        "capturemann --version failed: {}",
        exit_text(v.code)
    );
    let version = v
        .stdout
        .trim()
        .trim_start_matches("capturemann")
        .trim()
        .to_owned();
    check!(
        !version.is_empty() && version.chars().next().is_some_and(|c| c.is_ascii_digit()),
        "odd version text `{}`",
        v.stdout.trim()
    );
    ctx.note(format!("version {version}"));

    ctx.start_daemon()?;
    let r = ctx.cli_text(&["diagnostics"])?;
    check!(r.ok(), "diagnostics failed: {}", exit_text(r.out.code));
    let text = r.out.stdout.clone();
    let mut missing = Vec::new();
    for needle in [
        "## App",
        "## System",
        "## Models and engines",
        "## Settings",
    ] {
        if !text.contains(needle) {
            missing.push(format!("section `{needle}` missing"));
        }
    }
    if !text.contains(&format!("CaptureMann {version}")) {
        missing.push(format!("`CaptureMann {version}` not in the App section"));
    }
    if !text.to_lowercase().contains(sys::PLATFORM) {
        missing.push(format!("the report does not name {}", sys::PLATFORM));
    }
    for needle in ["text recognizer:", "speaker engine"] {
        if !text.contains(needle) {
            missing.push(format!("no `{needle}` line"));
        }
    }
    if !ctx.opts.dev {
        if !text.contains("ffmpeg at") {
            missing.push("no `ffmpeg at` line (the bundled FFmpeg is not found)".to_owned());
        }
        if ctx.pkg.has("llama-server") && !text.contains("bundled with this build") {
            missing.push("the llama-server line does not say `bundled with this build`".to_owned());
        }
    }
    let wrong = wrong_platform_words(&text, sys::PLATFORM);
    if !wrong.is_empty() {
        missing.push(format!(
            "the report uses another platform's words: {wrong:?}"
        ));
    }
    for line in text.lines().filter(|l| {
        l.contains("ffmpeg")
            || l.contains("llama-server")
            || l.contains("recognizer")
            || l.contains("speaker")
            || l.starts_with("- windows")
            || l.starts_with("- GPU")
            || l.contains("gpu")
    }) {
        ctx.note(crate::util::cut(line, 160));
    }
    if text.contains("[redacted]") {
        ctx.note("note: the report contains a [redacted] mark (check it is not a hardware name)");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        fail(missing.join("; "))
    }
}
