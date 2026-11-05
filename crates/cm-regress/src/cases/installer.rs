//! The per-user installer: silent install into a temporary folder, the installed programs work, silent
//! uninstall leaves nothing behind. Only with `--installer <setup.exe>`; it restores the machine's own
//! uninstall registry entry afterwards (the installer writes the same key as a real install).

use crate::check;
use crate::ctx::Ctx;
use crate::model::{fail, skip, Case, Need, Tier, R, WIN};
use crate::proc::exit_text;
use crate::sys;
use std::path::PathBuf;
use std::process::Command;

pub fn cases() -> Vec<Case> {
    vec![Case {
        id: "installer.silent",
        title:
            "silent install into a temp folder, the programs run, silent uninstall leaves nothing",
        area: "installer",
        specs: &["O7 installer", "windows-package.yml silent install"],
        tier: Tier::Full,
        needs: &[Need::Installer],
        platforms: WIN,
        timeout_s: 420,
        run: silent,
    }]
}

const UNINSTALL_ROOT: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall";

fn reg(ctx: &mut Ctx, args: &[&str]) -> R<crate::proc::Output> {
    let mut c = Command::new("reg");
    c.args(args);
    ctx.run(c, 60)
}

/// The uninstall keys whose name mentions CaptureMann.
fn keys(ctx: &mut Ctx) -> R<Vec<String>> {
    let out = reg(ctx, &["query", UNINSTALL_ROOT, "/f", "CaptureMann", "/k"])?;
    Ok(out
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| {
            l.to_uppercase().starts_with("HKEY_CURRENT_USER")
                && l.to_lowercase().contains("capturemann")
        })
        .map(str::to_owned)
        .collect())
}

fn silent(ctx: &mut Ctx) -> R {
    let Some(setup) = ctx.opts.installer.clone() else {
        return skip("no --installer given");
    };
    let dest: PathBuf = ctx.root.join("installed");
    if dest.to_string_lossy().contains(' ') {
        return skip("the temp folder has a space in its name; NSIS /D= must stay unquoted");
    }
    // Remember what a real install of this machine has, to put it back.
    let before_keys = keys(ctx)?;
    let mut exports = Vec::new();
    for (i, k) in before_keys.iter().enumerate() {
        let f = ctx.root.join(format!("uninstall-key-{i}.reg"));
        let fs = f.to_string_lossy().into_owned();
        reg(ctx, &["export", k, &fs, "/y"])?;
        exports.push(fs);
    }
    ctx.note(format!(
        "{} uninstall key(s) of CaptureMann before: {before_keys:?}",
        before_keys.len()
    ));
    let menu = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs"));
    let menu_before = menu.as_ref().map(|m| list_matching(m)).unwrap_or_default();

    let result = (|| -> R {
        let mut c = Command::new(&setup);
        c.arg("/S").arg(format!("/D={}", dest.display()));
        let out = ctx.run(c, 180)?;
        ctx.note(format!(
            "installer: {} in {} ms",
            exit_text(out.code),
            out.elapsed_ms
        ));
        check!(
            out.code == Some(0),
            "the installer ended with {}",
            exit_text(out.code)
        );
        check!(
            dest.join("capturemann.exe").is_file(),
            "capturemann.exe is not in {}",
            dest.display()
        );
        for f in [
            "cm-daemon.exe",
            "cm-mcp.exe",
            "cm-desktop.exe",
            "cm-overlay.exe",
        ] {
            check!(dest.join(f).is_file(), "{f} is not installed");
        }
        // The installed programs work.
        ctx.pkg.dir = dest.clone();
        let mut v = ctx.product_cmd("capturemann");
        v.arg("--version");
        let o = ctx.run(v, 30)?;
        check!(
            o.success(),
            "the installed capturemann --version ended with {}",
            exit_text(o.code)
        );
        ctx.note(format!("installed: {}", o.stdout.trim()));
        ctx.start_daemon()?;
        let r = ctx.cli_ok(&["library", "list"])?;
        check!(
            r.get("captures").is_some(),
            "the installed daemon's library list is odd: {r}"
        );
        ctx.stop_daemon()?;
        std::thread::sleep(std::time::Duration::from_secs(1));
        // Uninstall.
        let un = dest.join("uninstall.exe");
        check!(
            un.is_file(),
            "there is no uninstall.exe in the install folder"
        );
        let mut u = Command::new(&un);
        u.arg("/S").arg(format!("_?={}", dest.display()));
        let out = ctx.run(u, 180)?;
        ctx.note(format!(
            "uninstaller: {} in {} ms",
            exit_text(out.code),
            out.elapsed_ms
        ));
        check!(
            out.code == Some(0),
            "the uninstaller ended with {}",
            exit_text(out.code)
        );
        let left: Vec<String> = Ctx::files_under(&dest)
            .into_iter()
            .filter(|f| {
                !f.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case("uninstall.exe"))
            })
            .map(|f| f.strip_prefix(&dest).unwrap_or(&f).display().to_string())
            .collect();
        check!(
            left.is_empty(),
            "the uninstaller left {} file(s): {}",
            left.len(),
            crate::util::cut(&left.join(", "), 300)
        );
        let menu_after = menu.as_ref().map(|m| list_matching(m)).unwrap_or_default();
        check!(
            menu_after == menu_before,
            "the Start menu entries changed: before {menu_before:?}, after {menu_after:?}"
        );
        Ok(())
    })();
    ctx.kill_daemon();
    // Put the machine's own registry entries back whatever happened.
    let after_keys = keys(ctx).unwrap_or_default();
    for k in &after_keys {
        if !before_keys.contains(k) {
            let _ = reg(ctx, &["delete", k, "/f"]);
        }
    }
    for f in &exports {
        let _ = reg(ctx, &["import", f]);
    }
    let _ = std::fs::remove_dir_all(&dest);
    match result {
        Ok(()) => {
            let end = keys(ctx)?;
            if end != before_keys {
                return fail(format!("the uninstall registry keys differ after the run: before {before_keys:?}, after {end:?}"));
            }
            Ok(())
        }
        e => e,
    }
}

fn list_matching(dir: &std::path::Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().contains("capturemann"))
        .collect();
    v.sort();
    let _ = sys::PLATFORM;
    v
}
