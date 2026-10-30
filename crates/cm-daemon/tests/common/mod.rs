#![allow(dead_code, clippy::unwrap_used)]

use cm_capture::fake::FakeBackend;
use cm_capture::Capturer;
use cm_daemon::{Daemon, DaemonBuilder};
use cm_library::Library;
use serde_json::{json, Value};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

pub fn desktop() -> FakeBackend {
    FakeBackend::new()
        .with_display(1, 0, 0, 400, 300, 2.0)
        .with_display(2, 400, 0, 200, 100, 1.0)
        .with_window(10, "Editor", "Code", 100, 1, (20, 10, 100, 80), false)
}

pub fn builder(backend: FakeBackend) -> (DaemonBuilder, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let lib = Library::open_or_create(dir.path()).unwrap();
    (
        Daemon::builder(Capturer::new(backend).with_own_pid(None), lib),
        dir,
    )
}

pub fn daemon() -> (Daemon, tempfile::TempDir) {
    let (b, dir) = builder(desktop());
    (b.build(), dir)
}

/// A JSON-RPC test client over an in-memory pipe served by the daemon.
pub struct Client {
    w: WriteHalf<DuplexStream>,
    r: Lines<BufReader<ReadHalf<DuplexStream>>>,
    next: u64,
    pub events: Vec<Value>,
    stash: Vec<Value>,
}

impl Client {
    pub fn connect(daemon: &Daemon, name: &str) -> Client {
        let (c, s) = tokio::io::duplex(1 << 22);
        let (sr, sw) = tokio::io::split(s);
        tokio::spawn(cm_daemon::serve_connection(daemon.clone(), sr, sw));
        let (cr, cw) = tokio::io::split(c);
        let client = Client {
            w: cw,
            r: BufReader::new(cr).lines(),
            next: 1,
            events: Vec::new(),
            stash: Vec::new(),
        };
        // The hello is sent lazily by `hello`.
        let _ = name;
        client
    }

    pub async fn hello(&mut self, name: &str) {
        let r = self.call("session.hello", json!({ "client": name })).await;
        assert_eq!(r["result"]["client"], name, "{r}");
    }

    pub async fn send_raw(&mut self, line: &str) {
        self.w.write_all(line.as_bytes()).await.unwrap();
        self.w.write_all(b"\n").await.unwrap();
    }

    /// Reads lines until the response to `id`; notifications are kept in `events`.
    pub async fn read_response(&mut self, id: &Value) -> Value {
        if let Some(i) = self.stash.iter().position(|v| &v["id"] == id) {
            return self.stash.remove(i);
        }
        loop {
            let line = self
                .r
                .next_line()
                .await
                .unwrap()
                .expect("connection closed");
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("method") == Some(&json!("event")) {
                self.events.push(v["params"].clone());
                continue;
            }
            if &v["id"] == id {
                return v;
            }
            self.stash.push(v);
        }
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Value {
        let id = self.send(method, params).await;
        self.read_response(&json!(id)).await
    }

    pub async fn send(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next;
        self.next += 1;
        let msg = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.send_raw(&msg.to_string()).await;
        id
    }

    /// Waits for the next event notification.
    pub async fn next_event(&mut self) -> Value {
        if !self.events.is_empty() {
            return self.events.remove(0);
        }
        loop {
            let line = self
                .r
                .next_line()
                .await
                .unwrap()
                .expect("connection closed");
            let v: Value = serde_json::from_str(&line).unwrap();
            if v.get("method") == Some(&json!("event")) {
                return v["params"].clone();
            }
        }
    }
}

/// Makes a library folder unreadable, as an unplugged drive is, and returns what [`replug`]
/// needs. Unix moves the folder away. Windows cannot move a folder that holds open files (the
/// library's SQLite index lives inside it), so it denies listing the folder to everyone: the
/// handles already open keep working and `read_dir` fails, as the watcher sees an unplugged drive.
#[allow(dead_code)]
pub fn unplug(root: &std::path::Path) -> std::path::PathBuf {
    #[cfg(windows)]
    {
        let ok = std::process::Command::new("icacls")
            .arg(root)
            .args(["/deny", "*S-1-1-0:(RD)"])
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "icacls /deny failed");
        root.to_owned()
    }
    #[cfg(not(windows))]
    {
        let away = root.with_extension("unplugged");
        std::fs::rename(root, &away).unwrap();
        away
    }
}

/// Undoes [`unplug`].
#[allow(dead_code)]
pub fn replug(root: &std::path::Path, token: &std::path::Path) {
    #[cfg(windows)]
    {
        let _ = token;
        let ok = std::process::Command::new("icacls")
            .arg(root)
            .args(["/remove:d", "*S-1-1-0"])
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "icacls /remove:d failed");
    }
    #[cfg(not(windows))]
    std::fs::rename(token, root).unwrap();
}
