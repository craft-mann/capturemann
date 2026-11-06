//! The per-user installer: silent install into a temporary folder, the installed programs work, silent
//! uninstall leaves nothing behind. Only with `--installer <setup.exe>`; it restores the machine's own
//! uninstall registry entry afterwards (the installer writes the same key as a real install).
//!
//! The installer and the uninstaller work on per-user places that every install shares, whatever its folder:
//! the uninstall key, `HKCU\Software\CaptureMann`, the Open with registrations, the Start menu folder and the
//! per-user PATH. A temporary install and its uninstall therefore also remove what a real install on the same
//! account put there (found Nov 5, 2026, when this case took the Start menu shortcuts of the lab laptop's own
//! install). The case saves all of it first and puts it back at the end.

use crate::check;
use crate::ctx::Ctx;
use crate::model::{fail, skip, Case, Need, Tier, MAC, R, WIN};
use crate::proc::exit_text;
use crate::sys;
use std::path::PathBuf;
use std::process::Command;

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "installer.dmg",
            title: "the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing",
            area: "installer",
            specs: &["packaging/macos", "make verify-release"],
            tier: Tier::Full,
            needs: &[Need::Installer],
            platforms: MAC,
            timeout_s: 300,
            run: dmg,
        },
        Case {
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
    },
    ]
}

/// macOS: the same idea as `silent`, without a registry: the disk image is mounted under the sandbox, the app
/// is copied into the sandbox (never `/Applications`), checked and removed, and the image is ejected. The app
/// is not opened, so the user's own copy and its permissions are untouched.
fn dmg(ctx: &mut Ctx) -> R {
    let Some(src) = ctx.opts.installer.clone() else {
        return skip("no --installer given (a .dmg or a .app)");
    };
    let work = ctx.dir("dmg");
    match sys::check_app_install(&src, &work) {
        Ok(notes) => {
            for n in notes {
                ctx.note(n);
            }
            Ok(())
        }
        Err(e) => fail(e),
    }
}

const UNINSTALL_ROOT: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall";

/// Other registry keys the installer writes and the uninstaller deletes (installer.nsi), shared by all installs.
const SHARED_KEYS: &[&str] = &[
    r"HKCU\Software\CaptureMann",
    r"HKCU\Software\Classes\Applications\cm-desktop.exe",
    r"HKCU\Software\Classes\.mp4\OpenWithList\cm-desktop.exe",
    r"HKCU\Software\Classes\.mov\OpenWithList\cm-desktop.exe",
    r"HKCU\Software\Classes\.m4v\OpenWithList\cm-desktop.exe",
    r"HKCU\Software\Classes\.mkv\OpenWithList\cm-desktop.exe",
    r"HKCU\Software\Classes\.m4a\OpenWithList\cm-desktop.exe",
    r"HKCU\Software\Classes\.wav\OpenWithList\cm-desktop.exe",
];

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
    // The other shared keys: export those that exist now, and delete them again at the end before the import.
    let mut shared_exports: Vec<(String, String)> = Vec::new();
    for (i, k) in SHARED_KEYS.iter().enumerate() {
        let f = ctx.root.join(format!("shared-key-{i}.reg"));
        let fs = f.to_string_lossy().into_owned();
        if reg(ctx, &["export", k, &fs, "/y"])?.success() {
            shared_exports.push(((*k).to_owned(), fs));
        }
    }
    let user_path = reg_user_path(ctx);
    let menu = std::env::var_os("APPDATA")
        .map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs"));
    let menu_before = menu.as_ref().map(|m| list_matching(m)).unwrap_or_default();
    // The Start menu folder of a real install is copied away; the uninstaller deletes it (same folder name).
    let menu_dir = menu.as_ref().map(|m| m.join("CaptureMann"));
    let menu_backup = ctx.root.join("startmenu-backup");
    let menu_saved = menu_dir
        .as_ref()
        .is_some_and(|d| d.is_dir() && copy_dir(d, &menu_backup).is_ok());
    ctx.note(format!(
        "saved before: {} shared key(s), Start menu folder {}, user PATH {}",
        shared_exports.len(),
        if menu_saved { "yes" } else { "no (none there)" },
        if user_path.is_some() { "yes" } else { "no" }
    ));

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
        if let Some(d) = &menu_dir {
            check!(
                d.join("CaptureMann.lnk").is_file(),
                "the installer made no Start menu shortcut in {}",
                d.display()
            );
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
        // The folder is shared by every install, so the uninstaller must remove it whether or not one was there
        // before (then it is put back below).
        let menu_after = menu.as_ref().map(|m| list_matching(m)).unwrap_or_default();
        check!(
            menu_after.is_empty(),
            "the uninstaller left Start menu entries behind: {menu_after:?} (before the run: {menu_before:?})"
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
    // The shared keys: remove what the run left, then import the saved ones.
    for k in SHARED_KEYS {
        let _ = reg(ctx, &["delete", k, "/f"]);
    }
    for (_, f) in &shared_exports {
        let _ = reg(ctx, &["import", f]);
    }
    if let Some(d) = &menu_dir {
        let _ = std::fs::remove_dir_all(d);
        if menu_saved {
            let _ = copy_dir(&menu_backup, d);
        }
    }
    if let Some(p) = &user_path {
        let _ = reg(
            ctx,
            &[
                "add",
                r"HKCU\Environment",
                "/v",
                "Path",
                "/t",
                "REG_EXPAND_SZ",
                "/d",
                p,
                "/f",
            ],
        );
    }
    let _ = std::fs::remove_dir_all(&dest);
    match result {
        Ok(()) => {
            let end = keys(ctx)?;
            if reg_user_path(ctx) != user_path {
                return fail("the per-user PATH differs after the run".to_owned());
            }
            if end != before_keys {
                return fail(format!("the uninstall registry keys differ after the run: before {before_keys:?}, after {end:?}"));
            }
            Ok(())
        }
        e => e,
    }
}

/// The raw per-user PATH value (`reg query` text after the type), None when there is none.
fn reg_user_path(ctx: &mut Ctx) -> Option<String> {
    let out = reg(ctx, &["query", r"HKCU\Environment", "/v", "Path"]).ok()?;
    out.stdout.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix("Path")?.trim_start();
        let (_ty, val) = rest.split_once(char::is_whitespace)?;
        Some(val.trim().to_owned())
    })
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let t = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &t)?;
        } else {
            std::fs::copy(e.path(), t)?;
        }
    }
    Ok(())
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
