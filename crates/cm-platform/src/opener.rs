//! Handing a link or a path to the desktop's opener, with an error that says what to install when
//! the opener is missing (a bare "entity not found" helps nobody).

use std::ffi::OsStr;
use std::process::{Command, Stdio};

/// Why a link or path could not be handed to the opener.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenError {
    /// The opener program is not installed; the string is its name.
    OpenerMissing(&'static str),
    /// The opener could not be started for another reason.
    Failed(std::io::ErrorKind),
}

impl OpenError {
    /// A one-line message with the install hint, free of the target (a link can carry secrets).
    pub fn message(&self) -> String {
        match self {
            OpenError::OpenerMissing(tool) => {
                format!("{tool} is not installed; install xdg-utils to open links and folders")
            }
            OpenError::Failed(kind) => format!("could not start the system opener: {kind}"),
        }
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for OpenError {}

/// Starts `program <args> <target>` detached and reaps it in the background. `tool` names the
/// program in the error.
pub fn spawn_opener(
    program: &OsStr,
    tool: &'static str,
    args: &[&OsStr],
    target: &OsStr,
) -> Result<(), OpenError> {
    match Command::new(program)
        .args(args)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(OpenError::OpenerMissing(tool)),
        Err(e) => Err(OpenError::Failed(e.kind())),
    }
}

/// Opens a link or a path with the system's opener: `xdg-open` on Linux and other Unix systems,
/// `open` on macOS, `rundll32 url.dll,FileProtocolHandler` on Windows.
pub fn open_target(target: &OsStr) -> Result<(), OpenError> {
    if cfg!(windows) {
        spawn_opener(
            OsStr::new("rundll32"),
            "rundll32",
            &[OsStr::new("url.dll,FileProtocolHandler")],
            target,
        )
    } else if cfg!(target_os = "macos") {
        spawn_opener(OsStr::new("open"), "open", &[], target)
    } else {
        spawn_opener(OsStr::new("xdg-open"), "xdg-open", &[], target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_opener_names_what_to_install() {
        let e = spawn_opener(
            OsStr::new("cm-no-such-opener-7f3a"),
            "xdg-open",
            &[],
            OsStr::new("https://example.com/?code=secret"),
        )
        .unwrap_err();
        assert_eq!(e, OpenError::OpenerMissing("xdg-open"));
        assert!(e.message().contains("install xdg-utils"));
        assert!(!e.message().contains("secret"));
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_opener_is_started() {
        spawn_opener(OsStr::new("true"), "true", &[], OsStr::new("x")).unwrap();
    }
}
