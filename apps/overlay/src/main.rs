//! `cm-overlay <region|window|display>`: see the library docs.

use std::process::ExitCode;
use std::time::Instant;

use cm_overlay::args::{parse, Parsed, USAGE};
use cm_overlay::error::OverlayError;

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
    match cm_overlay::run(&args, started) {
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}
