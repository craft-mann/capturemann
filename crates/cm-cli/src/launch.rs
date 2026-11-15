//! Starting a headless daemon on demand (AGT-1: "if the GUI isn't running, the CLI starts a
//! headless daemon").

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cm_ipc::Endpoint;

/// Starts a daemon listening on the endpoint and returns without waiting for it. Injectable so
/// tests run the daemon in process.
pub type Launcher = Arc<dyn Fn(&Endpoint) -> io::Result<()> + Send + Sync>;

/// Binary name of the headless daemon.
pub const DAEMON_BIN: &str = if cfg!(windows) {
    "cm-daemon.exe"
} else {
    "cm-daemon"
};

/// Binary name of the MCP server.
pub const MCP_BIN: &str = if cfg!(windows) {
    "cm-mcp.exe"
} else {
    "cm-mcp"
};

/// Where to find `cm-mcp`: `CAPTUREMANN_MCP`, a sibling of this executable, else `PATH`.
pub fn mcp_binary() -> PathBuf {
    if let Some(p) = std::env::var_os("CAPTUREMANN_MCP") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(sibling) = exe.parent().map(|d| d.join(MCP_BIN)) {
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(MCP_BIN)
}

/// Where to find `cm-daemon`: `CAPTUREMANN_DAEMON`, a sibling of this executable, else `PATH`.
pub fn daemon_binary() -> PathBuf {
    if let Some(p) = std::env::var_os("CAPTUREMANN_DAEMON") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(DAEMON_BIN);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(DAEMON_BIN)
}

/// The production launcher: spawns `cm-daemon --library <dir> --socket <endpoint>` detached
/// from this process (no stdio). `--library` is passed only when `library` or
/// `CAPTUREMANN_LIBRARY` names one; otherwise `cm-daemon` picks the folder itself with the same
/// rule as the app (`library.path` in the app's settings, then `~/Movies/CaptureMann`), so a
/// daemon the CLI starts uses the library the app shows (R2-8).
pub fn process_launcher(library: Option<PathBuf>) -> Launcher {
    Arc::new(move |endpoint: &Endpoint| spawn_daemon(endpoint, library.as_deref()))
}

/// The arguments `cm-daemon` is started with: `--library` only for an explicit folder,
/// `--socket` only for a non-default endpoint.
fn daemon_args(endpoint: &Endpoint, library: Option<&Path>) -> Vec<std::ffi::OsString> {
    let mut args = Vec::new();
    if let Some(library) = library.filter(|p| !p.as_os_str().is_empty()) {
        args.push("--library".into());
        args.push(library.as_os_str().to_owned());
    }
    if *endpoint != Endpoint::default_for_platform() {
        args.push("--socket".into());
        args.push(match endpoint {
            Endpoint::Path(p) => p.as_os_str().to_owned(),
            Endpoint::Pipe(n) => n.into(),
        });
    }
    args
}

/// The library folder a daemon started from here would open: the explicit one, then
/// `CAPTUREMANN_LIBRARY`, then `library.path` in the app's `settings.json` (the same rule as
/// `cm_daemon::config`, which the CLI does not link); `None` when the default folder applies.
fn library_to_open(explicit: Option<&Path>) -> Option<PathBuf> {
    let from_env = || {
        std::env::var_os("CAPTUREMANN_LIBRARY")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    let from_settings = || {
        let dir = std::env::var_os("CAPTUREMANN_CONFIG_DIR")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| cm_platform::dirs::config_base_dir().map(|d| d.join("CaptureMann")))?;
        let text = std::fs::read_to_string(dir.join("settings.json")).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let s = v.get("library.path")?.as_str()?.trim().to_owned();
        (!s.is_empty()).then(|| PathBuf::from(s))
    };
    explicit
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .or_else(from_env)
        .or_else(from_settings)
}

/// A library folder whose path is too long cannot be opened: the daemon would exit at once and
/// the CLI would wait 15 s for it and say only `daemon_unavailable` (QA Windows pass 3). Checked
/// before anything is started; the error is `invalid_input` naming the path and the limit.
pub(crate) fn check_library_path(explicit: Option<&Path>) -> Result<(), String> {
    match library_to_open(explicit) {
        Some(p) => cm_core::library_path::check_root_path_len(&p).map_err(|e| e.to_string()),
        None => Ok(()),
    }
}

fn spawn_daemon(endpoint: &Endpoint, library: Option<&Path>) -> io::Result<()> {
    check_library_path(library).map_err(|m| io::Error::new(io::ErrorKind::InvalidInput, m))?;
    let library = library
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("CAPTUREMANN_LIBRARY").map(PathBuf::from));
    let bin = daemon_binary();
    let mut cmd = cm_platform::process::command(&bin);
    cmd.args(daemon_args(endpoint, library.as_deref()));
    // Detached, stdio null and no inherited caller handles (a script capturing our output must
    // see end-of-file when we exit, not when the daemon does).
    cm_platform::process::spawn_detached(&mut cmd)
        .map(|_child| ())
        .map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "cannot run `{}`: {e} (set CAPTUREMANN_DAEMON to the cm-daemon binary)",
                    bin.display()
                ),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// QA Windows pass 3: a library path past the limit and no app running waited 15 s for a daemon
    /// that could not open it and answered `daemon_unavailable` without the path. The launcher
    /// now refuses at once, before it starts anything, and the client maps that to `invalid_input`
    /// naming the path and the limit.
    #[tokio::test]
    async fn a_library_path_past_the_limit_is_invalid_input_at_once() {
        let long = PathBuf::from(format!("/tmp/{}", "x".repeat(60).repeat(6)));
        assert!(long.to_string_lossy().chars().count() > 340);
        // No daemon listens here and `CAPTUREMANN_DAEMON` points nowhere: had the launcher gone
        // on to spawn, the error would be about the binary or the 15 s wait.
        let endpoint = Endpoint::Path(PathBuf::from("/tmp/cm-no-such-daemon-for-test.sock"));
        let launcher = process_launcher(Some(long.clone()));
        let started = std::time::Instant::now();
        let err = crate::client::Client::connect_or_start(&endpoint, Some(&launcher))
            .await
            .err()
            .expect("an error");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        assert_eq!(err.code, "invalid_input");
        let m = err.to_string();
        assert!(m.contains(&*long.to_string_lossy()), "{m}");
        assert!(m.contains("at most"), "{m}");
        assert!(check_library_path(Some(Path::new("/tmp/CaptureMann"))).is_ok());
    }

    /// R2-8: without an explicit library the daemon picks the app's folder itself; the CLI no
    /// longer invents `~/Library/Application Support/CaptureMann/Library`.
    #[test]
    fn r2_8_library_is_passed_only_when_explicit() {
        let custom = Endpoint::Path(PathBuf::from("/tmp/x.sock"));
        let a = daemon_args(&custom, None);
        assert_eq!(
            a,
            vec![std::ffi::OsString::from("--socket"), "/tmp/x.sock".into()]
        );
        let a = daemon_args(&custom, Some(Path::new("/lib")));
        assert_eq!(a[0], "--library");
        assert_eq!(a[1], "/lib");
        assert!(daemon_args(&Endpoint::default_for_platform(), None).is_empty());
        assert!(daemon_args(&Endpoint::default_for_platform(), Some(Path::new(""))).is_empty());
    }
}
