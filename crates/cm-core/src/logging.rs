//! A minimal stderr logger for the binaries (the `log` facade, no extra dependencies).
//!
//! `RUST_LOG` takes the common `env_logger` forms: a bare level (`debug`) that applies to every
//! crate, or a comma list of `target=level` pairs with an optional bare level (`warn,cm_daemon=debug`).
//! Without it, the `cm_*` crates log at `info` and everything else (GPUI, wgpu, ...) at `warn`.

use std::io::Write;

use log::{LevelFilter, Log, Metadata, Record};

struct Logger {
    /// Level for crates that are not ours.
    default: LevelFilter,
    /// Level for `cm_*` crates.
    ours: LevelFilter,
    /// `(target, level)` pairs from `RUST_LOG`.
    targets: Vec<(String, LevelFilter)>,
}

impl Logger {
    fn parse(spec: Option<&str>) -> Logger {
        let mut logger = Logger {
            default: LevelFilter::Warn,
            ours: LevelFilter::Info,
            targets: Vec::new(),
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
        }
    }

    fn flush(&self) {}
}

/// Installs the stderr logger (`RUST_LOG` overrides the defaults). Later calls do nothing.
pub fn init() {
    let logger = Logger::parse(std::env::var("RUST_LOG").ok().as_deref());
    let max = logger.max_level();
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(max);
    }
}

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
}
