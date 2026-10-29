//! Where the log files are and how a binary starts them (DIAG-1).

use std::path::PathBuf;

use cm_core::logfile::FileLogConfig;

/// The log folder: `logs` inside [`crate::config::config_dir`] (so `CAPTUREMANN_CONFIG_DIR` moves it).
pub fn log_dir() -> PathBuf {
    crate::config::config_dir().join("logs")
}

/// Starts the stderr logger and the log file of `process` (`desktop`, `daemon` or `overlay`);
/// secrets are removed from every line before it is written.
pub fn init_logging(process: &'static str) {
    cm_core::logging::init_with_file(FileLogConfig {
        process,
        dir: log_dir(),
        version: env!("CARGO_PKG_VERSION"),
        scrub: Some(cm_jobs::scrub_secrets),
    });
}
