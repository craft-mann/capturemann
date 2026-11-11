// Scratch code for the 0.3.2 Velopack spike. Not product code. Shared by the three scratch bins.
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use velopack::*;

fn ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

pub fn log(line: &str) {
    let path = std::env::var("VS_LOG").unwrap_or_else(|_| r"C:\lab\velospike\spike.log".to_string());
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{} pid={} {}", ms(), std::process::id(), line);
    }
}

fn flag(name: &str) -> bool {
    std::path::Path::new(&format!(r"C:\lab\velospike\flags\{}", name)).exists()
}

/// What the product could do in the install hook: edit the per-user PATH with the path.ps1 that ships in
/// the package, only when the flag file `add_path` exists (so a plain run never touches PATH).
fn path_hook(action: &str) {
    if !flag("add_path") {
        return;
    }
    let dir = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())).unwrap();
    let t = std::time::Instant::now();
    let ps = if flag("abs_powershell") {
        format!(r"{}\System32\WindowsPowerShell\v1.0\powershell.exe", std::env::var("SystemRoot").unwrap_or_default())
    } else {
        "powershell.exe".to_string()
    };
    log(&format!("path_hook env PATH len={:?} SystemRoot={:?}", std::env::var("PATH").map(|p| p.len()), std::env::var("SystemRoot")));
    let st = std::process::Command::new(ps)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(dir.join("path.ps1"))
        .args(["-Action", action, "-Dir"])
        .arg(&dir)
        .status();
    log(&format!("path_hook {action} dir={} status={:?} ms={}", dir.display(), st.map(|s| s.code()), t.elapsed().as_millis()));
}

/// Velopack startup logic; `apply_on_startup` is false the way the product would set it.
pub fn velopack_startup(auto_apply: bool) {
    log(&format!("main-enter args={:?}", std::env::args().skip(1).collect::<Vec<_>>()));
    VelopackApp::build()
        .set_auto_apply_on_startup(auto_apply)
        .on_after_install_fast_callback(|v| {
            log(&format!("HOOK after_install {v}"));
            path_hook("add");
            if flag("fail_after_install") {
                log("HOOK after_install: exiting 5");
                std::process::exit(5);
            }
        })
        .on_after_update_fast_callback(|v| {
            log(&format!("HOOK after_update {v}"));
            if flag("fail_after_update") {
                log("HOOK after_update: exiting 5");
                std::process::exit(5);
            }
        })
        .on_before_update_fast_callback(|v| log(&format!("HOOK before_update {v}")))
        .on_before_uninstall_fast_callback(|v| {
            log(&format!("HOOK before_uninstall {v}"));
            path_hook("remove");
        })
        .on_first_run(|v| log(&format!("HOOK first_run {v}")))
        .on_restarted(|v| log(&format!("HOOK restarted {v}")))
        .run();
    log("velopack-run-returned");
}

pub fn app_main(args: &[String]) {
    if flag("crash_on_start") {
        log("crash_on_start flag: exit 3");
        std::process::exit(3);
    }
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("info");
    let url = args.get(1).cloned().unwrap_or_default();
    match cmd {
        "info" => info(),
        "check" => {
            let um = mgr(&url);
            report_check(&um);
        }
        "get" => {
            let um = mgr(&url);
            if let Some(ui) = report_check(&um) {
                download(&um, &ui);
            }
        }
        "apply" => {
            let um = mgr(&url);
            match um.get_update_pending_restart() {
                Some(a) => {
                    log(&format!("apply pending {}", a.Version));
                    let r = um.apply_updates_and_restart(&a);
                    log(&format!("apply returned {:?}", r.is_ok()));
                }
                None => log("apply: nothing pending"),
            }
        }
        "update" => {
            let um = mgr(&url);
            if let Some(ui) = report_check(&um) {
                if download(&um, &ui) {
                    log("update: applying and restarting");
                    let r = um.apply_updates_and_restart(&ui.TargetFullRelease);
                    log(&format!("apply returned ok={}", r.is_ok()));
                }
            }
        }
        "sleep" => {
            let n: u64 = url.parse().unwrap_or(600);
            log(&format!("sleeping {n}s"));
            std::thread::sleep(std::time::Duration::from_secs(n));
            log("sleep done");
        }
        "version" => println!("velospike {}", env!("CARGO_PKG_VERSION")),
        other => log(&format!("unknown command {other}")),
    }
}

fn mgr(url: &str) -> UpdateManager {
    match UpdateManager::new(sources::HttpSource::new(url), None, None) {
        Ok(m) => m,
        Err(e) => {
            log(&format!("UpdateManager::new ERR {e:?}"));
            std::process::exit(2);
        }
    }
}

fn info() {
    let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    match UpdateManager::new(sources::NoneSource {}, None, None) {
        Ok(m) => {
            let s = format!(
                "info exe={exe} installed=true version={} id={} portable={} pending={:?}",
                m.get_current_version_as_string(),
                m.get_app_id(),
                m.get_is_portable(),
                m.get_update_pending_restart().map(|a| a.Version)
            );
            log(&s);
            println!("{s}");
        }
        Err(e) => {
            let s = format!("info exe={exe} installed=false err={e:?}");
            log(&s);
            println!("{s}");
        }
    }
}

fn report_check(um: &UpdateManager) -> Option<UpdateInfo> {
    let t = std::time::Instant::now();
    let r = um.check_for_updates();
    let el = t.elapsed().as_millis();
    match r {
        Ok(UpdateCheck::UpdateAvailable(ui)) => {
            log(&format!(
                "check: UPDATE target={} size={} deltas={:?} base={:?} elapsed_ms={el}",
                ui.TargetFullRelease.Version,
                ui.TargetFullRelease.Size,
                ui.DeltasToTarget.iter().map(|d| (d.Version.clone(), d.Size)).collect::<Vec<_>>(),
                ui.BaseRelease.as_ref().map(|b| b.Version.clone()),
            ));
            Some(*ui)
        }
        Ok(UpdateCheck::NoUpdateAvailable) => {
            log(&format!("check: no update elapsed_ms={el}"));
            None
        }
        Ok(UpdateCheck::RemoteIsEmpty) => {
            log(&format!("check: remote empty elapsed_ms={el}"));
            None
        }
        Err(e) => {
            log(&format!("check: ERR {e:?} elapsed_ms={el}"));
            None
        }
    }
}

fn download(um: &UpdateManager, ui: &UpdateInfo) -> bool {
    let t = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel::<i16>();
    let h = std::thread::spawn(move || {
        let mut last = -1;
        while let Ok(p) = rx.recv() {
            last = p;
        }
        last
    });
    let r = um.download_updates(ui, Some(tx));
    let last = h.join().unwrap_or(-2);
    log(&format!("download: ok={} err={:?} last_progress={last} elapsed_ms={}", r.is_ok(), r.as_ref().err(), t.elapsed().as_millis()));
    r.is_ok()
}
