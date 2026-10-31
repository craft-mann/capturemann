use std::io;
use std::path::{Path, PathBuf};

/// Where the daemon listens: a socket path (Unix) or a pipe name (Windows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// Unix domain socket path.
    Path(PathBuf),
    /// Windows named pipe, e.g. `\\.\pipe\capturemann`.
    Pipe(String),
}

/// Longest usable Unix socket path in bytes (`sun_path` minus the NUL).
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
pub const MAX_SOCKET_PATH_BYTES: usize = 103;
/// Longest usable Unix socket path in bytes (`sun_path` minus the NUL).
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
pub const MAX_SOCKET_PATH_BYTES: usize = 107;

fn too_long(path: &Path) -> io::Error {
    let len = path.as_os_str().len();
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!(
            "socket path `{}` is {len} bytes; Unix sockets allow at most \
             {MAX_SOCKET_PATH_BYTES}. Use a shorter path (for example under /tmp)",
            path.display()
        ),
    )
}

/// A pipe name as the user typed it, made a full pipe path: `work` becomes `\\.\pipe\work`, a
/// name that already starts with `\\` (such as `\\.\pipe\work`) is kept.
pub fn pipe_name(typed: &str) -> String {
    if typed.starts_with(r"\\") {
        typed.to_owned()
    } else {
        format!(r"\\.\pipe\{typed}")
    }
}

impl Endpoint {
    /// The endpoint a user typed (`--socket`, `CAPTUREMANN_SOCKET`): a socket path on Unix, a
    /// pipe name on Windows (a bare name gets the `\\.\pipe\` prefix).
    pub fn from_arg(typed: &str) -> Endpoint {
        if cfg!(windows) {
            Endpoint::Pipe(pipe_name(typed))
        } else {
            Endpoint::Path(PathBuf::from(typed))
        }
    }

    /// `CAPTUREMANN_SOCKET` when it is set and not empty (a socket path on Unix, a pipe name on
    /// Windows), else [`Endpoint::default_for_platform`]. Every client and the daemon read the
    /// same variable, so a second instance can be pointed at its own endpoint.
    pub fn from_env_or_default() -> Endpoint {
        match std::env::var("CAPTUREMANN_SOCKET") {
            Ok(v) if !v.is_empty() => Endpoint::from_arg(&v),
            _ => Endpoint::default_for_platform(),
        }
    }

    /// The platform default (see the crate docs). Needs `HOME` on Unix; without it the socket
    /// goes to the system temp dir. If the preferred path is too long for a Unix socket (a
    /// very long user name), falls back to `/tmp/cm-<uid>/daemon.sock`.
    pub fn default_for_platform() -> Endpoint {
        #[cfg(windows)]
        {
            Endpoint::Pipe(r"\\.\pipe\capturemann".to_owned())
        }
        #[cfg(not(windows))]
        {
            let home = std::env::var_os("HOME").map(PathBuf::from);
            #[cfg(target_os = "macos")]
            let dir = home
                .map(|h| h.join("Library/Application Support/CaptureMann"))
                .unwrap_or_else(|| std::env::temp_dir().join("CaptureMann"));
            #[cfg(not(target_os = "macos"))]
            let dir = std::env::var_os("XDG_RUNTIME_DIR")
                .map(|d| PathBuf::from(d).join("capturemann"))
                .or_else(|| home.map(|h| h.join(".local/share/capturemann")))
                .unwrap_or_else(|| std::env::temp_dir().join("capturemann"));
            Endpoint::Path(dir.join("daemon.sock")).or_short_fallback()
        }
    }

    /// An endpoint for tests and throwaway tools, unique to `dir` and `name`: a socket file
    /// `<dir>/<name>.sock` on Unix, a pipe named from the process, the directory and `name` on
    /// Windows (pipe names live in one machine-wide namespace, so they cannot live in `dir`).
    pub fn scratch(dir: &Path, name: &str) -> Endpoint {
        #[cfg(windows)]
        {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            dir.hash(&mut h);
            Endpoint::Pipe(format!(
                r"\\.\pipe\cm-scratch-{}-{:016x}-{name}",
                std::process::id(),
                h.finish()
            ))
        }
        #[cfg(not(windows))]
        {
            Endpoint::Path(dir.join(format!("{name}.sock")))
        }
    }

