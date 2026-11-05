//! Running child programs with a time limit, capturing their output, and never leaving them behind.

use crate::sys;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Process ids the current case started; the runner kills what is left after the case.
pub type Tracker = Arc<Mutex<Vec<u32>>>;

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub elapsed_ms: u64,
}

impl Output {
    pub fn success(&self) -> bool {
        !self.timed_out && self.code == Some(0)
    }
    /// Both streams, for messages.
    pub fn text(&self) -> String {
        format!(
            "{}{}{}",
            self.stdout,
            if self.stdout.is_empty() { "" } else { "\n" },
            self.stderr
        )
    }
}

/// Whether an exit code is a crash rather than an ordinary failure: a Windows NTSTATUS error such as
/// 0xC0000005 (access violation), 0xC000001D (illegal instruction) or 0xC0000409 (fast fail), or a
/// signal on Unix.
pub fn is_crash_code(code: Option<i32>) -> bool {
    match code {
        None => true,
        Some(c) => {
            let u = c as u32;
            (0xC000_0000..=0xC000_FFFF).contains(&u) || (c < 0 && c > -200) || u == 0x8000_0003
        }
    }
}

pub fn exit_text(code: Option<i32>) -> String {
    match code {
        None => "killed by a signal".to_owned(),
        Some(c) => format!("exit {c} ({:#010x})", c as u32),
    }
}

fn reader(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Runs a command to the end or to the time limit (then kills it and its children). Standard input is
/// closed. The pid is in `tracker` while it runs.
pub fn run(mut cmd: Command, timeout: Duration, tracker: &Tracker) -> std::io::Result<Output> {
    sys::hide_console(&mut cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let start = Instant::now();
    let mut child = cmd.spawn()?;
    let pid = child.id();
    if let Ok(mut t) = tracker.lock() {
        t.push(pid);
    }
    let out = child.stdout.take().map(reader);
    let err = child.stderr.take().map(reader);
    let mut timed_out = false;
    let code = loop {
        if let Some(status) = child.try_wait()? {
            break status.code();
        }
        if start.elapsed() >= timeout {
            timed_out = true;
            sys::kill_tree(pid);
            let _ = child.kill();
            break child.wait().ok().and_then(|s| s.code());
        }
        std::thread::sleep(Duration::from_millis(15));
    };
    if let Ok(mut t) = tracker.lock() {
        t.retain(|p| *p != pid);
    }
    Ok(Output {
        code,
        stdout: out.and_then(|h| h.join().ok()).unwrap_or_default(),
        stderr: err.and_then(|h| h.join().ok()).unwrap_or_default(),
        timed_out,
        elapsed_ms: start.elapsed().as_millis() as u64,
    })
}

/// Waits until `f` returns Some or the time is up; polls every `step`.
pub fn wait_for<T>(
    timeout: Duration,
    step: Duration,
    mut f: impl FnMut() -> Option<T>,
) -> Option<T> {
    let start = Instant::now();
    loop {
        if let Some(v) = f() {
            return Some(v);
        }
        if start.elapsed() >= timeout {
            return None;
        }
        std::thread::sleep(step);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crash_codes() {
        assert!(is_crash_code(Some(0xC000_001Du32 as i32)));
        assert!(is_crash_code(Some(0xC000_0409u32 as i32)));
        assert!(is_crash_code(Some(0xC000_0005u32 as i32)));
        assert!(is_crash_code(None));
        assert!(!is_crash_code(Some(0)));
        assert!(!is_crash_code(Some(2)));
        assert!(!is_crash_code(Some(1)));
    }

    #[cfg(unix)]
    #[test]
    fn runs_captures_and_times_out() {
        let t: Tracker = Arc::default();
        let mut c = Command::new("sh");
        c.args(["-c", "echo out; echo err >&2; exit 3"]);
        let o = run(c, Duration::from_secs(10), &t).unwrap();
        assert_eq!(o.code, Some(3));
        assert_eq!(o.stdout.trim(), "out");
        assert_eq!(o.stderr.trim(), "err");
        assert!(!o.timed_out);
        let mut c = Command::new("sleep");
        c.arg("30");
        let o = run(c, Duration::from_millis(200), &t).unwrap();
        assert!(o.timed_out);
        assert!(t.lock().unwrap().is_empty());
    }
}
