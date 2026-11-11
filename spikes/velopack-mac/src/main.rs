// Throw-away spike for the Velopack evaluation (0.3.2). Never merged.
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use velopack::*;

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()
}

fn log(line: &str) {
    let msg = format!("{} pid={} ver={} {}\n", now_ms(), std::process::id(), option_env!("SPIKE_VER").unwrap_or("unset"), line);
    print!("{msg}");
    // `open -n` (used by UpdateMac to relaunch) does not pass the environment, so default to a fixed scratch file.
    if let Ok(p) = std::env::var("VELO_LOG").or_else(|_| Ok::<String, ()>("/tmp/cm-velo-mac/logs/run.log".into())) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
            let _ = f.write_all(msg.as_bytes());
        }
    }
}

fn main() {
    let auto = std::env::var("VELO_AUTO").is_ok();
    // Real app: must be first in main(). auto_apply=false so nothing applies behind our back.
    VelopackApp::build().set_auto_apply_on_startup(auto).on_restarted(|v| log(&format!("on_restarted hook v{v}"))).on_first_run(|v| log(&format!("on_first_run hook v{v}"))).run();

    let args: Vec<String> = std::env::args().collect();
    log(&format!("start args={:?} exe={:?}", &args[1..], std::env::current_exe().unwrap()));
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("version");
    let feed = args.get(2).cloned().unwrap_or_else(|| "http://127.0.0.1:8099".into());

    match cmd {
        "version" => {
            match UpdateManager::new(sources::HttpSource::new(&feed), None, None) {
                Ok(um) => log(&format!("installed by velopack: id={} version={} portable={}", um.get_app_id(), um.get_current_version_as_string(), um.get_is_portable())),
                Err(e) => log(&format!("UpdateManager::new error: {e:?}")),
            }
        }
        "after-update" => log("after-update: running new version"),
        "hold" => {
            // spawn children that outlive/are siblings, then idle (to watch what apply does to them)
            let c = std::process::Command::new("/bin/sleep").arg("600").spawn().unwrap();
            log(&format!("spawned child sleep pid={}", c.id()));
            std::thread::sleep(std::time::Duration::from_secs(600));
        }
        "check" | "download" | "update" | "update-silent" | "download-only" => {
            let um = match UpdateManager::new(sources::HttpSource::new(&feed), None, None) {
                Ok(u) => u,
                Err(e) => {
                    log(&format!("UpdateManager::new error: {e:?}"));
                    std::process::exit(2);
                }
            };
            let t0 = now_ms();
            let chk = um.check_for_updates();
            log(&format!("check took {} ms", now_ms() - t0));
            match chk {
                Err(e) => {
                    log(&format!("check error: {e:?}"));
                    std::process::exit(3);
                }
                Ok(UpdateCheck::RemoteIsEmpty) => log("remote empty"),
                Ok(UpdateCheck::NoUpdateAvailable) => log("no update"),
                Ok(UpdateCheck::UpdateAvailable(info)) => {
                    log(&format!(
                        "update available: target={} type={} size={} base={:?} deltas={}",
                        info.TargetFullRelease.Version,
                        info.TargetFullRelease.Type,
                        info.TargetFullRelease.Size,
                        info.BaseRelease.as_ref().map(|b| b.FileName.clone()),
                        info.DeltasToTarget.iter().map(|d| format!("{}:{}", d.FileName, d.Size)).collect::<Vec<_>>().join(",")
                    ));
                    if cmd == "check" {
                        return;
                    }
                    let t1 = now_ms();
                    let (tx, rx) = std::sync::mpsc::channel();
                    let r = um.download_updates(&info, Some(tx));
                    let last = rx.try_iter().last();
                    log(&format!("download took {} ms result={:?} last_progress={:?}", now_ms() - t1, r.as_ref().map(|_| "ok"), last));
                    if r.is_err() {
                        std::process::exit(4);
                    }
                    log(&format!("pending restart: {:?}", um.get_update_pending_restart().map(|a| a.FileName)));
                    if cmd == "download-only" {
                        return;
                    }
                    // Here a real app would verify its own detached signature of packages/<FileName>.
                    log("applying and restarting");
                    if cmd == "update-silent" {
                        // silent: UpdateMac must not ask for administrator permission (no dialog)
                        let r = um.wait_exit_then_apply_updates(&info.TargetFullRelease, true, true, ["after-update"]);
                        log(&format!("wait_exit_then_apply_updates(silent) returned: {r:?}; exiting so updater can run"));
                        std::process::exit(0);
                    }
                    let r = um.apply_updates_and_restart_with_args(&info.TargetFullRelease, ["after-update"]);
                    log(&format!("apply returned (should not): {r:?}"));
                }
            }
        }
        other => log(&format!("unknown command {other}")),
    }
}