    /// `self` when it fits a Unix socket address, else the short per-user fallback.
    pub fn or_short_fallback(self) -> Endpoint {
        match &self {
            Endpoint::Path(p) if p.as_os_str().len() > MAX_SOCKET_PATH_BYTES => {
                Endpoint::Path(short_runtime_dir().join("daemon.sock"))
            }
            _ => self,
        }
    }

    /// `invalid_input` if a Unix socket path is too long to bind or connect; always `Ok` for
    /// pipes. The message names the path and the limit.
    pub fn validate(&self) -> io::Result<()> {
        match self {
            Endpoint::Path(p) if p.as_os_str().len() > MAX_SOCKET_PATH_BYTES => Err(too_long(p)),
            _ => Ok(()),
        }
    }

    /// A printable form: the socket path or the pipe name.
    pub fn describe(&self) -> String {
        match self {
            Endpoint::Path(p) => p.display().to_string(),
            Endpoint::Pipe(n) => n.clone(),
        }
    }

    /// Server side: validates the endpoint and creates the socket's parent directory (mode
    /// 0700). For the short per-user fallback directory, which lives in a shared location, it
    /// also refuses a directory owned by someone else and tightens loose permissions.
    pub fn prepare(&self) -> io::Result<()> {
        self.validate()?;
        #[cfg(unix)]
        if let Endpoint::Path(path) = self {
            unix_prepare(path)?;
        }
        Ok(())
    }
}

/// Runs `f` with the process umask set to `0o077` (owner-only files), then restores it. Used
/// around `bind` so the socket never exists with looser permissions, not even briefly. The umask
/// is process-wide; a lock makes concurrent callers take turns so the restore is always the
/// original value.
#[cfg(unix)]
pub fn with_private_umask<T>(f: impl FnOnce() -> T) -> T {
    use std::sync::{Mutex, PoisonError};
    static UMASK: Mutex<()> = Mutex::new(());
    let _g = UMASK.lock().unwrap_or_else(PoisonError::into_inner);
    // SAFETY: `umask` has no preconditions and cannot fail; the lock serializes our own calls.
    let old = unsafe { libc::umask(0o077) };
    let out = f();
    // SAFETY: as above.
    unsafe { libc::umask(old) };
    out
}

/// `/tmp/cm-<uid>`: short enough for any Unix socket address.
#[cfg(unix)]
fn short_runtime_dir() -> PathBuf {
    // SAFETY: `geteuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::geteuid() };
    PathBuf::from(format!("/tmp/cm-{uid}"))
}

#[cfg(not(unix))]
fn short_runtime_dir() -> PathBuf {
    std::env::temp_dir().join("cm")
}

#[cfg(unix)]
fn unix_prepare(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let Some(dir) = path.parent() else {
        return Ok(());
    };
    if !dir.exists() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    if dir == short_runtime_dir() {
        let meta = std::fs::symlink_metadata(dir)?;
        // SAFETY: `geteuid` has no preconditions and cannot fail.
        let euid = unsafe { libc::geteuid() };
        if !meta.is_dir() || meta.uid() != euid {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!(
                    "`{}` exists but is not a directory owned by you; refusing to put the \
                     daemon socket there",
                    dir.display()
                ),
            ));
        }
        if meta.permissions().mode() & 0o077 != 0 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
mod imp {
    use super::*;
    use tokio::net::UnixStream;

    pub type ClientStream = UnixStream;

    pub async fn connect(endpoint: &Endpoint) -> io::Result<ClientStream> {
        match endpoint {
            Endpoint::Path(p) => UnixStream::connect(p).await,
            Endpoint::Pipe(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a named pipe endpoint is not usable on this platform",
            )),
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeClient};

    pub type ClientStream = NamedPipeClient;

    pub async fn connect(endpoint: &Endpoint) -> io::Result<ClientStream> {
        const ERROR_PIPE_BUSY: i32 = 231;
        let Endpoint::Pipe(name) = endpoint else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a socket path endpoint is not usable on this platform",
            ));
        };
        loop {
            match ClientOptions::new().open(name) {
                Ok(c) => return Ok(c),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(e) => return Err(e),
            }
        }
    }
}

pub use imp::ClientStream;

