//! A minimal stderr logger for the binaries (the `log` facade, no extra dependencies).
//!
//! `RUST_LOG` takes the common `env_logger` forms: a bare level (`debug`) that applies to every
//! crate, or a comma list of `target=level` pairs with an optional bare level (`warn,cm_daemon=debug`).
//! Without it, the `cm_*` crates log at `info` and everything else (GPUI, wgpu, ...) at `warn`.

use std::io::Write;
use std::sync::{Arc, OnceLock};

use log::{LevelFilter, Log, Metadata, Record};

use crate::logfile::{one_line_backtrace, FileLogConfig, FileSink};

struct Logger {
    /// Level for crates that are not ours.
    default: LevelFilter,
    /// Level for `cm_*` crates.
    ours: LevelFilter,
    /// `(target, level)` pairs from `RUST_LOG`.
    targets: Vec<(String, LevelFilter)>,
    /// The log file (DIAG-1), when the process opened one.
    file: Option<Arc<FileSink>>,
}

impl Logger {
    fn parse(spec: Option<&str>) -> Logger {
        let mut logger = Logger {
            default: LevelFilter::Warn,
            ours: LevelFilter::Info,
            targets: Vec::new(),
            file: None,
        };
        for part in spec.unwrap_or("").split(',').map(str::trim) {
            match part.split_once('=') {
                Some((t, l)) => {
                    if let Ok(l) = l.trim().parse() {
                        logger.targets.push((t.trim().replace('-', "_"), l));
                    }
                }
                None => {
                    if let Ok(l) = part.parse() {
                        logger.default = l;
                        logger.ours = l;
                    }
                }
            }
        }
        logger
    }

    fn level_for(&self, target: &str) -> LevelFilter {
        let named = self
            .targets
            .iter()
            .filter(|(p, _)| {
                target == p
                    || target
                        .strip_prefix(p.as_str())
                        .is_some_and(|rest| rest.starts_with("::"))
            })
            .max_by_key(|(p, _)| p.len());
        match named {
            Some((_, l)) => *l,
            None if target.starts_with("cm_") => self.ours,
            None => self.default,
        }
    }

    fn max_level(&self) -> LevelFilter {
        self.targets
            .iter()
            .map(|(_, l)| *l)
            .chain([self.default, self.ours])
            .max()
            .unwrap_or(LevelFilter::Warn)
    }
}

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= self.level_for(m.target())
    }

    fn log(&self, r: &Record) {
        if self.enabled(r.metadata()) {
            // A failed write to stderr has nowhere to be reported.
            let _ = writeln!(
                std::io::stderr().lock(),
                "[{} {}] {}",
                r.level(),
                r.target(),
                r.args()
            );
            if let Some(f) = &self.file {
                f.write(r.level().as_str(), r.target(), &r.args().to_string());
            }
        }
    }

    fn flush(&self) {}
}

/// Installs the stderr logger (`RUST_LOG` overrides the defaults). Later calls do nothing.
pub fn init() {
    install(Logger::parse(std::env::var("RUST_LOG").ok().as_deref()));
}

fn install(logger: Logger) -> bool {
    let max = logger.max_level();
    let ok = log::set_boxed_logger(Box::new(logger)).is_ok();
    if ok {
        log::set_max_level(max);
    }
    ok
}

/// The sink the exit handler closes (`atexit` takes no argument).
static EXIT_SINK: OnceLock<Arc<FileSink>> = OnceLock::new();

