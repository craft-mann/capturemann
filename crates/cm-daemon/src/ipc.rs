//! Local IPC transport: a Unix domain socket (macOS) or a named pipe (Windows).
//!
//! The client half ([`Endpoint`], [`connect`], [`ClientStream`], default paths and the socket
//! path length rules) lives in `cm-ipc` and is re-exported here; this module adds the
//! listener. Default endpoints and the long-path fallback are documented there.
//!
//! The Unix socket is created under a `0o077` umask (never briefly looser) with mode 0600 inside
//! a 0700 directory, so only the same user can connect. A stale *socket* file from a crashed
//! daemon is replaced, a live one is an `AddrInUse` error, and any other file at the path (a
//! regular file passed as `--socket` by mistake) is `InvalidInput` and left untouched (R2-7).
//! At most [`MAX_CONNECTIONS`] connections are served at once; further ones are closed at
//! once (R2-6). The Windows pipe rejects remote clients. Windows code is compiled only
//! there and is not exercised by the local test run (CI matrix / manual).

use std::io;
#[cfg(unix)]
use std::path::PathBuf;

use std::sync::Arc;
use tokio::sync::Semaphore;

use crate::daemon::Daemon;
use crate::rpc::serve_connection;

/// Most simultaneous connections served (R2-6).
pub const MAX_CONNECTIONS: usize = 64;

/// Serves `r`/`w` on a task unless the connection budget is used up, in which case the stream is
/// dropped (the peer sees EOF).
fn spawn_limited<R, W>(daemon: &Daemon, slots: &Arc<Semaphore>, r: R, w: W)
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    match slots.clone().try_acquire_owned() {
        Ok(permit) => {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                serve_connection(daemon, r, w).await;
                drop(permit);
            });
        }
        Err(_) => drop((r, w)),
    }
}

pub use cm_ipc::Endpoint;

#[cfg(unix)]
mod imp {
    use super::*;
    use std::os::unix::fs::{FileTypeExt, PermissionsExt};
    use tokio::net::UnixListener;

    pub struct Listener {
        inner: UnixListener,
        path: PathBuf,
    }

    pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
        let Endpoint::Path(path) = endpoint else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a named pipe endpoint is not usable on this platform",
            ));
        };
        // Length check (`invalid_input`), private parent directory.
        endpoint.prepare()?;
        match std::fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_socket() => {
                // A live daemon answers; a leftover socket from a crash does not.
                if std::os::unix::net::UnixStream::connect(path).is_ok() {
                    return Err(io::Error::new(
                        io::ErrorKind::AddrInUse,
                        format!("a daemon is already listening on {}", path.display()),
                    ));
                }
                std::fs::remove_file(path)?;
            }
            Ok(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "`{}` exists and is not a socket; refusing to replace it",
                        path.display()
                    ),
                ));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        // `tokio`'s bind is not async; do it with a private umask so the socket file is never
        // visible with looser permissions than 0600.
        let inner = cm_ipc::with_private_umask(|| UnixListener::bind(path))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(Listener {
            inner,
            path: path.clone(),
        })
    }

    impl Listener {
        pub async fn accept_loop(self, daemon: Daemon) -> io::Result<()> {
            let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
            loop {
                let (stream, _) = self.inner.accept().await?;
                let (r, w) = stream.into_split();
                spawn_limited(&daemon, &slots, r, w);
            }
        }
    }

    impl Drop for Listener {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::*;
    use tokio::net::windows::named_pipe::{NamedPipeServer, PipeMode, ServerOptions};

    pub struct Listener {
        name: String,
        next: NamedPipeServer,
    }

    fn create(name: &str, first: bool) -> io::Result<NamedPipeServer> {
        ServerOptions::new()
            .first_pipe_instance(first)
            .pipe_mode(PipeMode::Byte)
            .reject_remote_clients(true)
            .create(name)
    }

    pub fn bind(endpoint: &Endpoint) -> io::Result<Listener> {
        let Endpoint::Pipe(name) = endpoint else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "a socket path endpoint is not usable on this platform",
            ));
        };
        // `first_pipe_instance` fails if another daemon already owns the name.
        Ok(Listener {
            name: name.clone(),
            next: create(name, true)?,
        })
    }

    impl Listener {
        pub async fn accept_loop(mut self, daemon: Daemon) -> io::Result<()> {
            let slots = Arc::new(Semaphore::new(MAX_CONNECTIONS));
            loop {
                self.next.connect().await?;
                let connected = std::mem::replace(&mut self.next, create(&self.name, false)?);
                let (r, w) = tokio::io::split(connected);
                spawn_limited(&daemon, &slots, r, w);
            }
        }
    }
}

