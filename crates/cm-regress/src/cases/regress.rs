//! Cases for bugs found on the Windows lab laptop that no other area covers, and the end-of-run checks.

use super::common::{write_clips, wrong_platform_words};
use crate::check;
use crate::ctx::Ctx;
use crate::hygiene;
use crate::model::{fail, skip, Case, Need, Tier, ALL, R};
use crate::proc::exit_text;
use crate::sys;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "regress.ffmpeg_missing_wording",
            title: "with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows)",
            area: "regress",
            specs: &["bug: FFmpeg-missing text named Homebrew on Windows", "R5-3"],
            tier: Tier::Standard,
            needs: &[],
            platforms: ALL,
            timeout_s: 240,
            run: ffmpeg_missing,
        },
        Case {
            id: "regress.encoder_unavailable_wording",
            title: "a recording the hardware encoder refuses ends with neutral wording and leaves no bundle",
            area: "regress",
            specs: &["bug: hardware_encoder_unavailable text named a Mac", "REC-WIN section 5"],
            tier: Tier::Full,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: ALL,
            timeout_s: 150,
            run: encoder_wording,
        },
        Case {
            id: "zz.crash_events",
            title: "no crash of a CaptureMann program was logged by Windows during the whole run",
            area: "zz",
            specs: &["bug: recording stop crash 0xC0000409", "bug: illegal instruction"],
            tier: Tier::Smoke,
            needs: &[],
            platforms: ALL,
            timeout_s: 60,
            run: crash_events,
        },
    ]
}

fn ffmpeg_missing(ctx: &mut Ctx) -> R {
    // A copy of the CLI and the daemon in a folder with no FFmpeg next to it, and a PATH without one.
    let bin = ctx.dir("bin-no-ffmpeg");
    for base in ["capturemann", "cm-daemon"] {
        std::fs::copy(ctx.pkg.exe(base), bin.join(sys::exe_name(base)))?;
    }
    for e in std::fs::read_dir(&ctx.pkg.dir)?.flatten() {
        if e.path().extension().and_then(|x| x.to_str()) == Some("dll") {
            std::fs::copy(e.path(), bin.join(e.file_name()))?;
        }
    }
    ctx.pkg.dir = bin;
    let path = if sys::PLATFORM == "windows" {
        r"C:\Windows\System32;C:\Windows"
    } else {
        "/usr/bin:/bin"
    };
    ctx.extra_env.push(("PATH".to_owned(), path.to_owned()));
    let clips = write_clips(ctx)?;
    ctx.start_daemon()?;
    let diag = ctx.cli_text(&["diagnostics"])?;
    let ffline = diag
        .out
        .stdout
        .lines()
        .find(|l| l.to_lowercase().contains("ffmpeg"))
        .unwrap_or("")
        .to_owned();
    ctx.note(format!("diagnostics: {ffline}"));
    if ffline.contains("ffmpeg at") {
        return skip(format!(
            "this system has an FFmpeg in a standard folder the suite cannot hide ({})",
            crate::util::cut(&ffline, 80)
        ));
    }
    check!(
        ffline.to_lowercase().contains("not found")
            || ffline.to_lowercase().contains("no ffmpeg")
            || ffline.to_lowercase().contains("missing"),
        "diagnostics does not say FFmpeg is missing: '{ffline}'"
    );
    let p = clips.join("clip-h264.mp4").to_string_lossy().into_owned();
    let r = ctx.cli_long(&["import", &p, "--wait", "--timeout", "60"], 100)?;
    let text = crate::util::one_line(&r.out.text());
    ctx.note(format!(
        "import without FFmpeg: {} {}",
        exit_text(r.code()),
        crate::util::cut(&text, 300)
    ));
    check!(!r.out.timed_out, "the import hung without FFmpeg");
    check!(
        text.to_lowercase().contains("ffmpeg"),
        "the failure does not mention FFmpeg: {}",
        crate::util::cut(&text, 300)
    );
    let all = format!("{text} {ffline}");
    let wrong = wrong_platform_words(&all, sys::PLATFORM);
    check!(
        wrong.is_empty(),
        "the FFmpeg-missing text uses another platform's words {wrong:?}: {}",
        crate::util::cut(&all, 300)
    );
    // The daemon logged the missing helper; that is the expected line here.
    ctx.log_allow.push("ffmpeg".to_owned());
    ctx.log_allow.push("FFmpeg".to_owned());
    Ok(())
}

fn encoder_wording(ctx: &mut Ctx) -> R {
    ctx.allow_recording_without_dialog()?;
    ctx.start_daemon()?;
    let d = ctx.primary_display()?;
    let id = d.id.to_string();
    let r = ctx.cli_long(
        &[
            "record",
            "start",
            "--display",
            &id,
            "--region",
            "0,0,2,2",
            "--countdown",
            "0",
        ],
        60,
    )?;
    if r.ok() {
        let _ = ctx.cli_long(&["record", "stop"], 60);
        return skip("the machine recorded a 2x2 region: could not provoke the encoder refusal");
    }
    let code = r.error_code().unwrap_or_default();
    let msg = r.error_message();
    ctx.note(format!(
        "2x2 region: {} {code}: {}",
        exit_text(r.code()),
        crate::util::cut(&msg, 200)
    ));
    if !matches!(
        code.as_str(),
        "hardware_encoder_unavailable" | "platform_rejected"
    ) || !msg.to_lowercase().contains("encoder")
    {
        return skip(format!(
            "the request was refused earlier ({code}), not by the encoder: could not provoke it"
        ));
    }
    let wrong = wrong_platform_words(&msg, sys::PLATFORM);
    check!(
        wrong.is_empty(),
        "the hardware-encoder message uses another platform's words {wrong:?}: {msg}"
    );
    check!(
        !msg.contains("2016"),
        "the message still names a Mac model year: {msg}"
    );
    check!(
        ctx.bundles("cmrec").is_empty(),
        "a failed start left a recording bundle behind"
    );
    Ok(())
}

fn crash_events(ctx: &mut Ctx) -> R {
    if sys::PLATFORM != "windows" {
        return skip("the Application event log is Windows only");
    }
    let crashes = hygiene::app_crashes_since(ctx.opts.started - std::time::Duration::from_secs(2));
    for c in &crashes {
        ctx.note(format!("crash event: {c}"));
    }
    if crashes.is_empty() {
        Ok(())
    } else {
        fail(format!(
            "{} crash event(s) of our programs since the run started: {}",
            crashes.len(),
            crashes[0]
        ))
    }
}