/// Connects a client stream to a daemon endpoint. A Unix path that is too long is
/// `InvalidInput` (not a confusing `connect` failure).
pub async fn connect(endpoint: &Endpoint) -> io::Result<ClientStream> {
    endpoint.validate()?;
    imp::connect(endpoint).await
}

#[cfg(test)]
mod pipe_name_tests {
    use super::*;

    #[test]
    fn a_bare_name_becomes_a_pipe_path_and_a_full_path_is_kept() {
        assert_eq!(pipe_name("work"), r"\\.\pipe\work");
        assert_eq!(pipe_name(r"\\.\pipe\work"), r"\\.\pipe\work");
        assert_eq!(pipe_name(r"\\server\pipe\x"), r"\\server\pipe\x");
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// `CAPTUREMANN_SOCKET` picks the endpoint for the daemon, the CLI and MCP alike; an empty
    /// value is the same as unset. One test, because the environment is process-wide.
    #[test]
    fn the_environment_variable_picks_the_endpoint() {
        let old = std::env::var_os("CAPTUREMANN_SOCKET");
        std::env::set_var("CAPTUREMANN_SOCKET", "/tmp/cm-env-test.sock");
        assert_eq!(
            Endpoint::from_env_or_default(),
            Endpoint::Path(PathBuf::from("/tmp/cm-env-test.sock"))
        );
        std::env::set_var("CAPTUREMANN_SOCKET", "");
        assert_eq!(
            Endpoint::from_env_or_default(),
            Endpoint::default_for_platform()
        );
        match old {
            Some(v) => std::env::set_var("CAPTUREMANN_SOCKET", v),
            None => std::env::remove_var("CAPTUREMANN_SOCKET"),
        }
    }

    /// Session edge case: a path past the `sun_path` limit is `invalid_input` with the limit
    /// in the message, for validate, prepare and connect alike.
    #[tokio::test]
    async fn long_explicit_paths_are_invalid_input() {
        let long = Endpoint::Path(PathBuf::from(format!("/tmp/{}/d.sock", "x".repeat(120))));
        for e in [
            long.validate().unwrap_err(),
            long.prepare().unwrap_err(),
            connect(&long).await.unwrap_err(),
        ] {
            assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
            assert!(e.to_string().contains("at most"), "{e}");
        }
        let ok = Endpoint::Path(PathBuf::from("/tmp/ok.sock"));
        assert!(ok.validate().is_ok());
        assert!(Endpoint::Pipe("p".into()).validate().is_ok());
    }

    #[test]
    fn long_default_falls_back_to_a_short_per_user_dir() {
        let long = Endpoint::Path(PathBuf::from(format!(
            "/home/{}/daemon.sock",
            "u".repeat(150)
        )));
        let Endpoint::Path(p) = long.or_short_fallback() else {
            panic!("path expected")
        };
        assert!(
            p.as_os_str().len() <= MAX_SOCKET_PATH_BYTES,
            "{}",
            p.display()
        );
        assert!(
            p.to_string_lossy().starts_with("/tmp/cm-"),
            "{}",
            p.display()
        );
        let short = Endpoint::Path(PathBuf::from("/tmp/a.sock"));
        assert_eq!(short.clone().or_short_fallback(), short);
    }

    #[test]
    fn default_endpoint_always_fits() {
        let e = Endpoint::default_for_platform();
        assert!(e.validate().is_ok(), "{e:?}");
    }

    /// R2-7: files created inside are owner-only, and the previous umask comes back.
    #[test]
    fn r2_7_private_umask_applies_and_is_restored() {
        use std::os::unix::fs::PermissionsExt;
        let mode_of =
            |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        let t = tempfile::tempdir().unwrap();
        let before = t.path().join("before");
        std::fs::write(&before, b"x").unwrap();
        let inside = t.path().join("inside");
        with_private_umask(|| std::fs::write(&inside, b"x").unwrap());
        assert_eq!(mode_of(&inside) & 0o077, 0);
        let after = t.path().join("after");
        std::fs::write(&after, b"x").unwrap();
        assert_eq!(mode_of(&after), mode_of(&before), "the umask was restored");
    }

    #[test]
    fn prepare_creates_a_private_parent_dir() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let sock = t.path().join("a/b/daemon.sock");
        Endpoint::Path(sock.clone()).prepare().unwrap();
        let mode = std::fs::metadata(sock.parent().unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}
