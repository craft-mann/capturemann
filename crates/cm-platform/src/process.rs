//! Starting helper programs without a console window.
//!
//! The Windows builds of `cm-desktop`, `cm-overlay` and `cm-daemon` use the GUI subsystem, so they own
//! no console. A console program they start (`ffmpeg.exe`, `ffprobe.exe`, `powershell.exe`,
//! `llama-server.exe`, `rundll32.exe`, ...) then gets a new console window, which flashes on screen
//! next to every toast and at the end of every recording. `CREATE_NO_WINDOW` prevents that, and it
//! leaves piped stdin/stdout/stderr and the exit code untouched.
//!
//! Every place that starts a child program must go through [`command`] (or call [`hide_console`]);
//! `scripts/check_no_bare_command.py` (CI) rejects a bare `Command::new(` in non-test code. On other
//! systems both functions change nothing.

use std::ffi::OsStr;
use std::process::Command;

/// The Windows process creation flag that starts a console program without a console window.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Makes `cmd` start without a console window (Windows); no change elsewhere. Returns `cmd` so it
/// chains. For a `tokio::process::Command`, build it with
/// `tokio::process::Command::from(command(program))`.
pub fn hide_console(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// `Command::new(program)` with [`hide_console`] applied: the way to start a helper program.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    hide_console(&mut cmd);
    cmd
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_value_is_create_no_window() {
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }

    /// Pipes and the exit code still work with the flag (a real child; `cmd` on Windows, `sh` elsewhere).
    #[test]
    fn piped_output_and_exit_code_survive() {
        let mut c = if cfg!(windows) {
            let mut c = command("cmd");
            c.args(["/C", "echo hello& exit 3"]);
            c
        } else {
            let mut c = command("sh");
            c.args(["-c", "echo hello; exit 3"]);
            c
        };
        let out = c
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert!(String::from_utf8_lossy(&out.stdout).contains("hello"));
    }
}
