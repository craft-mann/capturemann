//! The MCP client config snippet copied by "Copy MCP server config" (LNC-1.AC8). Same shape
//! as the Settings -> Agents & CLI card: stdio transport through the real `capturemann mcp` command and
//! the real daemon socket. The HTTP bearer token is never part of it (AGT-1).

use cm_daemon::Endpoint;
use serde_json::json;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub fn binary_name() -> String {
    format!("capturemann{}", std::env::consts::EXE_SUFFIX)
}

/// Next to the running executable first (a bundled install), then in `PATH`.
pub fn locate(current_exe: Option<&Path>, path_env: Option<OsString>) -> Option<PathBuf> {
    let name = binary_name();
    if let Some(p) = current_exe
        .and_then(Path::parent)
        .map(|d| d.join(&name))
        .filter(|p| p.is_file())
    {
        return Some(p);
    }
    path_env.and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    })
}

/// The JSON for an MCP client's config file. `command` falls back to the bare binary name
/// when `capturemann` cannot be found, which works when it is on the client's `PATH`.
pub fn snippet(command: Option<&Path>, endpoint: Option<&Endpoint>) -> String {
    let command = command.map_or_else(binary_name, |p| p.to_string_lossy().into_owned());
    let mut args: Vec<String> = vec!["mcp".to_owned()];
    match endpoint {
        Some(Endpoint::Path(p)) => {
            args.push("--socket".to_owned());
            args.push(p.to_string_lossy().into_owned());
        }
        Some(Endpoint::Pipe(name)) => {
            args.push("--socket".to_owned());
            args.push(name.clone());
        }
        None => {}
    }
    let value = json!({
        "mcpServers": {
            "capturemann": { "command": command, "args": args }
        }
    });
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    /// LNC-1.AC8: the snippet is valid JSON with the real binary path and socket.
    #[test]
    fn snippet_has_command_and_socket() {
        let s = snippet(
            Some(Path::new(
                "/Applications/CaptureMann.app/Contents/MacOS/capturemann",
            )),
            Some(&Endpoint::Path(PathBuf::from("/tmp/cm.sock"))),
        );
        let v: Value = serde_json::from_str(&s).unwrap();
        let server = &v["mcpServers"]["capturemann"];
        assert_eq!(
            server["command"],
            "/Applications/CaptureMann.app/Contents/MacOS/capturemann"
        );
        assert_eq!(server["args"], json!(["mcp", "--socket", "/tmp/cm.sock"]));
    }

    #[test]
    fn snippet_without_endpoint_or_binary() {
        let v: Value = serde_json::from_str(&snippet(None, None)).unwrap();
        assert_eq!(v["mcpServers"]["capturemann"]["command"], binary_name());
        assert_eq!(v["mcpServers"]["capturemann"]["args"], json!(["mcp"]));
    }

    /// No secret is ever part of the snippet (AGT-1).
    #[test]
    fn snippet_has_no_token() {
        let s = snippet(None, Some(&Endpoint::Pipe("cm".into())));
        assert!(!s.to_lowercase().contains("token") && !s.to_lowercase().contains("bearer"));
    }

    #[test]
    fn locate_prefers_the_executable_directory() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join(binary_name());
        std::fs::write(&bin, b"x").unwrap();
        let exe = dir.path().join("cm-desktop");
        assert_eq!(locate(Some(&exe), None), Some(bin));
        assert_eq!(
            locate(Some(Path::new("/nonexistent/cm-desktop")), None),
            None
        );
    }
}
