//! Linux-only cases (Ubuntu 24.04, X11 and GNOME Wayland): the desktop session, monitors, the app and its
//! second launch, the tray, VA-API versus the software encoder, and the `.deb` and tarball installs.
//! The probes live in `sys/linux.rs`; every case here reports SKIP with the reason when the machine cannot
//! answer (no desktop session, a Wayland session for window probes, no passwordless sudo, no package given).
//!
//! The install cases take the package from `--installer <file>`: a `.deb` runs `linux.deb_install`, a
//! `.tar.gz` runs `linux.tarball_install`. Recording cases are the shared `rec.*` ones; they report SKIP
//! when the run has `CM_LINUX_RECORDING=0` (`sys::recording_unavailable`; recording is on by default since 0.3.0).

use crate::check;
use crate::ctx::{displays, Ctx};
use crate::model::{fail, skip, Case, Need, Platform, Stop, Tier, R};
use crate::proc::exit_text;
use crate::sys::linux::CommandRunner;
use crate::sys::{self, linux as lx};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const LINUX: &[Platform] = &[Platform::Linux];

pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "linux.session",
            title: "the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal",
            area: "linux",
            specs: &["LINUX-1", "platform-acceptance"],
            tier: Tier::Smoke,
            needs: &[Need::DesktopSession],
            platforms: LINUX,
            timeout_s: 30,
            run: session,
        },
        Case {
            id: "linux.displays",
            title: "xrandr and the product list the same monitors",
            area: "linux",
            specs: &["CAP-3", "LINUX-1"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: LINUX,
            timeout_s: 90,
            run: monitors,
        },
        Case {
            id: "linux.desktop_stays_up",
            title: "cm-desktop starts, stays up for 10 s and (X11) shows a window of its own",
            area: "linux",
            specs: &["bug class: crash at start", "LINUX-1"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: LINUX,
            timeout_s: 120,
            run: stays_up,
        },
        Case {
            id: "linux.second_launch",
            title: "a second launch with a file forwards it to the running app and exits",
            area: "linux",
            specs: &["IMP-4: second launch opens Import"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: LINUX,
            timeout_s: 120,
            run: second_launch,
        },
        Case {
            id: "linux.tray_icon",
            title: "the tray icon registers as a StatusNotifierItem while the app runs",
            area: "linux",
            specs: &["REC-5 tray"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession],
            platforms: LINUX,
            timeout_s: 90,
            run: tray_icon,
        },
        Case {
            id: "linux.hotkey_overlay",
            title: "the capture hotkey opens the overlay on every monitor",
            area: "linux",
            specs: &["CAP-1 overlay"],
            tier: Tier::Standard,
            needs: &[Need::DesktopSession, Need::Screen],
            platforms: LINUX,
            timeout_s: 30,
            run: hotkey_overlay,
        },
        Case {
            id: "linux.vaapi_encoder",
            title: "VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding",
            area: "linux",
            specs: &["REC-1", "DIAG-1", "D-R2 software VP9 fallback"],
            tier: Tier::Standard,
            needs: &[],
            platforms: LINUX,
            timeout_s: 120,
            run: vaapi,
        },
        Case {
            id: "linux.deb_install",
            title: "the .deb installs, its programs find their libraries and run, remove and purge leave nothing",
            area: "linux",
            specs: &["PKG-deb", "packaging/linux/build-deb.sh"],
            tier: Tier::Full,
            needs: &[Need::Installer],
            platforms: LINUX,
            timeout_s: 600,
            run: deb_install,
        },
        Case {
            id: "linux.tarball_install",
            title: "the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing",
            area: "linux",
            specs: &["PKG-tarball", "packaging/linux/install.sh"],
            tier: Tier::Full,
            needs: &[Need::Installer],
            platforms: LINUX,
            timeout_s: 420,
            run: tarball_install,
        },
    ]
}

fn runner() -> lx::SystemRunner {
    lx::SystemRunner::default()
}

fn env_get(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

fn alive(pid: u32) -> bool {
    sys::list_processes().iter().any(|(p, _)| *p == pid)
}

fn session(ctx: &mut Ctx) -> R {
    let kind = lx::session_kind(&env_get);
    check!(
        kind != lx::Session::Headless,
        "no DISPLAY or WAYLAND_DISPLAY in the environment of the run"
    );
    ctx.note(format!(
        "session {} desktop {} runtime dir {}",
        kind.name(),
        env_get("XDG_CURRENT_DESKTOP").unwrap_or_else(|| "?".into()),
        env_get("XDG_RUNTIME_DIR").unwrap_or_else(|| "unset".into())
    ));
    let r = runner();
    let portal = r.run(
        "gdbus",
        &[
            "introspect",
            "--session",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
        ],
    );
    ctx.note(match portal {
        Some(o) if o.ok() => "desktop portal answers on the session bus".to_owned(),
        Some(o) => format!(
            "desktop portal does not answer: {}",
            crate::util::cut(o.stderr.trim(), 120)
        ),
        None => "gdbus is not installed, portal not probed".to_owned(),
    });
    Ok(())
}

fn monitors(ctx: &mut Ctx) -> R {
    let r = runner();
    let found = lx::list_monitors(&r).map_err(Stop::Fail)?;
    check!(!found.is_empty(), "xrandr lists no monitor");
    for m in &found {
        ctx.note(format!(
            "xrandr: {} {}x{} at ({},{}){}",
            m.name,
            m.w,
            m.h,
            m.x,
            m.y,
            if m.primary { " primary" } else { "" }
        ));
    }
    ctx.start_daemon()?;
    let v = ctx.cli_ok(&["targets"])?;
    let ds = displays(&v);
    for d in &ds {
        ctx.note(format!(
            "product: display {} '{}' {}x{} at ({},{}) scale {}",
            d.id, d.name, d.w, d.h, d.x, d.y, d.scale
        ));
    }
    check!(!ds.is_empty(), "the product lists no display");
    if ds.len() != found.len() {
        let msg = format!(
            "the product lists {} display(s), xrandr {} monitor(s)",
            ds.len(),
            found.len()
        );
        // Under XWayland xrandr shows what GNOME exposes to X clients, which can merge or hide a monitor:
        // a difference is evidence, not proof, there.
        if lx::session_kind(&env_get) == lx::Session::X11 {
            return fail(msg);
        }
        ctx.note(format!(
            "{msg} (Wayland session: xrandr goes through XWayland, not compared strictly)"
        ));
    }
    Ok(())
}

struct App {
    pid: u32,
    tracker: crate::proc::Tracker,
}

impl Drop for App {
    fn drop(&mut self) {
        sys::kill_tree(self.pid);
        if let Ok(mut t) = self.tracker.lock() {
            t.retain(|p| *p != self.pid);
        }
    }
}

fn log_tail(ctx: &Ctx) -> String {
    let mut s = String::new();
    if let Ok(rd) = std::fs::read_dir(ctx.config.join("logs")) {
        for e in rd.flatten() {
            if let Ok(t) = std::fs::read_to_string(e.path()) {
                let lines: Vec<&str> = t.lines().collect();
                s.push_str(&lines[lines.len().saturating_sub(5)..].join(" | "));
            }
        }
    }
    crate::util::cut(&s, 500)
}

fn start_app(ctx: &mut Ctx) -> R<App> {
    let mut c = ctx.product_cmd("cm-desktop");
    c.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = c
        .spawn()
        .map_err(|e| Stop::Fail(format!("cm-desktop did not start: {e}")))?;
    let pid = child.id();
    if let Ok(mut t) = ctx.tracker.lock() {
        t.push(pid);
    }
    // Dropped on purpose: the tracker kills the process after the case.
    drop(child);
    let ok = crate::proc::wait_for(ctx.secs(60), Duration::from_millis(500), || {
        if !alive(pid) {
            return Some(false);
        }
        ctx.daemon_answers().then_some(true)
    });
    match ok {
        Some(true) => Ok(App {
            pid,
            tracker: ctx.tracker.clone(),
        }),
        Some(false) => fail(format!(
            "cm-desktop (pid {pid}) exited during start-up; log: {}",
            log_tail(ctx)
        )),
        None => fail(format!(
            "the app's daemon never answered on {}; log: {}",
            ctx.endpoint,
            log_tail(ctx)
        )),
    }
}

fn stays_up(ctx: &mut Ctx) -> R {
    let app = start_app(ctx)?;
    std::thread::sleep(Duration::from_secs(10));
    check!(
        alive(app.pid),
        "cm-desktop (pid {}) is gone after 10 s; log: {}",
        app.pid,
        log_tail(ctx)
    );
    match lx::window_probe_unavailable(&env_get, &runner()) {
        None => {
            let wins: Vec<_> = sys::enum_windows()
                .into_iter()
                .filter(|w| w.pid == app.pid && w.visible && w.width() > 50)
                .collect();
            ctx.note(format!(
                "{} visible window(s) of the app: {}",
                wins.len(),
                wins.iter()
                    .map(|w| w.brief())
                    .collect::<Vec<_>>()
                    .join("; ")
            ));
            check!(!wins.is_empty(), "the app shows no window after start-up");
        }
        Some(why) => ctx.note(format!(
            "window not checked: {why}; the app stayed up and its daemon answers"
        )),
    }
    Ok(())
}

fn second_launch(ctx: &mut Ctx) -> R {
    super::common::need_ffmpeg(ctx)?;
    let clips = super::common::write_clips(ctx)?;
    let baseline = crate::hygiene::product_pids();
    let app = start_app(ctx)?;
    std::thread::sleep(Duration::from_secs(3));
    let mut c = ctx.product_cmd("cm-desktop");
    c.arg(clips.join("clip-h264.mp4"));
    let out = ctx.run(c, 40)?;
    ctx.note(format!(
        "second launch: {} after {} ms",
        exit_text(out.code),
        out.elapsed_ms
    ));
    check!(
        !out.timed_out,
        "the second launch did not exit within 40 s (it should hand over and quit)"
    );
    check!(
        out.code == Some(0),
        "the second launch ended with {}",
        exit_text(out.code)
    );
    std::thread::sleep(Duration::from_secs(3));
    check!(
        alive(app.pid),
        "the first instance died when the second one forwarded a file"
    );
    let others = sys::list_processes()
        .iter()
        .filter(|(p, n)| *p != app.pid && !baseline.contains(p) && n.starts_with("cm-desktop"))
        .count();
    check!(
        others == 0,
        "{others} other cm-desktop process(es) are still running: the second launch did not hand over"
    );
    Ok(())
}

/// Items of the tray that belong to a program with this pid (`org.kde.StatusNotifierItem-<pid>-<n>`), or
/// to CaptureMann by name (an AppIndicator item path).
fn tray_mine(items: &[String], pid: u32) -> Vec<String> {
    items
        .iter()
        .filter(|i| i.contains(&format!("-{pid}-")) || i.to_lowercase().contains("capturemann"))
        .cloned()
        .collect()
}

fn tray_icon(ctx: &mut Ctx) -> R {
    let r = runner();
    let before = match lx::tray_items(&r) {
        Ok(v) => v,
        Err(e) => return skip(e),
    };
    let app = start_app(ctx)?;
    std::thread::sleep(Duration::from_secs(6));
    let after = lx::tray_items(&r).map_err(Stop::Fail)?;
    let mine = tray_mine(&after, app.pid);
    let had = tray_mine(&before, app.pid).len();
    ctx.note(format!(
        "tray items: {} before, {} with the app; CaptureMann's: {}",
        before.len(),
        after.len(),
        mine.join(", ")
    ));
    check!(
        alive(app.pid),
        "cm-desktop (pid {}) is gone after the probe",
        app.pid
    );
    check!(
        mine.len() > had,
        "no new CaptureMann item registered with the StatusNotifierWatcher ({} items before, {} after)",
        before.len(),
        after.len()
    );
    Ok(())
}

fn hotkey_overlay(_ctx: &mut Ctx) -> R {
    skip(
        "no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md)",
    )
}

fn vaapi(ctx: &mut Ctx) -> R {
    let r = runner();
    let va = lx::vaapi_report(&r).map_err(Stop::Skip)?;
    ctx.note(format!("vainfo: {}", va.describe()));
    ctx.note(format!("profiles: {}", va.profiles.join(", ")));
    ctx.start_daemon()?;
    let d = ctx.cli_text(&["diagnostics"])?;
    check!(d.ok(), "diagnostics failed: {}", exit_text(d.out.code));
    let text = d.out.stdout.clone();
    let line = text
        .lines()
        .find(|l| l.contains("VAAPI render device"))
        .map(str::trim)
        .unwrap_or("");
    ctx.note(format!(
        "diagnostics: {}",
        if line.is_empty() {
            "(no VAAPI line)"
        } else {
            line
        }
    ));
    check!(
        !line.is_empty(),
        "diagnostics has no `VAAPI render device` line"
    );
    let says_none = line.ends_with("none");
    let note_software = text.contains("software VP9");
    if va.h264_encode {
        check!(
            !says_none,
            "vainfo lists H.264 encode ({}) but diagnostics reports no VAAPI render device",
            va.driver
        );
        check!(
            !note_software,
            "vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder"
        );
    } else {
        ctx.note("this machine records with the software VP9 encoder (no VA-API H.264 encode)");
    }
    Ok(())
}

// ----- installs ---------------------------------------------------------------------------------------

fn installer_file(ctx: &Ctx, suffixes: &[&str]) -> R<PathBuf> {
    let Some(p) = ctx.opts.installer.clone() else {
        return skip("no --installer given");
    };
    let name = p.to_string_lossy().to_lowercase();
    if !suffixes.iter().any(|s| name.ends_with(s)) {
        return skip(format!(
            "--installer is {}, this case wants a file ending in {}",
            p.display(),
            suffixes.join(" or ")
        ));
    }
    check!(p.is_file(), "--installer {} does not exist", p.display());
    Ok(p)
}

/// The programs a package must hold and their dynamic libraries must resolve.
const PROGRAMS: &[&str] = &[
    "capturemann",
    "cm-mcp",
    "cm-daemon",
    "cm-desktop",
    "cm-overlay",
];

/// Libraries `ldd` could not find (pure, tested below).
fn missing_libs(ldd_text: &str) -> Vec<String> {
    ldd_text
        .lines()
        .filter(|l| l.contains("not found"))
        .map(|l| l.trim().to_owned())
        .collect()
}

fn run_plain(ctx: &mut Ctx, program: &str, args: &[&str], secs: u64) -> R<crate::proc::Output> {
    let mut c = Command::new(program);
    c.args(args).env("DEBIAN_FRONTEND", "noninteractive");
    ctx.run(c, secs)
}

fn check_programs(ctx: &mut Ctx, dir: &Path, version_hint: Option<&str>) -> R {
    for p in PROGRAMS {
        let exe = dir.join(p);
        if !exe.is_file() {
            // cm-overlay and cm-daemon are optional in a reduced build; the CLI and the app are not.
            check!(
                !matches!(*p, "capturemann" | "cm-desktop" | "cm-mcp"),
                "{} is missing",
                exe.display()
            );
            ctx.note(format!("{} not in the package", exe.display()));
            continue;
        }
        let exe_s = exe.to_string_lossy().into_owned();
        let ldd = run_plain(ctx, "ldd", &[&exe_s], 30)?;
        let missing = missing_libs(&ldd.stdout);
        check!(
            missing.is_empty(),
            "{p} cannot find its libraries: {}",
            missing.join("; ")
        );
    }
    let exe = dir.join("capturemann");
    let mut c = Command::new(&exe);
    c.arg("--version");
    let v = ctx.run(c, 30)?;
    check!(
        v.success(),
        "{} --version failed: {}",
        exe.display(),
        exit_text(v.code)
    );
    ctx.note(format!("{} --version: {}", exe.display(), v.stdout.trim()));
    if let Some(want) = version_hint {
        check!(
            v.stdout.contains(want),
            "capturemann --version says `{}`, the package says {want}",
            v.stdout.trim()
        );
    }
    Ok(())
}

/// What `pkg.files` wants of a package, for an installed one: the licence and third-party licence documents
/// in the documentation folder and the LGPL source offer next to the programs.
fn check_package_documents(ctx: &mut Ctx, doc_dir: &Path, lib_dir: &Path) -> R {
    let has = |dir: &Path, prefix: &str| {
        std::fs::read_dir(dir).is_ok_and(|rd| {
            rd.flatten().any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .to_lowercase()
                    .starts_with(prefix)
            })
        })
    };
    let mut missing = Vec::new();
    for prefix in ["license", "third_party_licenses"] {
        if !has(doc_dir, prefix) {
            missing.push(format!("no {prefix}* file in {}", doc_dir.display()));
        }
    }
    for rel in ["FFmpeg/SOURCE-OFFER.txt", "FFmpeg/BUILD-INFO.txt"] {
        if !lib_dir.join(rel).is_file() {
            missing.push(format!("{rel} is missing in {}", lib_dir.display()));
        }
    }
    ctx.note(format!("documents checked in {}", doc_dir.display()));
    check!(missing.is_empty(), "{}", missing.join("; "));
    Ok(())
}

