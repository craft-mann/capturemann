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

    /// R4-10: the restart helper waits for the old pid before opening the new copy.
    #[test]
    fn the_relaunch_script_waits_for_the_old_process() {
        let s = relaunch_script(4242);
        assert!(s.starts_with("while kill -0 4242"));
        assert!(s.find("kill -0").unwrap() < s.find("open -n").unwrap());
        assert!(s.contains("\"$0\""));
    }
}
