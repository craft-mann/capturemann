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

fn spawn_daemon(endpoint: &Endpoint, library: Option<&Path>) -> io::Result<()> {
    let library = library
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("CAPTUREMANN_LIBRARY").map(PathBuf::from));
    let bin = daemon_binary();
    let mut cmd = std::process::Command::new(&bin);
    cmd.args(daemon_args(endpoint, library.as_deref()));
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    cmd.spawn().map(|_child| ()).map_err(|e| {
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
