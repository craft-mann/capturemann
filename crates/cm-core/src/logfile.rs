//! The log file (DIAG-1): what the stderr logger prints, kept on disk so a problem on a user's Mac
//! can be read afterwards (an app opened from Finder has no visible stderr).
//!
//! * One file per process (`desktop.log`, `daemon.log`, `overlay.log`) in a `logs` folder; the
//!   current file plus the previous two, each at most [`MAX_FILE_BYTES`], so a process keeps at most
//!   [`MAX_FILES`] x 2 MB. Rotation happens when the cap is reached and at the start of a run whose
//!   file is already full. Writes are not buffered by us, so the last lines before a crash are on disk.
//! * Every line is `<UTC time> <process> <LEVEL> <target>: <message>` and every record is one line.
//! * Lines are redacted before they are written: the user's home folder becomes `~`, newlines are
//!   folded, and the caller's `scrub` function (the job store's `scrub_secrets`) removes tokens and
//!   secret query strings. Code must still not log transcript text, report text, tool parameters or
//!   tokens: redaction is the safety net, not the policy (see `docs/development/logging.md`).
//! * A panic is written with its location and a one-line backtrace. A `running-<process>.pid` marker
//!   exists while the process runs and is removed by a normal exit (`exit` handlers run, a crash,
//!   `abort` or force quit do not), so the next start can say that the previous run did not end
//!   normally.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

/// A log file is rotated when it reaches this size.
pub const MAX_FILE_BYTES: u64 = 2_000_000;
/// The current file and the previous ones that are kept.
pub const MAX_FILES: usize = 3;
/// The longest line written; the rest is cut (a runaway message must not fill the file).
const MAX_LINE_CHARS: usize = 4000;

/// What a process gives to open its log file.
pub struct FileLogConfig {
    /// `desktop`, `daemon` or `overlay`: the file stem and the process column.
    pub process: &'static str,
    /// The folder of the files (`<config dir>/logs`); created when missing.
    pub dir: PathBuf,
    /// The app version, written in the start line.
    pub version: &'static str,
    /// Removes secrets from a message (the job store's `scrub_secrets`); `None` keeps the message.
    pub scrub: Option<fn(&str) -> String>,
}

/// The open log file of one process.
pub struct FileSink {
    process: &'static str,
    dir: PathBuf,
    scrub: Option<fn(&str) -> String>,
    home: Option<String>,
    cap: u64,
    state: Mutex<State>,
}

struct State {
    file: Option<File>,
    size: u64,
}

fn home_dir() -> Option<String> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var(var).ok().filter(|h| h.len() > 1)
}

/// `~` for the home folder inside `s` (also the form with the other path separator).
pub fn redact_home(s: &str, home: Option<&str>) -> String {
    let Some(home) = home else {
        return s.to_owned();
    };
    let mut out = s.replace(home, "~");
    let alt = home.replace('\\', "/");
    if alt != home {
        out = out.replace(&alt, "~");
    }
    out
}

impl FileSink {
    /// Opens (and rotates when full) the log file; `None` when the folder or file cannot be made, in
    /// which case the process just logs to stderr.
    pub fn open(cfg: &FileLogConfig) -> Option<FileSink> {
        Self::open_with(cfg, MAX_FILE_BYTES, home_dir())
    }

    pub(crate) fn open_with(
        cfg: &FileLogConfig,
        cap: u64,
        home: Option<String>,
    ) -> Option<FileSink> {
        fs::create_dir_all(&cfg.dir).ok()?;
        let sink = FileSink {
            process: cfg.process,
            dir: cfg.dir.clone(),
            scrub: cfg.scrub,
            home,
            cap,
            state: Mutex::new(State {
                file: None,
                size: 0,
            }),
        };
        {
            let mut st = sink.state.lock().ok()?;
            let size = fs::metadata(sink.path(0)).map_or(0, |m| m.len());
            if size >= cap {
                sink.rotate(&mut st);
            } else {
                st.size = size;
                st.file = open_append(&sink.path(0));
            }
            st.file.as_ref()?;
        }
        Some(sink)
    }

    fn path(&self, n: usize) -> PathBuf {
        let base = format!("{}.log", self.process);
        if n == 0 {
            self.dir.join(base)
        } else {
            self.dir.join(format!("{base}.{n}"))
        }
    }

    fn rotate(&self, st: &mut State) {
        st.file = None;
        let _ = fs::remove_file(self.path(MAX_FILES - 1));
        for n in (0..MAX_FILES - 1).rev() {
            let _ = fs::rename(self.path(n), self.path(n + 1));
        }
        st.size = 0;
        st.file = open_append(&self.path(0));
    }

    /// Writes one record (`level` is the log level name).
    pub fn write(&self, level: &str, target: &str, message: &str) {
        self.write_at(&timestamp(), level, target, message);
    }

