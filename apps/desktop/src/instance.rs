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
/// The running instance's pid. Not the lock file: on Windows a held lock is mandatory, so no other
/// handle can read the locked file.
pub const PID_FILE: &str = "instance.pid";

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
            // Informational only; the lock is the source of truth.
            let _ = std::fs::write(dir.join(PID_FILE), format!("{}\n", std::process::id()));
            Acquired::First(InstanceLock { _file: file })
        }
        Err(TryLockError::WouldBlock) => Acquired::AlreadyRunning,
        Err(TryLockError::Error(e)) => Acquired::Unavailable(e),
    }
}

/// The pid the running instance wrote into `instance.pid` (informational: the lock itself is the
/// source of truth, so a missing or stale number only means "unknown").
pub fn running_pid(dir: &Path) -> Option<u32> {
    std::fs::read_to_string(dir.join(PID_FILE))
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

/// What a second launch asks the running instance to do. On Linux a desktop that cannot grab
/// global keys (Wayland without the GlobalShortcuts portal) binds its own keyboard shortcut to
/// `cm-desktop --launcher` or `cm-desktop --capture region`, and these reach the app that runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// Bring the app forward (a plain second launch).
    Show,
    /// Toggle the command launcher.
    Launcher,
    Library,
    Settings,
    Capture(CaptureKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureKind {
    Region,
    Window,
    Display,
}

impl Request {
    /// The request the command line of a launch makes: `--launcher`, `--library`, `--settings`,
    /// `--capture region|window|display`, or `Show` when none is given.
    pub fn from_args(args: &[String]) -> Request {
        let after = |f: &str| {
            args.iter()
                .position(|a| a == f)
                .and_then(|i| args.get(i + 1))
                .map(String::as_str)
        };
        if let Some(k) = after("--capture").and_then(CaptureKind::parse) {
            return Request::Capture(k);
        }
        if args.iter().any(|a| a == "--launcher") {
            Request::Launcher
        } else if args.iter().any(|a| a == "--library") {
            Request::Library
        } else if args.iter().any(|a| a == "--settings") {
            Request::Settings
        } else {
            Request::Show
        }
    }

    /// The line sent over the socket.
    pub fn to_line(self) -> &'static str {
        match self {
            Request::Show => "show",
            Request::Launcher => "launcher",
            Request::Library => "library",
            Request::Settings => "settings",
            Request::Capture(CaptureKind::Region) => "capture region",
            Request::Capture(CaptureKind::Window) => "capture window",
            Request::Capture(CaptureKind::Display) => "capture display",
        }
    }

    pub fn from_line(line: &str) -> Option<Request> {
        let line = line.trim();
        match line {
            "show" => Some(Request::Show),
            "launcher" => Some(Request::Launcher),
            "library" => Some(Request::Library),
            "settings" => Some(Request::Settings),
            _ => line
                .strip_prefix("capture ")
                .and_then(CaptureKind::parse)
                .map(Request::Capture),
        }
    }
}

impl CaptureKind {
    fn parse(s: &str) -> Option<CaptureKind> {
        match s {
            "region" => Some(CaptureKind::Region),
            "window" => Some(CaptureKind::Window),
            "display" => Some(CaptureKind::Display),
            _ => None,
        }
    }
}

/// The running instance's request socket, in the config directory next to the lock.
pub const SOCKET_FILE: &str = "instance.sock";

/// Listens for [`Request`]s from later launches on a thread. The caller holds the instance lock,
/// so any socket file left behind is stale and is replaced. The socket is owner-only.
#[cfg(unix)]
pub fn serve(dir: &Path, on_request: impl Fn(Request) + Send + 'static) -> io::Result<()> {
    use std::io::{BufRead, BufReader, Read};
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    let path = dir.join(SOCKET_FILE);
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::Builder::new()
        .name("instance-requests".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                let mut line = String::new();
                if BufReader::new(&stream)
                    .take(64)
                    .read_line(&mut line)
                    .is_ok()
                {
                    if let Some(req) = Request::from_line(&line) {
                        on_request(req);
                    }
                }
            }
        })?;
    Ok(())
}

/// Sends `request` to the running instance.
#[cfg(unix)]
pub fn send(dir: &Path, request: Request) -> io::Result<()> {
    use std::io::Write;
    let mut stream = std::os::unix::net::UnixStream::connect(dir.join(SOCKET_FILE))?;
    stream.set_write_timeout(Some(std::time::Duration::from_secs(2)))?;
    writeln!(stream, "{}", request.to_line())
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

    /// A second launch's flags become the request the running instance gets.
    #[test]
    fn a_launch_line_round_trips_to_a_request() {
        let a = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Request::from_args(&a(&[])), Request::Show);
        assert_eq!(Request::from_args(&a(&["--launcher"])), Request::Launcher);
        assert_eq!(
            Request::from_args(&a(&["--capture", "window"])),
            Request::Capture(CaptureKind::Window)
        );
        assert_eq!(
            Request::from_args(&a(&["--capture", "bogus"])),
            Request::Show
        );
        for r in [
            Request::Show,
            Request::Launcher,
            Request::Library,
            Request::Settings,
            Request::Capture(CaptureKind::Region),
            Request::Capture(CaptureKind::Display),
        ] {
            assert_eq!(Request::from_line(r.to_line()), Some(r));
        }
        assert_eq!(Request::from_line("rm -rf"), None);
    }

    /// The first instance receives what a second launch sends.
    #[cfg(unix)]
    #[test]
    fn a_request_reaches_the_running_instance() {
        let dir = tempfile::tempdir().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        serve(dir.path(), move |r| {
            let _ = tx.lock().unwrap().send(r);
        })
        .unwrap();
        send(dir.path(), Request::Capture(CaptureKind::Region)).unwrap();
        let got = rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(got, Request::Capture(CaptureKind::Region));
        // No listener: the error is returned, not swallowed.
        assert!(send(tempfile::tempdir().unwrap().path(), Request::Show).is_err());
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
