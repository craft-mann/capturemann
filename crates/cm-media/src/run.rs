//! Runs one FFmpeg process: `-progress pipe:1` parsing, cancellation, a stderr tail.
//!
//! Progress comes from the machine-readable key=value blocks FFmpeg writes to stdout
//! (`out_time_us=...`, `speed=1.5x`, `progress=continue|end`). Cancellation polls a caller closure
//! about every 50 ms and kills the child (EXP-4.AC4: the caller then deletes the partial output).

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Stdio;
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cm_core::CmError;

use crate::tools::spawn_error;

/// How many stderr lines are kept for the error reason.
pub const STDERR_TAIL_LINES: usize = 12;
/// Longest stderr line kept, in bytes: a filter-graph dump can be one enormous line.
pub const STDERR_LINE_MAX_BYTES: usize = 512;

/// `line` cut to [`STDERR_LINE_MAX_BYTES`] on a char boundary, with an ellipsis when cut.
pub fn cap_line(line: &str) -> String {
    if line.len() <= STDERR_LINE_MAX_BYTES {
        return line.to_owned();
    }
    let mut end = STDERR_LINE_MAX_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &line[..end])
}

/// Maps an I/O error to a shared code: a full disk (`ENOSPC` 28, or Windows `ERROR_DISK_FULL` 112
/// and `ERROR_HANDLE_DISK_FULL` 39, or `StorageFull`) is `disk_full`; the rest is `internal`.
pub fn map_io_error(what: &str, e: &std::io::Error) -> CmError {
    let full = e.kind() == std::io::ErrorKind::StorageFull
        || match e.raw_os_error() {
            Some(28) => cfg!(unix),
            Some(112 | 39) => cfg!(windows),
            _ => false,
        };
    if full {
        return CmError::disk_full("The disk is full, so the video could not be written.");
    }
    CmError::internal(format!("{what}: {e}"))
}

/// One progress update.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// 0..=1 of the output duration (1.0 only when FFmpeg says `progress=end`).
    pub fraction: f64,
    /// Encoded media seconds so far.
    pub out_time_s: f64,
    /// Encoding speed as a multiple of real time, when reported.
    pub speed: Option<f64>,
}

/// Turns `-progress` lines into [`Progress`].
#[derive(Debug, Clone)]
pub struct ProgressParser {
    total_s: f64,
    out_time_s: f64,
    speed: Option<f64>,
}

impl ProgressParser {
    pub fn new(total_s: f64) -> Self {
        ProgressParser {
            total_s: total_s.max(0.001),
            out_time_s: 0.0,
            speed: None,
        }
    }

    /// Feeds one line; returns an update when a block ends (`progress=...`).
    pub fn feed(&mut self, line: &str) -> Option<Progress> {
        let (k, v) = line.trim().split_once('=')?;
        match k {
            "out_time_us" | "out_time_ms" => {
                // Despite its name, `out_time_ms` is also in microseconds.
                if let Ok(us) = v.trim().parse::<i64>() {
                    self.out_time_s = us.max(0) as f64 / 1e6;
                }
                None
            }
            "speed" => {
                self.speed = v.trim().trim_end_matches('x').trim().parse::<f64>().ok();
                None
            }
            "progress" => {
                let done = v.trim() == "end";
                let fraction = if done {
                    1.0
                } else {
                    (self.out_time_s / self.total_s).clamp(0.0, 0.99)
                };
                Some(Progress {
                    fraction,
                    out_time_s: self.out_time_s,
                    speed: self.speed,
                })
            }
            _ => None,
        }
    }
}

/// How long the end of a run waits for its helper threads (the stdout and stderr readers and the
/// feeder) once FFmpeg has exited or been killed. A reader only ends when every holder of the pipe
/// has closed it; on Windows another child started at the same moment can hold a copy (see
/// `cm_platform::process::spawn`), and the feeder may sit in a slow GPU step. The run must not
/// wait for them: a cancel (EXP-4.AC4) is prompt and bounded on every system. A thread that is
/// still busy after this is left to end by itself (its pipe or channel is closed, so it does).
pub const WIND_DOWN_WAIT: Duration = Duration::from_secs(3);