pub use cm_ipc::{connect, ClientStream};

/// A bound endpoint, not yet accepting. Dropping it removes the socket file (Unix).
pub struct IpcServer {
    listener: imp::Listener,
}

impl IpcServer {
    /// Binds the endpoint (failing with `AddrInUse` if a daemon already owns it).
    pub fn bind(endpoint: &Endpoint) -> io::Result<IpcServer> {
        Ok(IpcServer {
            listener: imp::bind(endpoint)?,
        })
    }

    /// Accepts connections forever, serving each on its own task. Returns only on an I/O
    /// error of the listener; cancel by dropping the future or `select!`ing on a shutdown.
    pub async fn run(self, daemon: Daemon) -> io::Result<()> {
        self.listener.accept_loop(daemon).await
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn ep(dir: &std::path::Path, name: &str) -> (Endpoint, PathBuf) {
        let p = dir.join(name);
        (Endpoint::Path(p.clone()), p)
    }

    /// R2-7: a regular file at the socket path is never deleted.
    #[test]
    fn r2_7_bind_refuses_to_remove_a_regular_file() {
        let t = tempfile::tempdir().unwrap();
        let (e, p) = ep(t.path(), "precious.txt");
        std::fs::write(&p, "my data").unwrap();
        let err = IpcServer::bind(&e).err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{err}");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "my data");
        // A symlink to a file is not a socket either, and is left alone.
        let link = t.path().join("link.sock");
        std::os::unix::fs::symlink(&p, &link).unwrap();
        let err = IpcServer::bind(&Endpoint::Path(link.clone()))
            .err()
            .unwrap();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(link.exists());
    }

    /// R2-7: a stale socket is replaced, a live one is `AddrInUse`, and the socket is 0600.
    #[tokio::test]
    async fn r2_7_stale_socket_is_replaced_live_one_is_not() {
        let t = tempfile::tempdir().unwrap();
        let (e, p) = ep(t.path(), "d.sock");
        // A stale socket: bind and leak the file by forgetting the listener's cleanup.
        {
            let l = std::os::unix::net::UnixListener::bind(&p).unwrap();
            drop(l); // the file stays, nobody listens
        }
        assert!(p.exists());
        let server = IpcServer::bind(&e).unwrap();
        let mode = std::fs::metadata(&p).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "mode {mode:o}");
        let again = IpcServer::bind(&e).err().unwrap();
        assert_eq!(again.kind(), io::ErrorKind::AddrInUse);
        drop(server);
        assert!(!p.exists(), "dropping the server removes the socket");
    }

    /// R2-6: connections beyond the cap are closed at once.
    #[tokio::test]
    async fn r2_6_connection_cap() {
        use tokio::io::AsyncReadExt;
        let t = tempfile::tempdir().unwrap();
        let (e, p) = ep(t.path(), "c.sock");
        let backend = cm_capture::fake::FakeBackend::new();
        let lib = cm_library::Library::open_or_create(&t.path().join("lib")).unwrap();
        let daemon =
            Daemon::builder(cm_capture::Capturer::new(backend).with_own_pid(None), lib).build();
        let server = IpcServer::bind(&e).unwrap();
        let task = tokio::spawn(server.run(daemon));
        let mut held = Vec::new();
        for _ in 0..MAX_CONNECTIONS {
            held.push(tokio::net::UnixStream::connect(&p).await.unwrap());
        }
        // Let the accept loop take them all.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let mut extra = tokio::net::UnixStream::connect(&p).await.unwrap();
        let mut b = [0u8; 1];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), extra.read(&mut b))
            .await
            .expect("the extra connection is closed, not left hanging")
            .unwrap();
        assert_eq!(n, 0, "EOF for the connection over the cap");
        // Freeing one slot lets a new connection in (it stays open: read times out).
        held.pop();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        let mut ok = tokio::net::UnixStream::connect(&p).await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(300), ok.read(&mut b))
                .await
                .is_err()
        );
        task.abort();
    }
}