    fn write_at(&self, ts: &str, level: &str, target: &str, message: &str) {
        let mut msg = match self.scrub {
            Some(f) => f(message),
            None => message.to_owned(),
        };
        msg = redact_home(&msg, self.home.as_deref());
        msg = msg.replace(['\r', '\n'], " | ");
        if msg.chars().count() > MAX_LINE_CHARS {
            msg = msg.chars().take(MAX_LINE_CHARS).collect::<String>() + "...";
        }
        let line = format!("{ts} {} {level} {target}: {msg}\n", self.process);
        let Ok(mut st) = self.state.lock() else {
            return;
        };
        if st.size + line.len() as u64 > self.cap && st.size > 0 {
            self.rotate(&mut st);
        }
        if let Some(f) = st.file.as_mut() {
            // A failed write has nowhere to be reported.
            if f.write_all(line.as_bytes()).is_ok() {
                st.size += line.len() as u64;
            }
        }
    }

    /// The marker file of this process.
    fn marker(&self) -> PathBuf {
        self.dir.join(format!("running-{}.pid", self.process))
    }

    /// True when a marker from an earlier run is still there and that run is not alive: it did not end
    /// normally. Writes this run's marker either way.
    pub fn begin_run(&self) -> bool {
        let marker = self.marker();
        let previous = fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        let unclean =
            cfg!(unix) && previous.is_some_and(|pid| pid != std::process::id() && !pid_alive(pid));
        let _ = fs::write(&marker, std::process::id().to_string());
        unclean
    }

    /// Removes the marker (a normal exit).
    pub fn end_run(&self) {
        let _ = fs::remove_file(self.marker());
    }

    /// Records a panic: where, what and a one-line backtrace.
    pub fn write_panic(&self, location: &str, payload: &str, backtrace: &str) {
        self.write(
            "ERROR",
            "panic",
            &format!("panic at {location}: {payload}; backtrace: {backtrace}"),
        );
    }
}

fn open_append(path: &Path) -> Option<File> {
    OpenOptions::new().create(true).append(true).open(path).ok()
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // Signal 0 only checks that the process exists (EPERM means it does, owned by someone else).
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: `kill` with signal 0 sends nothing.
    let r = unsafe { libc::kill(pid, 0) };
    r == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(not(unix))]
fn pid_alive(_pid: u32) -> bool {
    false
}

fn timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "0000-00-00T00:00:00Z".into())
}

/// A backtrace as one line: the frame names, innermost first, cut to `max` characters.
pub fn one_line_backtrace(full: &str, max: usize) -> String {
    let frames: Vec<&str> = full
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("at "))
        .map(|l| l.split_once(": ").map_or(l, |(_, name)| name))
        .collect();
    let mut s = frames.join(" <- ");
    if s.chars().count() > max {
        s = s.chars().take(max).collect::<String>() + "...";
    }
    s
}

/// One parsed log line (see [`recent_lines`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogLine {
    pub time: String,
    pub process: String,
    pub level: String,
    pub text: String,
}

fn level_rank(level: &str) -> u8 {
    match level {
        "ERROR" => 4,
        "WARN" => 3,
        "INFO" => 2,
        "DEBUG" => 1,
        _ => 0,
    }
}

