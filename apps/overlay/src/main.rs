//! `cm-overlay <region|window|display>`: see the library docs.

use std::process::ExitCode;
use std::time::Instant;

use cm_overlay::args::{parse, Parsed, USAGE};
use cm_overlay::error::OverlayError;
use cm_overlay::lifetime;

fn fail(e: &OverlayError) -> ExitCode {
    if e.code == "user_declined" {
        println!("{}", serde_json::json!({ "cancelled": true }));
    }
    eprintln!("{}", e.render_json());
    ExitCode::from(u8::try_from(e.exit).unwrap_or(1))
}

/// The environment variable the desktop app sets when it starts the overlay: the code of the UI
/// language (`appearance.language`, resolved). The overlay is a separate process, so it cannot
/// share the app's global; without the variable (a CLI run) the hints are English.
const LANG_ENV: &str = "CAPTUREMANN_LANG";

/// The folder for the overlay's log file (DIAG-1), set by the desktop app; without it the overlay
/// logs to stderr only.
const LOG_DIR_ENV: &str = "CAPTUREMANN_LOG_DIR";

fn main() -> ExitCode {
    let started = Instant::now();
    // The app tells the overlay where its log files go (the overlay does not link the daemon).
    match std::env::var_os(LOG_DIR_ENV).filter(|v| !v.is_empty()) {
        Some(dir) => cm_core::logging::init_with_file(cm_core::logfile::FileLogConfig {
            process: "overlay",
            dir: dir.into(),
            version: env!("CARGO_PKG_VERSION"),
            scrub: None,
        }),
        None => cm_core::logging::init(),
    }
    cm_i18n::set_language(cm_i18n::Lang::from_setting(
        std::env::var(LANG_ENV).ok().as_deref(),
    ));
    let parsed = parse(
        std::env::args().skip(1),
        std::env::var("CAPTUREMANN_SOCKET").ok(),
    );
    let args = match parsed {
        Ok(Parsed::Run(a)) => a,
        Ok(Parsed::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("{}\n{USAGE}", e.render_json());
            return ExitCode::from(u8::try_from(e.exit).unwrap_or(2));
        }
    };
    let parent = lifetime::parent_pid_from(std::env::var(lifetime::PARENT_PID_ENV).ok().as_deref());
    let deadline = lifetime::deadline_from(std::env::var(lifetime::DEADLINE_ENV).ok().as_deref());
    log::info!(
        "overlay started: pid {}, mode {:?}, pick {}, endpoint {}, parent pid {:?}, deadline {} s",
        std::process::id(),
        args.mode,
        args.pick,
        args.endpoint.describe(),
        parent,
        deadline.as_secs()
    );
    // A stray overlay (the app was killed, or the daemon never answers) ends itself and says so.
    lifetime::guard(deadline, parent, |reason| {
        log::error!("overlay: {}", reason.describe());
        let e = OverlayError::internal(reason.describe());
        let code = e.exit;
        // Whatever blocks below (a full stderr pipe, a stuck exit handler), the process ends:
        // `TerminateProcess` / exit after a second at the latest.
        let _ = std::thread::Builder::new()
            .name("overlay-hard-exit".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(1));
                cm_overlay::platform::hard_exit(code);
            });
        eprintln!("{}", e.render_json());
        cm_overlay::platform::hard_exit(code);
    });
    let result = cm_overlay::run(&args, started);
    log::info!(
        "overlay finished after {} ms: {}",
        started.elapsed().as_millis(),
        match &result {
            Ok(v) if v.get("capture_id").is_some() => "capture made".to_owned(),
            Ok(_) => "ok".to_owned(),
            Err(e) => format!("{} ({})", e.code, e.exit),
        }
    );
    match result {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}
