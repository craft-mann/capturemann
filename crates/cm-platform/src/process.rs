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
use std::io;
use std::process::{Child, Command, Stdio};

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

static SPAWN_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `cmd.spawn()` for a child whose stdout or stderr is read to the end of file: one spawn at a
/// time in this process.
///
/// Why: on Windows `std::process::Command` creates the child's pipe ends as inheritable and then
/// calls `CreateProcess` with `bInheritHandles = TRUE`. A second thread that starts another child
/// between those two steps hands that child the first child's pipe ends too. The second child
/// (a decoder that lives for many seconds) then keeps the first one's stdout open after the
/// first was killed, and a reader waiting for end-of-file blocks until the second child exits
/// (a cancelled export took 23 to 31 s on loaded GitHub runners, 0.3.1 plan section 5). Holding
/// one process-wide lock around every such spawn closes that window. The lock is held for the
/// spawn only, never while the child runs. Elsewhere it changes nothing visible.
pub fn spawn(cmd: &mut Command) -> io::Result<Child> {
    let _guard = SPAWN_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    cmd.spawn()
}

/// Windows: `CREATE_NEW_PROCESS_GROUP` (Ctrl+C aimed at the caller does not reach the child).
pub const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
/// Windows: `DETACHED_PROCESS` (the child has no console, not even the caller's).
pub const DETACHED_PROCESS: u32 = 0x0000_0008;

/// Starts a long-lived background program (the headless daemon) that must outlive the caller
/// and must not keep anything of the caller open: stdin, stdout and stderr go to the null
/// device and, on Windows, the child is detached from the console and the process group.
///
/// Why this exists: on Windows `CreateProcess` with `bInheritHandles = TRUE` (what
/// `std::process::Command` always uses) hands the child every inheritable handle of the
/// caller, including the caller's own stdout/stderr pipe, even when the child's stdio is
/// `Stdio::null()`. A script that captured the CLI's output then never saw end-of-file while
/// the daemon lived. So the inherit flag of the caller's three standard handles is cleared
/// just before the spawn and put back right after. The flag is process-wide: call this while
/// no other thread of the process starts a child that is meant to inherit those handles (the
/// CLI is single-threaded at that point). On Unix the child gets its own process group and
/// only file descriptors 0 to 2, which are null.
pub fn spawn_detached(command: &mut Command) -> io::Result<Child> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
        let _guard = sys::NoInherit::std_handles();
        command.spawn()
    }
    #[cfg(not(windows))]
    {
        command.spawn()
    }
}

#[cfg(windows)]
mod sys {
    use windows::Win32::Foundation::{HANDLE, HANDLE_FLAGS};
    use windows::Win32::System::Console::{
        GetStdHandle, STD_ERROR_HANDLE, STD_INPUT_HANDLE, STD_OUTPUT_HANDLE,
    };

    const HANDLE_FLAG_INHERIT: u32 = 1;

    /// Clears `HANDLE_FLAG_INHERIT` on the standard handles and restores it on drop.
    pub struct NoInherit(Vec<HANDLE>);

    impl NoInherit {
        pub fn std_handles() -> NoInherit {
            let mut changed = Vec::new();
            for which in [STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
                // SAFETY: plain queries and flag changes on the process's own standard handles;
                // a missing or invalid handle makes the call fail and is skipped.
                unsafe {
                    let Ok(h) = GetStdHandle(which) else { continue };
                    if h.is_invalid()
                        || h.0.is_null()
                        || changed.iter().any(|c: &HANDLE| c.0 == h.0)
                    {
                        continue;
                    }
                    let mut flags = 0u32;
                    if windows::Win32::Foundation::GetHandleInformation(h, &mut flags).is_err() {
                        continue;
                    }
                    if flags & HANDLE_FLAG_INHERIT != 0
                        && windows::Win32::Foundation::SetHandleInformation(
                            h,
                            HANDLE_FLAG_INHERIT,
                            HANDLE_FLAGS(0),
                        )
                        .is_ok()
                    {
                        changed.push(h);
                    }
                }
            }
            NoInherit(changed)
        }
    }

    impl Drop for NoInherit {
        fn drop(&mut self) {
            for h in &self.0 {
                // SAFETY: the handle was valid when it was changed; a failure is harmless.
                unsafe {
                    let _ = windows::Win32::Foundation::SetHandleInformation(
                        *h,
                        HANDLE_FLAG_INHERIT,
                        HANDLE_FLAGS(HANDLE_FLAG_INHERIT),
                    );
                }
            }
        }
    }
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

    /// Helper entry for `a_detached_child_does_not_hold_the_callers_output_pipe`: runs in a re-executed
    /// copy of this test binary whose stdout is a pipe, starts a long sleeper with
    /// [`spawn_detached`], prints its pid and exits.
    #[test]
    #[allow(clippy::zombie_processes)]
    fn detached_helper_entry() {
        if std::env::var_os("CM_SPAWN_DETACHED_HELPER").is_none() {
            return;
        }
        let mut c = if cfg!(windows) {
            let mut c = command("cmd");
            c.args(["/C", "ping -n 30 127.0.0.1"]);
            c
        } else {
            let mut c = command("sleep");
            c.arg("30");
            c
        };
        let child = spawn_detached(&mut c).unwrap();
        println!("CMPID={}", child.id());
    }

    /// A caller whose stdout is a pipe starts a detached child and exits: the reader of the pipe
    /// sees end-of-file at once (not when the child ends) and the child is still alive.
    #[test]
    fn a_detached_child_does_not_hold_the_callers_output_pipe() {
        use std::io::Read;
        use std::time::{Duration, Instant};
        let exe = std::env::current_exe().unwrap();
        let mut helper = Command::new(exe)
            .args([
                "--exact",
                "process::tests::detached_helper_entry",
                "--nocapture",
            ])
            .env("CM_SPAWN_DETACHED_HELPER", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut out = helper.stdout.take().unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut text = String::new();
            let _ = out.read_to_string(&mut text);
            let _ = tx.send(text);
        });
        let started = Instant::now();
        let text = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("the pipe never reached end-of-file: the detached child holds it");
        let _ = helper.wait();
        assert!(started.elapsed() < Duration::from_secs(10));
        let pid: u32 = text
            .lines()
            .find_map(|l| l.strip_prefix("CMPID="))
            .and_then(|p| p.trim().parse().ok())
            .unwrap_or_else(|| panic!("no pid in {text:?}"));
        let alive = alive(pid);
        kill(pid);
        assert!(alive, "the detached child should still be running");
    }

    #[cfg(unix)]
    fn alive(pid: u32) -> bool {
        // SAFETY: signal 0 only checks that the process exists.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(unix)]
    fn kill(pid: u32) {
        // SAFETY: ends the sleeper this test started.
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
    }

    #[cfg(windows)]
    fn alive(pid: u32) -> bool {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{
            GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        // SAFETY: opens, queries and closes a handle of a process this test started.
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return false;
            };
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code).is_ok();
            let _ = CloseHandle(h);
            ok && code == 259 // STILL_ACTIVE
        }
    }

    #[cfg(windows)]
    fn kill(pid: u32) {
        use windows::Win32::Foundation::CloseHandle;
        use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};
        // SAFETY: ends the pinger this test started.
        unsafe {
            if let Ok(h) = OpenProcess(PROCESS_TERMINATE, false, pid) {
                let _ = TerminateProcess(h, 1);
                let _ = CloseHandle(h);
            }
        }
    }
}