fn deb_install(ctx: &mut Ctx) -> R {
    let deb = installer_file(ctx, &[".deb"])?;
    let r = runner();
    if !lx::passwordless_sudo(&r) {
        return skip("sudo needs a password here (the lab laptop has passwordless sudo, scripts/testlab/linux-setup.sh)");
    }
    match lx::deb_state(&r, "capturemann") {
        lx::DebState::NotInstalled => {}
        other => {
            return skip(format!(
                "a capturemann package is already on this machine ({other:?}); the case would replace it. Run `sudo apt-get purge capturemann` first, or use the tarball case"
            ))
        }
    }
    let deb_s = deb.to_string_lossy().into_owned();
    let field = |f: &str| -> String {
        r.run("dpkg-deb", &["--field", &deb_s, f])
            .map(|o| o.stdout.trim().to_owned())
            .unwrap_or_default()
    };
    check!(
        field("Package") == "capturemann",
        "the .deb's Package is `{}`",
        field("Package")
    );
    let version = field("Version");
    check!(!version.is_empty(), "the .deb has no Version");
    ctx.note(format!("{} version {version}", deb.display()));

    let result = deb_cycle(ctx, &deb_s, &version);
    // Whatever happened, leave the machine without the package (purge also removes config files).
    let _ = run_plain(
        ctx,
        "sudo",
        &["-n", "apt-get", "purge", "-y", "capturemann"],
        240,
    );
    let left = lx::deb_state(&r, "capturemann");
    result?;
    check!(
        left == lx::DebState::NotInstalled,
        "after purge dpkg still reports {left:?}"
    );
    check!(
        !Path::new("/opt/capturemann").exists(),
        "/opt/capturemann is still there after purge"
    );
    for b in PROGRAMS {
        check!(
            !Path::new("/usr/bin").join(b).exists(),
            "/usr/bin/{b} is still there after purge"
        );
    }
    Ok(())
}

