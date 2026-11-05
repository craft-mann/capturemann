//! Making sure the overlay cannot outlive what it is for.
//!
//! The overlay is a short-lived child of the desktop app (one pick, then it exits). The app kills
//! it after its own watchdog and when the app's future is dropped (`kill_on_drop`), but neither
//! helps when the app itself is killed or crashes, and nothing in the overlay itself ended a run
//! that waits for a daemon that never answers. On Windows a child is not ended with its parent,
//! so such an overlay stayed in the task list (a stray `cm-overlay.exe`, 0.2.6). Two guards, both
//! logged to `overlay.log` before the process exits:
//!
//! * a **deadline** ([`DEFAULT_DEADLINE`], `CM_OVERLAY_DEADLINE_S` to change it): longer than the
//!   app's watchdog, so in a normal run it never fires;
//! * the **parent**: the app passes its process id in [`PARENT_PID_ENV`]; on Windows the overlay
//!   waits for that process to end and then exits (elsewhere the deadline is the only guard).

use std::time::Duration;

/// The longest an overlay lives. The desktop app's watchdog is 120 s.
pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(150);

/// Seconds, overrides [`DEFAULT_DEADLINE`] (0 or unparsable: the default).
pub const DEADLINE_ENV: &str = "CM_OVERLAY_DEADLINE_S";

/// The process id of the app that started the overlay.
pub const PARENT_PID_ENV: &str = "CAPTUREMANN_PARENT_PID";

/// Why a guard ended the process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Deadline(Duration),
    ParentGone(u32),
}

impl Reason {
    pub fn describe(self) -> String {
        match self {
            Reason::Deadline(d) => format!(
                "the overlay was still running after {} s and ends itself",
                d.as_secs()
            ),
            Reason::ParentGone(pid) => {
                format!("the app that started the overlay (process {pid}) is gone; the overlay ends itself")
            }
        }
    }
}

/// The deadline from the environment value.
pub fn deadline_from(value: Option<&str>) -> Duration {
    value
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .map_or(DEFAULT_DEADLINE, Duration::from_secs)
}

/// The parent process id from the environment value.
pub fn parent_pid_from(value: Option<&str>) -> Option<u32> {
    value
        .and_then(|v| v.trim().parse::<u32>().ok())
        .filter(|p| *p > 0)
}

/// Starts the guards. `on_expire` runs on a helper thread, once, when a guard fires; the binary's
/// one ends the process.
pub fn guard(
    deadline: Duration,
    parent: Option<u32>,
    on_expire: impl Fn(Reason) + Send + Sync + 'static,
) {
    let on_expire = std::sync::Arc::new(on_expire);
    {
        let on_expire = on_expire.clone();
        std::thread::Builder::new()
            .name("overlay-deadline".into())
            .spawn(move || {
                std::thread::sleep(deadline);
                on_expire(Reason::Deadline(deadline));
            })
            .ok();
    }
    if let Some(pid) = parent {
        std::thread::Builder::new()
            .name("overlay-parent".into())
            .spawn(move || {
                if crate::platform::wait_for_process_exit(pid) {
                    on_expire(Reason::ParentGone(pid));
                }
            })
            .ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn the_deadline_comes_from_the_environment_or_the_default() {
        assert_eq!(deadline_from(None), DEFAULT_DEADLINE);
        assert_eq!(deadline_from(Some("30")), Duration::from_secs(30));
        assert_eq!(deadline_from(Some(" 7 ")), Duration::from_secs(7));
        assert_eq!(deadline_from(Some("0")), DEFAULT_DEADLINE);
        assert_eq!(deadline_from(Some("soon")), DEFAULT_DEADLINE);
    }

    #[test]
    fn the_parent_pid_must_be_a_positive_number() {
        assert_eq!(parent_pid_from(Some("1234")), Some(1234));
        assert_eq!(parent_pid_from(Some("0")), None);
        assert_eq!(parent_pid_from(Some("x")), None);
        assert_eq!(parent_pid_from(None), None);
    }

    /// The overlay that waits for a daemon that never answers ends itself.
    #[test]
    fn the_deadline_fires_and_says_why() {
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        guard(Duration::from_millis(20), None, move |r| {
            let _ = tx.lock().unwrap().send(r);
        });
        let r = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(r, Reason::Deadline(Duration::from_millis(20)));
        assert!(r.describe().contains("ends itself"));
    }

    /// A parent that ends is noticed (a real child process, so this runs on Unix and Windows; on
    /// Unix there is no parent watch and the test only checks that the helper does not fire).
    #[cfg(windows)]
    #[test]
    fn a_parent_that_ends_is_noticed() {
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "ping -n 2 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let pid = child.id();
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        guard(Duration::from_secs(60), Some(pid), move |r| {
            let _ = tx.lock().unwrap().send(r);
        });
        child.wait().unwrap();
        assert_eq!(
            rx.recv_timeout(Duration::from_secs(10)).unwrap(),
            Reason::ParentGone(pid)
        );
    }
}
