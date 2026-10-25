//! Launching the `cm-overlay` helper (region picker and friends) and reading its result.
//!
//! Contract with `apps/overlay`:
//!
//! * Invocation: `cm-overlay <region|window|display> [--socket <daemon endpoint>]`,
//!   found next to this executable or on `PATH`.
//! * Result on stdout, one JSON object on the last non-empty line: `{"capture_id": ..., ...}`
//!   after a capture; `{"cancelled": true}` with exit 3 (`user_declined`) when the user
//!   cancelled with Esc (CAP-1.AC6). Failures print `{"error": {"code", "message"}}` on
//!   stderr and exit with the shared code (4 = `permission_missing`, 5 = daemon unreachable).

use cm_i18n::tr;
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Region,
    Window,
    Display,
}

impl CaptureMode {
    pub fn arg(self) -> &'static str {
        match self {
            CaptureMode::Region => "region",
            CaptureMode::Window => "window",
            CaptureMode::Display => "display",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayOutcome {
    Captured {
        capture_id: String,
    },
    /// Esc, or the overlay closed without a capture.
    Cancelled,
}

/// What `cm-overlay --pick` returns: the chosen target, nothing captured (REC-1: the start sheet
/// picks a window or a region to record).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picked {
    pub display_id: u32,
    pub window_id: Option<u32>,
    /// `[x, y, w, h]` physical px relative to the display (region mode).
    pub rect: Option<[i64; 4]>,
}

impl Picked {
    /// The `start_recording` target.
    pub fn target(&self) -> crate::recording::start::Target {
        use crate::recording::start::Target;
        match (self.window_id, self.rect) {
            (Some(window_id), _) => Target::Window { window_id },
            (None, Some(rect)) => Target::Region {
                display_id: self.display_id,
                rect,
            },
            (None, None) => Target::Display {
                display_id: self.display_id,
            },
        }
    }
}

/// Interprets a `--pick` run: the chosen target, `None` when the user pressed Esc.
pub fn parse_pick(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<Option<Picked>, OverlayError> {
    let last = stdout.lines().rev().find(|l| !l.trim().is_empty());
    let picked = last
        .and_then(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .and_then(|v| v.get("picked").cloned());
    if let Some(p) = picked {
        let display_id = p
            .get("display_id")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| OverlayError::BadOutput(p.to_string()))?;
        let rect = p.get("rect").and_then(Value::as_array).and_then(|a| {
            Some([
                a.first()?.as_i64()?,
                a.get(1)?.as_i64()?,
                a.get(2)?.as_i64()?,
                a.get(3)?.as_i64()?,
            ])
        });
        return Ok(Some(Picked {
            display_id,
            window_id: p
                .get("window_id")
                .and_then(Value::as_u64)
                .and_then(|n| u32::try_from(n).ok()),
            rect,
        }));
    }
    match parse_result(exit_code, stdout, stderr)? {
        OverlayOutcome::Cancelled => Ok(None),
        OverlayOutcome::Captured { .. } => Err(OverlayError::BadOutput(
            "the overlay took a screenshot instead of picking".to_owned(),
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayError {
    /// `cm-overlay` is not installed next to the app or on PATH.
    NotFound,
    Spawn(String),
    PermissionMissing(String),
    Failed(String),
    BadOutput(String),
    /// The overlay did not finish within the watchdog time and was killed (R4-20).
    TimedOut,
}

impl OverlayError {
    /// Text for the error toast.
    pub fn user_message(&self) -> String {
        match self {
            OverlayError::NotFound => tr!(
                "The capture overlay (cm-overlay) was not found. Reinstall CaptureMann or put cm-overlay next to the app."
            ),
            OverlayError::Spawn(e) => tr!("Couldn’t start the capture overlay: {e}", e = e),
            OverlayError::PermissionMissing(m) => {
                tr!("Screen Recording permission is missing. {m}", m = m)
            }
            OverlayError::Failed(m) => tr!("Capture failed: {m}", m = m),
            OverlayError::BadOutput(m) => {
                tr!("The capture overlay returned something unexpected: {m}", m = m)
            }
            OverlayError::TimedOut => tr!(
                "The capture overlay stopped responding and was closed. Try the shortcut again."
            ),
        }
    }
}

/// The environment variable `cm-overlay` reads its UI language from (`apps/overlay/src/main.rs`).
pub const LANG_ENV: &str = "CAPTUREMANN_LANG";

/// The code to pass in [`LANG_ENV`]: the app's current UI language. The pseudo-locale is passed on
/// by its own environment variable, which the child inherits.
pub fn child_language_code() -> &'static str {
    match cm_i18n::language() {
        cm_i18n::Lang::EnXa => cm_i18n::Lang::En.code(),
        l => l.code(),
    }
}

pub fn binary_name() -> String {
    format!("cm-overlay{}", std::env::consts::EXE_SUFFIX)
}

/// Looks next to the running executable first (a bundled install), then in `PATH`.
pub fn locate(current_exe: Option<&Path>, path_env: Option<OsString>) -> Option<PathBuf> {
    let name = binary_name();
    if let Some(dir) = current_exe.and_then(Path::parent) {
        let p = dir.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    path_env.and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    })
}

/// Interprets the helper's exit status and output (see the module docs).
pub fn parse_result(
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
) -> Result<OverlayOutcome, OverlayError> {
    let last = stdout.lines().rev().find(|l| !l.trim().is_empty());
    let parsed: Option<Value> = last
        .and_then(|l| serde_json::from_str(l.trim()).ok())
        .or_else(|| {
            stderr
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .and_then(|l| serde_json::from_str(l.trim()).ok())
        });
    if let Some(v) = &parsed {
        if v.get("cancelled").and_then(Value::as_bool) == Some(true) {
            return Ok(OverlayOutcome::Cancelled);
        }
        if let Some(id) = v.get("capture_id").and_then(Value::as_str) {
            if exit_code.unwrap_or(0) == 0 {
                return Ok(OverlayOutcome::Captured {
                    capture_id: id.to_owned(),
                });
            }
        }
        if let Some(e) = v.get("error") {
            let msg = e
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error") // i18n-ignore: detail from the child process
                .to_owned();
            return Err(
                if e.get("code").and_then(Value::as_str) == Some("permission_missing") {
                    OverlayError::PermissionMissing(msg)
                } else {
                    OverlayError::Failed(msg)
                },
            );
        }
    }
    match exit_code {
        Some(0) if last.is_none() => Ok(OverlayOutcome::Cancelled),
        Some(3) => Ok(OverlayOutcome::Cancelled),
        Some(0) => Err(OverlayError::BadOutput(last.unwrap_or_default().to_owned())),
        Some(4) => Err(OverlayError::PermissionMissing(stderr.trim().to_owned())),
        Some(code) => {
            let msg = stderr.trim();
            Err(OverlayError::Failed(if msg.is_empty() {
                format!("overlay exited with status {code}") // i18n-ignore: technical detail
            } else {
                msg.to_owned()
            }))
        }
        None => Err(OverlayError::Failed("overlay was terminated".to_owned())), // i18n-ignore: technical detail,
    }
}

#[derive(Debug, Clone)]
pub struct OverlayLauncher {
    binary: Option<PathBuf>,
    /// Daemon endpoint the overlay should talk to (the socket this app serves).
    socket: Option<PathBuf>,
    /// How long a picker may stay open before it is killed (R4-20).
    watchdog: Duration,
}

/// A picker open for two minutes is a hung one: a person picks in seconds. Without a limit a hung
/// overlay keeps the capture busy flag set and every capture shortcut is ignored until the app is
/// restarted.
pub const OVERLAY_WATCHDOG: Duration = Duration::from_secs(120);

impl OverlayLauncher {
    pub fn new(binary: Option<PathBuf>, socket: Option<PathBuf>) -> Self {
        OverlayLauncher {
            binary,
            socket,
            watchdog: OVERLAY_WATCHDOG,
        }
    }

    /// The same launcher with another watchdog (tests).
    pub fn with_watchdog(mut self, watchdog: Duration) -> Self {
        self.watchdog = watchdog;
        self
    }

    pub fn args(&self, mode: CaptureMode) -> Vec<OsString> {
        self.args_with(mode, false)
    }

    /// `pick`: choose a target only (`--pick`), no screenshot.
    pub fn args_with(&self, mode: CaptureMode, pick: bool) -> Vec<OsString> {
        let mut a: Vec<OsString> = vec![mode.arg().into()];
        if pick {
            a.push("--pick".into());
        }
        if let Some(s) = &self.socket {
            a.push("--socket".into());
            a.push(s.clone().into_os_string());
        }
        a
    }

    pub fn is_available(&self) -> bool {
        self.binary.is_some()
    }

    /// Runs the overlay to completion. The child is killed if this future is dropped.
    pub async fn run(&self, mode: CaptureMode) -> Result<OverlayOutcome, OverlayError> {
        let (code, out, err) = self.run_raw(mode, false).await?;
        parse_result(code, &out, &err)
    }

    /// Runs the picker only: the chosen target, or `None` on Esc (REC-1).
    pub async fn pick(&self, mode: CaptureMode) -> Result<Option<Picked>, OverlayError> {
        let (code, out, err) = self.run_raw(mode, true).await?;
        parse_pick(code, &out, &err)
    }

    async fn run_raw(
        &self,
        mode: CaptureMode,
        pick: bool,
    ) -> Result<(Option<i32>, String, String), OverlayError> {
        let Some(bin) = &self.binary else {
            return Err(OverlayError::NotFound);
        };
        crate::trace::mark("overlay_task_started");
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(self.args_with(mode, pick))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        // The overlay is its own process: hand it the UI language (its hint labels are translated).
        cmd.env(LANG_ENV, child_language_code());
        let tracing = crate::trace::enabled();
        if tracing {
            cmd.env("CM_OVERLAY_TRACE", "1");
        }
        // Debug: extra overlay flags (for example `--exit-after-first-frame`) for benchmarks.
        if let Some(extra) = std::env::var_os("CM_OVERLAY_EXTRA_ARGS") {
            cmd.args(extra.to_string_lossy().split_whitespace());
        }
        let map_spawn = |e: std::io::Error| {
            if e.kind() == std::io::ErrorKind::NotFound {
                OverlayError::NotFound
            } else {
                OverlayError::Spawn(e.to_string())
            }
        };
        crate::trace::mark("overlay_spawn_begin");
        let child = cmd.spawn().map_err(map_spawn)?;
        crate::trace::mark("overlay_spawned");
        // `kill_on_drop`: when the watchdog fires the pending wait is dropped with the child in it,
        // which kills the overlay; the caller then clears the capture-busy flag (R4-20).
        let out = match tokio::time::timeout(self.watchdog, child.wait_with_output()).await {
            Ok(out) => out.map_err(map_spawn)?,
            Err(_) => {
                crate::trace::mark("overlay_watchdog");
                return Err(OverlayError::TimedOut);
            }
        };
        crate::trace::mark("overlay_exited");
        if tracing {
            eprint!("{}", String::from_utf8_lossy(&out.stderr));
        }
        Ok((
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CAP-1: the overlay hands back a capture id.
    #[test]
    fn captured() {
        let r = parse_result(Some(0), "noise\n{\"capture_id\":\"shot_1\"}\n", "");
        assert_eq!(
            r,
            Ok(OverlayOutcome::Captured {
                capture_id: "shot_1".into()
            })
        );
    }

    /// CAP-1.AC6: Esc closes the overlay and nothing is saved: no error, no capture.
    #[test]
    fn cancelled_is_not_an_error() {
        assert_eq!(parse_result(Some(0), "", ""), Ok(OverlayOutcome::Cancelled));
        assert_eq!(
            parse_result(Some(0), "\n \n", ""),
            Ok(OverlayOutcome::Cancelled)
        );
    }

    /// CAP-1 edge case: missing Screen Recording permission.
    #[test]
    fn permission_missing() {
        assert!(matches!(
            parse_result(Some(4), "", "screen"),
            Err(OverlayError::PermissionMissing(_))
        ));
        let r = parse_result(
            Some(1),
            "{\"error\":{\"code\":\"permission_missing\",\"message\":\"Allow it\"}}",
            "",
        );
        assert_eq!(r, Err(OverlayError::PermissionMissing("Allow it".into())));
    }

    #[test]
    fn failures_keep_the_message() {
        assert_eq!(
            parse_result(Some(2), "", "bad window\n"),
            Err(OverlayError::Failed("bad window".into()))
        );
        assert_eq!(
            parse_result(Some(2), "", ""),
            Err(OverlayError::Failed("overlay exited with status 2".into()))
        );
        assert!(matches!(
            parse_result(None, "", ""),
            Err(OverlayError::Failed(_))
        ));
        assert!(matches!(
            parse_result(Some(0), "hello", ""),
            Err(OverlayError::BadOutput(_))
        ));
    }

    /// I18N P2: the overlay process gets the UI language through `CAPTUREMANN_LANG`.
    #[test]
    fn the_overlay_is_told_the_ui_language() {
        use cm_i18n::{with_language, Lang};
        assert_eq!(with_language(Lang::En, child_language_code), "en");
        assert_eq!(with_language(Lang::Vi, child_language_code), "vi");
        // The pseudo-locale travels by its own variable; the child gets plain English.
        assert_eq!(with_language(Lang::EnXa, child_language_code), "en");
        assert_eq!(Lang::from_setting(Some(child_language_code())), Lang::En);
    }

    #[test]
    fn error_texts_keep_their_placeholders_in_every_language() {
        use cm_i18n::{with_language, Lang};
        let e = OverlayError::Failed("boom".into());
        assert_eq!(
            with_language(Lang::En, || e.user_message()),
            "Capture failed: boom"
        );
        assert!(with_language(Lang::Vi, || e.user_message()).contains("boom"));
    }

    #[test]
    fn missing_binary_has_a_clear_message() {
        let l = OverlayLauncher::new(None, None);
        assert!(!l.is_available());
        let err = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(l.run(CaptureMode::Region))
            .unwrap_err();
        assert_eq!(err, OverlayError::NotFound);
        assert!(err.user_message().contains("cm-overlay"));
    }

    #[test]
    fn locates_next_to_exe_then_path() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join(binary_name());
        std::fs::write(&bin, "").unwrap();
        let exe = dir.path().join("cm-desktop");
        assert_eq!(locate(Some(&exe), None), Some(bin.clone()));
        let other = tempfile::tempdir().unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(locate(Some(&other.path().join("x")), Some(path)), Some(bin));
        assert_eq!(locate(Some(&other.path().join("x")), None), None);
    }

    /// CAP-1.AC6: the overlay reports Esc as `{"cancelled":true}` with exit 3.
    #[test]
    fn esc_is_cancelled() {
        assert_eq!(
            parse_result(Some(3), "{\"cancelled\":true}\n", ""),
            Ok(OverlayOutcome::Cancelled)
        );
        assert_eq!(parse_result(Some(3), "", ""), Ok(OverlayOutcome::Cancelled));
    }

    /// The overlay prints failures as JSON on stderr.
    #[test]
    fn stderr_error_is_parsed() {
        let r = parse_result(
            Some(4),
            "",
            "{\"error\":{\"code\":\"permission_missing\",\"message\":\"screen\"}}\n",
        );
        assert_eq!(r, Err(OverlayError::PermissionMissing("screen".into())));
    }

    /// REC-1: `--pick` returns the target for `start_recording`, and Esc is not an error.
    #[test]
    fn pick_results() {
        let r = parse_pick(
            Some(0),
            "{\"picked\":{\"mode\":\"region\",\"display_id\":2,\"rect\":[10,20,300,200]}}\n",
            "",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            r.target(),
            crate::recording::start::Target::Region {
                display_id: 2,
                rect: [10, 20, 300, 200]
            }
        );
        let w = parse_pick(
            Some(0),
            "{\"picked\":{\"mode\":\"window\",\"display_id\":1,\"window_id\":77}}",
            "",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            w.target(),
            crate::recording::start::Target::Window { window_id: 77 }
        );
        let d = parse_pick(
            Some(0),
            "{\"picked\":{\"mode\":\"display\",\"display_id\":3}}",
            "",
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            d.target(),
            crate::recording::start::Target::Display { display_id: 3 }
        );
        assert_eq!(parse_pick(Some(3), "{\"cancelled\":true}", ""), Ok(None));
        assert!(matches!(
            parse_pick(Some(4), "", "screen"),
            Err(OverlayError::PermissionMissing(_))
        ));
        assert!(matches!(
            parse_pick(Some(0), "{\"capture_id\":\"shot_1\"}", ""),
            Err(OverlayError::BadOutput(_))
        ));
    }

    #[test]
    fn pick_adds_the_flag() {
        let l = OverlayLauncher::new(None, Some(PathBuf::from("/tmp/d.sock")));
        assert_eq!(
            l.args_with(CaptureMode::Region, true),
            ["region", "--pick", "--socket", "/tmp/d.sock"]
        );
    }

    #[test]
    fn args_pass_mode_and_socket() {
        let l = OverlayLauncher::new(None, Some(PathBuf::from("/tmp/d.sock")));
        let a = l.args(CaptureMode::Window);
        assert_eq!(a, ["window", "--socket", "/tmp/d.sock"]);
    }

    /// R4-20: an overlay that hangs is killed by the watchdog and reported, so the capture shortcuts
    /// work again.
    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn r4_20_a_hung_overlay_is_killed() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("hung-overlay");
        let marker = dir.path().join("alive");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let l = OverlayLauncher::new(Some(script), None).with_watchdog(Duration::from_millis(1500));
        let started = std::time::Instant::now();
        assert_eq!(
            l.run(CaptureMode::Region).await,
            Err(OverlayError::TimedOut)
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        // The child is gone.
        let pid: i32 = std::fs::read_to_string(&marker)
            .expect("the script ran")
            .trim()
            .parse()
            .unwrap();
        let mut gone = false;
        for _ in 0..50 {
            // SAFETY: signal 0 only checks that the process exists.
            if unsafe { libc::kill(pid, 0) } != 0 {
                gone = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(gone, "the hung overlay (pid {pid}) is still running");
        assert!(OverlayError::TimedOut
            .user_message()
            .contains("stopped responding"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_a_real_child_process() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("fake-overlay");
        std::fs::write(
            &script,
            "#!/bin/sh\necho \"{\\\"capture_id\\\":\\\"shot_9\\\"}\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let l = OverlayLauncher::new(Some(script), None);
        assert_eq!(
            l.run(CaptureMode::Display).await,
            Ok(OverlayOutcome::Captured {
                capture_id: "shot_9".into()
            })
        );
    }
}