fn deb_cycle(ctx: &mut Ctx, deb: &str, version: &str) -> R {
    let r = runner();
    let out = run_plain(ctx, "sudo", &["-n", "apt-get", "install", "-y", deb], 300)?;
    check!(
        out.success(),
        "apt-get install failed ({}): {}",
        exit_text(out.code),
        crate::util::cut(&crate::util::one_line(&out.stderr), 300)
    );
    check!(
        lx::deb_state(&r, "capturemann") == lx::DebState::Installed(version.to_owned()),
        "dpkg does not report capturemann {version} as installed"
    );
    let dir = Path::new("/opt/capturemann/lib/capturemann");
    check_programs(ctx, dir, version.split(['-', '+']).next())?;
    check_package_documents(ctx, Path::new("/usr/share/doc/capturemann"), dir)?;
    for f in [
        "/usr/bin/capturemann",
        "/usr/bin/cm-desktop",
        "/usr/share/applications/dev.capturemann.app.desktop",
    ] {
        check!(Path::new(f).exists(), "{f} is missing after install");
    }
    let out = run_plain(
        ctx,
        "sudo",
        &["-n", "apt-get", "remove", "-y", "capturemann"],
        240,
    )?;
    check!(
        out.success(),
        "apt-get remove failed: {}",
        crate::util::cut(&crate::util::one_line(&out.stderr), 300)
    );
    for f in [
        "/usr/bin/capturemann",
        "/usr/bin/cm-desktop",
        "/usr/share/applications/dev.capturemann.app.desktop",
    ] {
        check!(!Path::new(f).exists(), "{f} is still there after remove");
    }
    check!(
        !dir.join("capturemann").exists(),
        "the programs are still in {} after remove",
        dir.display()
    );
    Ok(())
}