/// The last `max` lines at `min_level` ("WARN" keeps WARN and ERROR) from every log file in `dir`
/// (all processes, oldest file first), in time order. Lines are already redacted when written.
pub fn recent_lines(dir: &Path, min_level: &str, max: usize) -> Vec<LogLine> {
    let mut all = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return all;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if !name.contains(".log") {
            continue;
        }
        let Ok(text) = fs::read_to_string(e.path()) else {
            continue;
        };
        for line in text.lines() {
            let mut it = line.splitn(4, ' ');
            let (Some(time), Some(process), Some(level), Some(rest)) =
                (it.next(), it.next(), it.next(), it.next())
            else {
                continue;
            };
            if level_rank(level) >= level_rank(min_level) && level_rank(level) > 0 {
                all.push(LogLine {
                    time: time.to_owned(),
                    process: process.to_owned(),
                    level: level.to_owned(),
                    text: rest.to_owned(),
                });
            }
        }
    }
    all.sort_by(|a, b| a.time.cmp(&b.time));
    let skip = all.len().saturating_sub(max);
    all.split_off(skip)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(dir: &Path) -> FileLogConfig {
        FileLogConfig {
            process: "desktop",
            dir: dir.to_path_buf(),
            version: "0.0.0",
            scrub: None,
        }
    }

    fn read(dir: &Path, name: &str) -> String {
        fs::read_to_string(dir.join(name)).unwrap_or_default()
    }

    #[test]
    fn a_record_is_one_line_with_time_process_level_and_target() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 1000, None).unwrap();
        sink.write("WARN", "cm_daemon::jobs", "first\nsecond");
        let text = read(d.path(), "desktop.log");
        assert_eq!(text.lines().count(), 1);
        let line = text.lines().next().unwrap();
        assert!(line.contains(" desktop WARN cm_daemon::jobs: first | second"));
        assert!(line.starts_with("20"));
    }

    // DIAG-1.AC1
    #[test]
    fn the_folder_holds_at_most_three_files_of_the_cap() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 1000, None).unwrap();
        for i in 0..200 {
            sink.write(
                "INFO",
                "cm_x",
                &format!("record number {i} {}", "x".repeat(50)),
            );
        }
        let names: Vec<String> = fs::read_dir(d.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("desktop.log"))
            .collect();
        assert_eq!(names.len(), MAX_FILES, "{names:?}");
        for n in &names {
            assert!(fs::metadata(d.path().join(n)).unwrap().len() <= 1000, "{n}");
        }
        // The newest record is in the current file, the oldest are gone.
        assert!(read(d.path(), "desktop.log").contains("record number 199"));
        assert!(!names
            .iter()
            .any(|n| read(d.path(), n).contains("number 0 ")));
    }

    #[test]
    fn a_full_file_is_rotated_at_the_next_start() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("desktop.log"), "y".repeat(1200)).unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 1000, None).unwrap();
        sink.write("INFO", "cm_x", "fresh");
        assert_eq!(read(d.path(), "desktop.log.1").len(), 1200);
        assert!(read(d.path(), "desktop.log").contains("fresh"));
    }

    // DIAG-1.AC2
    #[test]
    fn home_folder_tokens_and_secret_urls_do_not_reach_the_file() {
        let d = tempfile::tempdir().unwrap();
        let mut c = cfg(d.path());
        c.scrub = Some(|s| s.replace("sk-secret-123", "[redacted]"));
        let sink = FileSink::open_with(&c, 10_000, Some("/Users/alex".into())).unwrap();
        sink.write(
            "WARN",
            "cm_x",
            "failed /Users/alex/Movies/CaptureMann/rec_1.cmrec key sk-secret-123",
        );
        let text = read(d.path(), "desktop.log");
        assert!(text.contains("~/Movies/CaptureMann/rec_1.cmrec"), "{text}");
        assert!(!text.contains("alex"), "{text}");
        assert!(!text.contains("sk-secret-123"), "{text}");
    }

    #[test]
    fn a_runaway_message_is_cut() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 100_000, None).unwrap();
        sink.write("WARN", "cm_x", &"z".repeat(50_000));
        assert!(read(d.path(), "desktop.log").len() < MAX_LINE_CHARS + 200);
    }

    // DIAG-1.AC3
    #[test]
    fn a_panic_is_written_with_its_place_and_a_backtrace() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 100_000, None).unwrap();
        sink.write_panic("src/a.rs:3:5", "boom", "f <- g");
        let text = read(d.path(), "desktop.log");
        assert!(text.contains("ERROR panic: panic at src/a.rs:3:5: boom; backtrace: f <- g"));
    }

    #[test]
    fn a_backtrace_becomes_one_line_of_frame_names() {
        let full = "   0: cm_core::a\n             at /x/a.rs:1\n   1: cm_core::b\n";
        assert_eq!(one_line_backtrace(full, 100), "cm_core::a <- cm_core::b");
        assert!(one_line_backtrace(full, 5).ends_with("..."));
    }

    // DIAG-1.AC4
    #[test]
    fn a_marker_left_by_a_dead_run_is_reported_once_and_a_clean_exit_leaves_none() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 1000, None).unwrap();
        assert!(!sink.begin_run(), "no marker, no report");
        assert!(sink.marker().exists());
        sink.end_run();
        assert!(!sink.marker().exists());

        // A marker with the pid of a process that is gone (a crash or a force quit).
        let dead = {
            let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
                .spawn()
                .unwrap();
            let pid = child.id();
            child.wait().unwrap();
            pid
        };
        fs::write(sink.marker(), dead.to_string()).unwrap();
        #[cfg(unix)]
        {
            assert!(sink.begin_run(), "the previous run did not end normally");
            assert!(!sink.begin_run(), "reported once: the marker is now ours");
        }
    }

    #[test]
    fn recent_lines_filters_by_level_and_keeps_time_order_across_files() {
        let d = tempfile::tempdir().unwrap();
        let sink = FileSink::open_with(&cfg(d.path()), 10_000, None).unwrap();
        sink.write_at("2026-10-29T10:00:00Z", "INFO", "cm_x", "quiet");
        sink.write_at("2026-10-29T10:00:02Z", "ERROR", "cm_x", "bad");
        sink.write_at("2026-10-29T10:00:01Z", "WARN", "cm_x", "careful");
        let lines = recent_lines(d.path(), "WARN", 10);
        let texts: Vec<_> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["cm_x: careful", "cm_x: bad"]);
        assert_eq!(recent_lines(d.path(), "WARN", 1).len(), 1);
        assert!(recent_lines(&d.path().join("none"), "WARN", 5).is_empty());
    }
}
