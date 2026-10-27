//! One CaptureMann per user (R4-10). Two instances would register the same global shortcuts (the
//! second silently fails), show two tray icons, run two job workers and two crash recoveries on the
//! same library, and fight over the IPC socket. The first instance holds an exclusive lock on
//! `instance.lock` in the config directory for its whole life; a second launch finds the lock taken,
//! asks the first to show itself (`open` on the bundle sends the reopen event the app already
//! handles) and exits. The lock dies with the process, so a crash never leaves it behind.

use std::fs::{File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

/// The lock file's name inside the config directory.
pub const LOCK_FILE: &str = "instance.lock";

/// Held for as long as this process is the running instance.
#[derive(Debug)]
pub struct InstanceLock {
    _file: File,
}

/// What [`acquire`] found.
#[derive(Debug)]
pub enum Acquired {
    /// This process is the instance.
    First(InstanceLock),
    /// Another instance holds the lock.
    AlreadyRunning,
    /// The lock could not be used (read-only config directory, no lock support). The app starts
    /// anyway: a broken lock file must not keep the app from opening.
    Unavailable(io::Error),
}

/// Takes the instance lock in `dir`.
pub fn acquire(dir: &Path) -> Acquired {
    let path: PathBuf = dir.join(LOCK_FILE);
    let open = || -> io::Result<File> {
        std::fs::create_dir_all(dir)?;
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
    };
    let file = match open() {
        Ok(f) => f,
        Err(e) => return Acquired::Unavailable(e),
    };
    match file.try_lock() {
        Ok(()) => {
            // Informational only (`cat instance.lock`); the lock is the source of truth.
            let _ =
                io::Write::write_all(&mut &file, format!("{}\n", std::process::id()).as_bytes());
            Acquired::First(InstanceLock { _file: file })
        }
        Err(TryLockError::WouldBlock) => Acquired::AlreadyRunning,
        Err(TryLockError::Error(e)) => Acquired::Unavailable(e),
    }
}

/// The pid the running instance wrote into `instance.lock` (informational: the lock itself is the
/// source of truth, so a missing or stale number only means "unknown").
pub fn running_pid(dir: &Path) -> Option<u32> {
    std::fs::read_to_string(dir.join(LOCK_FILE))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// What a second launch does about the instance that holds the lock.
#[derive(Debug, PartialEq, Eq)]
pub enum SecondLaunch {
    /// The running instance is this same app: ask it to show itself (`open` on the bundle sends
    /// the reopen event the app already handles).
    ShowIt,
    /// Another copy (a release next to a development build) holds the lock and the library:
    /// bring it to the front and say why this one is not starting.
    OtherCopy { bundle: PathBuf },
    /// The running instance could not be identified. Opening our own bundle could start a third
    /// process that finds the same lock, so do nothing.
    Unknown,
}

/// Decides [`SecondLaunch`] from the bundle this process runs from and the running instance's.
pub fn second_launch(own: Option<&Path>, running: Option<&Path>) -> SecondLaunch {
    match (own, running) {
        (Some(o), Some(r)) if o == r => SecondLaunch::ShowIt,
        (_, Some(r)) => SecondLaunch::OtherCopy {
            bundle: r.to_path_buf(),
        },
        _ => SecondLaunch::Unknown,
    }
}

/// The shell script that relaunches `bundle` once the process `pid` is gone: waits for the old
/// instance to exit (it may be saving a recording for up to 10 s) so the two never overlap, then
/// opens a fresh copy. The bundle path is `$0`, never interpolated.
pub fn relaunch_script(pid: u32) -> String {
    format!("while kill -0 {pid} 2>/dev/null; do sleep 0.2; done; open -n \"$0\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// R4-10: the second launch finds the first, and the lock is free again once it is gone.
    #[test]
    fn r4_10_a_second_instance_is_refused_until_the_first_exits() {
        let dir = tempfile::tempdir().unwrap();
        let first = acquire(dir.path());
        assert!(matches!(first, Acquired::First(_)));
        assert!(matches!(acquire(dir.path()), Acquired::AlreadyRunning));
        assert!(matches!(acquire(dir.path()), Acquired::AlreadyRunning));
        drop(first);
        assert!(matches!(acquire(dir.path()), Acquired::First(_)));
    }

    /// Another config directory (a test instance, the demo flags) is another instance.
    #[test]
    fn separate_config_dirs_do_not_collide() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let _x = acquire(a.path());
        assert!(matches!(acquire(b.path()), Acquired::First(_)));
    }

    #[test]
    fn an_unusable_directory_does_not_block_startup() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f");
        std::fs::write(&file, "x").unwrap();
        // A path under a regular file cannot be created.
        assert!(matches!(
            acquire(&file.join("sub")),
            Acquired::Unavailable(_)
        ));
    }

    /// A second launch shows the same app, or names the other copy that holds the lock; an
    /// unidentified holder is left alone (no `open` that could chain more launches).
    #[test]
    fn a_second_launch_tells_the_same_app_from_another_copy() {
        let rel = Path::new("/Applications/CaptureMann.app");
        let dev = Path::new("/Users/x/Applications/CaptureMann-dev.app");
        assert_eq!(second_launch(Some(rel), Some(rel)), SecondLaunch::ShowIt);
        assert_eq!(
            second_launch(Some(dev), Some(rel)),
            SecondLaunch::OtherCopy {
                bundle: rel.to_path_buf()
            }
        );
        assert_eq!(
            second_launch(None, Some(rel)),
            SecondLaunch::OtherCopy {
                bundle: rel.to_path_buf()
            }
        );
        assert_eq!(second_launch(Some(rel), None), SecondLaunch::Unknown);
    }

    #[test]
    fn the_running_pid_is_read_from_the_lock_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(running_pid(dir.path()), None);
        let _first = acquire(dir.path());
        assert_eq!(running_pid(dir.path()), Some(std::process::id()));
    }

    /// R4-10: the restart helper waits for the old pid before opening the new copy.
    #[test]
    fn the_relaunch_script_waits_for_the_old_process() {
        let s = relaunch_script(4242);
        assert!(s.starts_with("while kill -0 4242"));
        assert!(s.find("kill -0").unwrap() < s.find("open -n").unwrap());
        assert!(s.contains("\"$0\""));
    }
}