/// Joins `handle` when it ends within `wait`; `None` (the thread is detached) otherwise.
pub fn join_within<T>(handle: JoinHandle<T>, wait: Duration) -> Option<T> {
    let end = Instant::now() + wait;
    while !handle.is_finished() {
        if Instant::now() >= end {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    handle.join().ok()
}

/// Marker text of a feeder error that only says FFmpeg stopped reading.
pub const BROKEN_PIPE: &str = "FFmpeg closed its input";

fn is_broken_pipe(e: &CmError) -> bool {
    e.to_string().contains(BROKEN_PIPE)
}

/// Why a run did not complete.
#[derive(Debug)]
pub enum RunError {
    /// The cancel closure returned true; the child was killed.
    Cancelled,
    /// FFmpeg exited with an error.
    Failed(CmError),
}

/// Maps FFmpeg's stderr tail to an error: a full disk is `disk_full`, anything else `internal`.
pub fn failure_error(status: &str, tail: &[String]) -> CmError {
    let text = tail.join("\n");
    let lower = text.to_lowercase();
    if lower.contains("no space left on device") || lower.contains("disk full") {
        return CmError::disk_full("The disk is full, so the video could not be written.");
    }
    let detail = if text.trim().is_empty() {
        "no error output".to_owned()
    } else {
        text
    };
    CmError::internal(format!("FFmpeg failed ({status}): {detail}"))
}

/// Writes the frames FFmpeg reads from `pipe:0`. Returning drops the pipe, which ends the input.
pub type Feeder = Box<dyn FnOnce(std::process::ChildStdin) -> Result<(), CmError> + Send>;

/// Runs `ffmpeg` with `args`. `total_s` is the expected output length for the progress fraction.
pub fn run_ffmpeg(
    ffmpeg: &Path,
    args: &[String],
    total_s: f64,
    on_progress: &mut dyn FnMut(Progress),
    cancel: &dyn Fn() -> bool,
) -> Result<(), RunError> {
    run_ffmpeg_fed(ffmpeg, args, total_s, on_progress, cancel, None)
}

/// [`run_ffmpeg`] with a `feeder` thread writing to FFmpeg's stdin (`-i pipe:0`). When the
/// feeder fails, FFmpeg is killed and the feeder's error is the result, unless FFmpeg itself
/// failed first (a broken pipe then only means it stopped reading: its stderr is the reason).
pub fn run_ffmpeg_fed(
    ffmpeg: &Path,
    args: &[String],
    total_s: f64,
    on_progress: &mut dyn FnMut(Progress),
    cancel: &dyn Fn() -> bool,
    feeder: Option<Feeder>,
) -> Result<(), RunError> {
    let feed_error: Arc<Mutex<Option<CmError>>> = Arc::new(Mutex::new(None));
    let mut cmd = cm_platform::process::command(ffmpeg);
    cmd.args(args)
        .stdin(if feeder.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cm_platform::process::spawn(&mut cmd)
        .map_err(|e| RunError::Failed(spawn_error(ffmpeg, &e)))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let feed_thread = match (feeder, child.stdin.take()) {
        (Some(f), Some(stdin)) => {
            let slot = feed_error.clone();
            Some(std::thread::spawn(move || {
                if let Err(e) = f(stdin) {
                    *slot.lock().unwrap_or_else(|p| p.into_inner()) = Some(e);
                }
            }))
        }
        _ => None,
    };
    let (tx, rx) = mpsc::channel::<String>();
    let out_thread = std::thread::spawn(move || {
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        }
    });
    let err_thread = std::thread::spawn(move || {
        let mut tail: VecDeque<String> = VecDeque::new();
        if let Some(err) = stderr {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                if line.trim().is_empty() {
                    continue;
                }
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(cap_line(&line));
            }
        }
        tail.into_iter().collect::<Vec<_>>()
    });

    let mut parser = ProgressParser::new(total_s);
    let mut cancelled = false;
    let status = loop {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(line) => {
                if let Some(p) = parser.feed(&line) {
                    on_progress(p);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // stdout closed: the process is exiting.
                break child.wait();
            }
        }
        if cancel() {
            cancelled = true;
            let _ = child.kill();
            break child.wait();
        }
        let failed = feed_error
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .is_some_and(|e| !is_broken_pipe(e));
        if failed {
            // The feeder (decode, compose) failed: stop FFmpeg, its error is the reason.
            let _ = child.kill();
            break child.wait();
        }
    };
    // Bounded: see WIND_DOWN_WAIT. The feeder is waited for first when FFmpeg failed on its own
    // (its error may be the reason); after a cancel nobody needs what it has to say.
    let _ = join_within(out_thread, WIND_DOWN_WAIT);
    let tail = join_within(err_thread, WIND_DOWN_WAIT).unwrap_or_default();
    if let Some(t) = feed_thread {
        let _ = join_within(t, WIND_DOWN_WAIT);
    }
    if cancelled {
        return Err(RunError::Cancelled);
    }
    let fed_err = feed_error.lock().unwrap_or_else(|p| p.into_inner()).take();
    if let Some(e) = fed_err.filter(|e| !is_broken_pipe(e)) {
        return Err(RunError::Failed(e));
    }
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(RunError::Failed(failure_error(&s.to_string(), &tail))),
        Err(e) => Err(RunError::Failed(CmError::internal(format!(
            "waiting for FFmpeg failed: {e}"
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_blocks_become_fractions() {
        let mut p = ProgressParser::new(10.0);
        let mut got = Vec::new();
        for l in [
            "frame=30",
            "fps=60.0",
            "out_time_us=2500000",
            "out_time_ms=2500000",
            "speed=1.8x",
            "progress=continue",
            "out_time_us=9990000",
            "speed=N/A",
            "progress=continue",
            "out_time_us=10000000",
            "progress=end",
        ] {
            if let Some(x) = p.feed(l) {
                got.push(x);
            }
        }
        assert_eq!(got.len(), 3);
        assert!((got[0].fraction - 0.25).abs() < 1e-9);
        assert_eq!(got[0].speed, Some(1.8));
        assert!(got[1].fraction <= 0.99);
        assert_eq!(got[1].speed, None);
        assert_eq!(got[2].fraction, 1.0);
        // Garbage and negative times (FFmpeg prints a huge negative before the first frame).
        assert!(p.feed("garbage").is_none());
        p.feed("out_time_us=-9223372036854775807");
        assert_eq!(p.feed("progress=continue").unwrap().fraction, 0.0);
    }

    #[test]
    fn failures_map_to_codes() {
        let e = failure_error("exit status: 1", &["Error: No space left on device".into()]);
        assert_eq!(e.code(), cm_core::ErrorCode::DiskFull);
        let e = failure_error(
            "exit status: 1",
            &["Invalid argument".into(), "boom".into()],
        );
        assert_eq!(e.code(), cm_core::ErrorCode::Internal);
        assert!(e.to_string().contains("boom") && e.to_string().contains("exit status: 1"));
        assert!(failure_error("x", &[])
            .to_string()
            .contains("no error output"));
    }

    #[cfg(unix)]
    #[test]
    fn cancellation_kills_the_child() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        let start = std::time::Instant::now();
        // `sleep` ignores its arguments being FFmpeg's: it just blocks for 30 s.
        let r = run_ffmpeg(
            Path::new("/bin/sleep"),
            &["30".to_owned()],
            1.0,
            &mut |_| {},
            &|| {
                if start.elapsed() > Duration::from_millis(200) {
                    flag.store(true, Ordering::SeqCst);
                }
                flag.load(Ordering::SeqCst)
            },
        );
        assert!(matches!(r, Err(RunError::Cancelled)));
        assert!(start.elapsed() < Duration::from_secs(5));
    }

    /// A grandchild that keeps FFmpeg's stdout and stderr open (on Windows: a child started at
    /// the same moment that inherited the pipe ends) must not hold a cancel back: the readers
    /// only end when the last holder exits, so the run stops waiting for them after a bound.
    #[cfg(unix)]
    #[test]
    fn a_cancel_does_not_wait_for_a_holder_of_the_output_pipes() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        let start = std::time::Instant::now();
        // The shell starts a background sleeper that inherits both pipes, then waits for it.
        let r = run_ffmpeg(
            Path::new("/bin/sh"),
            &["-c".to_owned(), "sleep 12 & wait".to_owned()],
            1.0,
            &mut |_| {},
            &|| {
                if start.elapsed() > Duration::from_millis(200) {
                    flag.store(true, Ordering::SeqCst);
                }
                flag.load(Ordering::SeqCst)
            },
        );
        assert!(matches!(r, Err(RunError::Cancelled)));
        assert!(
            start.elapsed() < WIND_DOWN_WAIT + Duration::from_secs(4),
            "took {:?}",
            start.elapsed()
        );
    }

    /// A feeder stuck in a slow step (a GPU frame, a decoder that does not deliver) must not hold
    /// a cancel back either.
    #[cfg(unix)]
    #[test]
    fn a_cancel_does_not_wait_for_a_stuck_feeder() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        let start = std::time::Instant::now();
        let feeder: Feeder = Box::new(|_stdin| {
            std::thread::sleep(Duration::from_secs(12));
            Ok(())
        });
        let r = run_ffmpeg_fed(
            Path::new("/bin/sleep"),
            &["30".to_owned()],
            1.0,
            &mut |_| {},
            &|| {
                if start.elapsed() > Duration::from_millis(200) {
                    flag.store(true, Ordering::SeqCst);
                }
                flag.load(Ordering::SeqCst)
            },
            Some(feeder),
        );
        assert!(matches!(r, Err(RunError::Cancelled)));
        assert!(
            start.elapsed() < WIND_DOWN_WAIT + Duration::from_secs(4),
            "took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn join_within_returns_a_result_or_gives_up() {
        let quick = std::thread::spawn(|| 7);
        assert_eq!(join_within(quick, Duration::from_secs(2)), Some(7));
        let slow = std::thread::spawn(|| std::thread::sleep(Duration::from_millis(600)));
        let t = Instant::now();
        assert!(join_within(slow, Duration::from_millis(50)).is_none());
        assert!(t.elapsed() < Duration::from_millis(500));
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_child_reports_its_stderr_tail() {
        let r = run_ffmpeg(
            Path::new("/bin/sh"),
            &["-c".into(), "echo bad thing >&2; exit 3".into()],
            1.0,
            &mut |_| {},
            &|| false,
        );
        match r {
            Err(RunError::Failed(e)) => {
                assert_eq!(e.code(), cm_core::ErrorCode::Internal);
                assert!(e.to_string().contains("bad thing"), "{e}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_missing_binary_is_not_found() {
        let r = run_ffmpeg(
            Path::new("/nonexistent/ffmpeg"),
            &[],
            1.0,
            &mut |_| {},
            &|| false,
        );
        match r {
            Err(RunError::Failed(e)) => {
                assert_eq!(e.code(), cm_core::ErrorCode::NotFound);
                assert!(crate::tools::is_not_found_message(&e.to_string()));
            }
            other => panic!("{other:?}"),
        }
    }
}

#[cfg(test)]
mod cap_tests {
    use super::*;

    #[test]
    fn r5_stderr_lines_are_capped_on_a_char_boundary() {
        assert_eq!(cap_line("short"), "short");
        let long = "x".repeat(5000);
        let c = cap_line(&long);
        assert!(c.len() <= STDERR_LINE_MAX_BYTES + 3 && c.ends_with("..."));
        let cjk = "字".repeat(1000);
        let c = cap_line(&cjk);
        assert!(c.len() <= STDERR_LINE_MAX_BYTES + 3);
        assert!(c.trim_end_matches("...").chars().all(|ch| ch == '字'));
    }

    /// A full disk is `disk_full` whatever wrote the file (R5: ENOSPC mapped everywhere).
    #[test]
    fn r5_enospc_maps_to_disk_full() {
        let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
        assert_eq!(
            map_io_error("x", &full).code(),
            cm_core::ErrorCode::DiskFull
        );
        #[cfg(unix)]
        {
            let e = std::io::Error::from_raw_os_error(28);
            assert_eq!(map_io_error("x", &e).code(), cm_core::ErrorCode::DiskFull);
        }
        let other = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        assert_eq!(
            map_io_error("x", &other).code(),
            cm_core::ErrorCode::Internal
        );
    }
}