/// [`init`] plus a log file: the same records go to `cfg.dir` (DIAG-1). Also records panics, writes
/// the start line, reports a previous run that did not end normally and removes the run marker on a
/// normal exit. A file that cannot be opened leaves the process logging to stderr only.
pub fn init_with_file(cfg: FileLogConfig) {
    let mut logger = Logger::parse(std::env::var("RUST_LOG").ok().as_deref());
    let sink = FileSink::open(&cfg).map(Arc::new);
    logger.file.clone_from(&sink);
    if !install(logger) {
        return;
    }
    let Some(sink) = sink else {
        log::warn!("the log file could not be opened; logging to stderr only");
        return;
    };
    let unclean = sink.begin_run();
    log::info!("{} {} started", cfg.process, cfg.version);
    if unclean {
        log::warn!(
            "the previous run of {} did not end normally (a crash, a force quit or a power loss)",
            cfg.process
        );
    }
    install_panic_hook(Arc::clone(&sink));
    if EXIT_SINK.set(sink).is_ok() {
        register_exit_handler();
    }
}

/// Writes every panic to the log file, then runs the hook that was there before.
pub fn install_panic_hook(sink: Arc<FileSink>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let location = info
            .location()
            .map_or_else(|| "unknown".to_owned(), ToString::to_string);
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_owned())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "(not a text)".to_owned());
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        sink.write_panic(&location, &payload, &one_line_backtrace(&backtrace, 3000));
        previous(info);
    }));
}

#[cfg(unix)]
fn register_exit_handler() {
    extern "C" fn at_exit() {
        if let Some(sink) = EXIT_SINK.get() {
            sink.end_run();
        }
    }
    // SAFETY: `at_exit` is a plain function that touches only a static; registering it is sound.
    unsafe {
        libc::atexit(at_exit);
    }
}

// No exit handler on other systems yet: the marker stays and the next start reports it, so a
// normal exit would be reported as unclean. Windows gets its own handler with the Windows port.
#[cfg(not(unix))]
fn register_exit_handler() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_show_info_for_our_crates_and_warn_for_others() {
        let l = Logger::parse(None);
        assert_eq!(l.level_for("cm_daemon::watcher"), LevelFilter::Info);
        assert_eq!(l.level_for("wgpu_core"), LevelFilter::Warn);
    }

    #[test]
    fn a_bare_level_applies_everywhere() {
        let l = Logger::parse(Some("debug"));
        assert_eq!(l.level_for("cm_core"), LevelFilter::Debug);
        assert_eq!(l.level_for("gpui"), LevelFilter::Debug);
    }

    #[test]
    fn the_longest_target_wins() {
        let l = Logger::parse(Some("warn,cm_daemon=debug,cm_daemon::jobs=off"));
        assert_eq!(l.level_for("cm_daemon::watcher"), LevelFilter::Debug);
        assert_eq!(l.level_for("cm_daemon::jobs"), LevelFilter::Off);
        assert_eq!(l.level_for("cm_daemon_other"), LevelFilter::Warn);
        assert_eq!(l.level_for("cm_core"), LevelFilter::Warn);
        assert_eq!(l.max_level(), LevelFilter::Debug);
    }

    #[test]
    fn junk_is_ignored() {
        let l = Logger::parse(Some("loud,cm_core=loud"));
        assert_eq!(l.level_for("cm_core"), LevelFilter::Info);
    }

    // DIAG-1.AC3: a real panic on a thread reaches the file.
    #[test]
    fn a_panic_on_a_thread_is_written_to_the_log_file() {
        let d = tempfile::tempdir().unwrap();
        let sink = Arc::new(
            FileSink::open(&FileLogConfig {
                process: "desktop",
                dir: d.path().to_path_buf(),
                version: "0.0.0",
                scrub: None,
            })
            .unwrap(),
        );
        let before = std::panic::take_hook();
        install_panic_hook(sink);
        let r = std::thread::spawn(|| panic!("native callback failed")).join();
        drop(std::panic::take_hook());
        std::panic::set_hook(before);
        assert!(r.is_err());
        let text = std::fs::read_to_string(d.path().join("desktop.log")).unwrap();
        assert!(text.contains("ERROR panic: panic at"), "{text}");
        assert!(text.contains("native callback failed"), "{text}");
        assert!(text.contains("backtrace:"), "{text}");
    }
}