fn tarball_install(ctx: &mut Ctx) -> R {
    let tar = installer_file(ctx, &[".tar.gz", ".tgz"])?;
    let unpack = ctx.dir("tarball");
    let prefix = ctx.root.join("prefix");
    if prefix.to_string_lossy().contains(' ') {
        return skip("the temp folder has a space in its name");
    }
    let out = run_plain(
        ctx,
        "tar",
        &[
            "-xzf",
            &tar.to_string_lossy(),
            "-C",
            &unpack.to_string_lossy(),
        ],
        180,
    )?;
    check!(out.success(), "tar failed: {}", exit_text(out.code));
    let script = std::fs::read_dir(&unpack)?
        .flatten()
        .map(|e| e.path().join("install.sh"))
        .find(|p| p.is_file());
    let Some(script) = script else {
        return fail("no install.sh in the tarball's top folder");
    };
    let (script_s, prefix_s) = (
        script.to_string_lossy().into_owned(),
        prefix.to_string_lossy().into_owned(),
    );
    // The script writes only under --prefix; HOME is pointed into the sandbox as well so a stray
    // write would not land in the lab account.
    let home = ctx.dir("home");
    let mut c = Command::new("sh");
    c.args([&script_s, "--prefix", &prefix_s])
        .env("HOME", &home);
    let out = ctx.run(c, 120)?;
    check!(
        out.success(),
        "install.sh failed ({}): {}",
        exit_text(out.code),
        crate::util::cut(&crate::util::one_line(&out.stderr), 300)
    );
    let lib = prefix.join("lib/capturemann");
    for p in ["capturemann", "cm-desktop", "cm-mcp"] {
        let link = prefix.join("bin").join(p);
        check!(link.exists(), "{} is missing after install", link.display());
    }
    check_programs(ctx, &lib, None)?;
    check_package_documents(ctx, &prefix.join("share/doc/capturemann"), &lib)?;
    check!(
        prefix
            .join("share/applications/dev.capturemann.app.desktop")
            .is_file(),
        "the .desktop entry is missing"
    );
    let mut c = Command::new("sh");
    c.args([&script_s, "--uninstall", "--prefix", &prefix_s])
        .env("HOME", &home);
    let out = ctx.run(c, 120)?;
    check!(
        out.success(),
        "install.sh --uninstall failed: {}",
        exit_text(out.code)
    );
    check!(
        !lib.exists(),
        "{} is still there after --uninstall",
        lib.display()
    );
    for p in PROGRAMS {
        check!(
            !prefix.join("bin").join(p).exists(),
            "bin/{p} is still there after --uninstall"
        );
    }
    check!(
        !prefix
            .join("share/applications/dev.capturemann.app.desktop")
            .exists(),
        "the .desktop entry is still there after --uninstall"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ldd_missing_libraries_are_found() {
        let t = "\tlinux-vdso.so.1 (0x00007ffc)\n\tlibxdo.so.3 => not found\n\tlibc.so.6 => /lib/x86_64-linux-gnu/libc.so.6 (0x1)\n";
        assert_eq!(missing_libs(t), vec!["libxdo.so.3 => not found"]);
        assert!(missing_libs("\tlibc.so.6 => /lib/libc.so.6\n").is_empty());
    }

    #[test]
    fn tray_items_of_the_app_are_matched_by_pid_or_name() {
        let items = vec![
            "org.kde.StatusNotifierItem-4242-1/StatusNotifierItem".to_owned(),
            ":1.9/org/ayatana/NotificationItem/dropbox".to_owned(),
            ":1.55/org/ayatana/NotificationItem/dev_capturemann_app".to_owned(),
        ];
        assert_eq!(tray_mine(&items, 4242).len(), 2);
        assert_eq!(tray_mine(&items, 1).len(), 1);
    }

    #[test]
    fn linux_case_ids_use_the_linux_area() {
        for c in cases() {
            assert!(c.id.starts_with("linux."));
            assert_eq!(c.platforms.len(), 1);
        }
    }
}
