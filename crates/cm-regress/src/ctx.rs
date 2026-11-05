//! The per-case context: a private sandbox (library, config folder, endpoint), the package under test,
//! helpers to run its CLI and start its daemon, and the evidence the case collects.

use crate::model::{fail, Stop, Tier, R};
use crate::proc::{self, Output, Tracker};
use crate::sys;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Settings of a run (the command line).
#[derive(Debug, Clone)]
pub struct Options {
    pub tier: Tier,
    pub only: Vec<String>,
    pub allow: BTreeSet<String>,
    pub package_dir: PathBuf,
    pub out_dir: PathBuf,
    pub keep: bool,
    pub list: bool,
    /// The package dir is a build tree (`target/debug`): no bundled FFmpeg, licences or helpers expected.
    pub dev: bool,
    pub installer: Option<PathBuf>,
    pub zip: Option<PathBuf>,
    pub commit: Option<String>,
    pub whisper_model: Option<PathBuf>,
    pub whisper_model_id: String,
    pub soak_minutes: u32,
    pub timeout_scale: f64,
    pub verdict: String,
    pub who: String,
    /// When the suite started (for the crash-event check at the end).
    pub started: std::time::SystemTime,
}

/// The package under test: a folder with the programs.
#[derive(Debug, Clone)]
pub struct Pkg {
    pub dir: PathBuf,
}

impl Pkg {
    pub fn exe(&self, base: &str) -> PathBuf {
        self.dir.join(sys::exe_name(base))
    }
    pub fn has(&self, base: &str) -> bool {
        self.exe(base).is_file()
    }
}

/// A daemon the case started.
pub struct Daemon {
    pub child: Child,
    pub pid: u32,
}

/// What `capturemann` printed.
#[derive(Debug, Clone)]
pub struct CliOut {
    pub out: Output,
    /// The JSON document on stdout, if it parsed.
    pub json: Option<Value>,
    /// The `error` object of a JSON error (stdout or stderr).
    pub error: Option<Value>,
}

impl CliOut {
    pub fn code(&self) -> Option<i32> {
        self.out.code
    }
    pub fn ok(&self) -> bool {
        self.out.success()
    }
    pub fn error_code(&self) -> Option<String> {
        self.error
            .as_ref()?
            .get("code")?
            .as_str()
            .map(str::to_owned)
    }
    /// `error.data.reason`, the finer code some errors carry (`no_model`, `app_only_setting`).
    pub fn error_reason(&self) -> Option<String> {
        self.error
            .as_ref()?
            .pointer("/data/reason")?
            .as_str()
            .map(str::to_owned)
    }
    pub fn error_message(&self) -> String {
        self.error
            .as_ref()
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    }
}

/// Finds the JSON error object in the output of a failed command.
pub fn parse_error(out: &Output) -> Option<Value> {
    for text in [&out.stdout, &out.stderr] {
        for line in text.lines().rev() {
            let line = line.trim();
            if line.starts_with('{') {
                if let Ok(v) = serde_json::from_str::<Value>(line) {
                    if let Some(e) = v.get("error") {
                        return Some(e.clone());
                    }
                }
            }
        }
        if let Ok(v) = serde_json::from_str::<Value>(text.trim()) {
            if let Some(e) = v.get("error") {
                return Some(e.clone());
            }
        }
    }
    None
}

pub struct Ctx {
    pub opts: Arc<Options>,
    pub pkg: Pkg,
    /// The case's scratch folder; removed after the case unless `--keep`.
    pub root: PathBuf,
    pub library: PathBuf,
    pub config: PathBuf,
    pub endpoint: String,
    pub evidence: Arc<Mutex<Vec<String>>>,
    pub tracker: Tracker,
    /// Log lines (substrings) this case expects, so the hygiene check lets them pass.
    pub log_allow: Vec<String>,
    pub daemon: Option<Daemon>,
    pub started_product: bool,
    pub timeout_scale: f64,
    /// Extra environment for every product program (a case can change PATH, for example).
    pub extra_env: Vec<(String, String)>,
}

impl Ctx {
    pub fn new(
        opts: Arc<Options>,
        root: PathBuf,
        evidence: Arc<Mutex<Vec<String>>>,
        tracker: Tracker,
        n: u32,
    ) -> std::io::Result<Ctx> {
        let library = root.join("library");
        let config = root.join("config");
        std::fs::create_dir_all(&library)?;
        std::fs::create_dir_all(&config)?;
        let endpoint = sys::new_endpoint(&root, n);
        Ok(Ctx {
            pkg: Pkg {
                dir: opts.package_dir.clone(),
            },
            timeout_scale: opts.timeout_scale,
            opts,
            root,
            library,
            config,
            endpoint,
            evidence,
            tracker,
            log_allow: Vec::new(),
            daemon: None,
            started_product: false,
            extra_env: Vec::new(),
        })
    }

    /// Adds a line of evidence to the report.
    pub fn note(&self, line: impl Into<String>) {
        if let Ok(mut e) = self.evidence.lock() {
            e.push(line.into());
        }
    }

    pub fn secs(&self, s: u64) -> Duration {
        Duration::from_secs_f64(s as f64 * self.timeout_scale)
    }

    /// A subfolder of the sandbox.
    pub fn dir(&self, name: &str) -> PathBuf {
        let d = self.root.join(name);
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// The environment every product program of the case gets: its own library, config folder, endpoint,
    /// and file-based secrets, so the maintainer's real data is never touched.
    pub fn apply_env(&self, cmd: &mut Command) {
        cmd.env("CAPTUREMANN_LIBRARY", &self.library)
            .env("CAPTUREMANN_CONFIG_DIR", &self.config)
            .env("CAPTUREMANN_SOCKET", &self.endpoint)
            .env("CAPTUREMANN_SECRETS", "file")
            .env_remove("CM_FFMPEG")
            .env_remove("CAPTUREMANN_MCP")
            .env_remove("CAPTUREMANN_DAEMON");
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
    }

    /// A command for a program of the package with the sandbox environment.
    pub fn product_cmd(&mut self, base: &str) -> Command {
        self.started_product = true;
        let mut c = Command::new(self.pkg.exe(base));
        self.apply_env(&mut c);
        sys::hide_console(&mut c);
        c
    }

    /// Runs a command of the package and waits.
    pub fn run(&mut self, cmd: Command, secs: u64) -> R<Output> {
        let t = self.secs(secs);
        proc::run(cmd, t, &self.tracker)
            .map_err(|e| Stop::Fail(format!("could not start the program: {e}")))
    }

    fn cli_inner(&mut self, args: &[&str], json: bool, secs: u64, no_start: bool) -> R<CliOut> {
        let mut c = self.product_cmd("capturemann");
        if json {
            c.arg("--json");
        }
        c.args(["--socket", &self.endpoint.clone()]);
        if no_start {
            c.arg("--no-start");
        }
        c.args(args);
        let out = self.run(c, secs)?;
        let parsed = serde_json::from_str::<Value>(out.stdout.trim()).ok();
        let error = if out.success() {
            None
        } else {
            parse_error(&out)
        };
        let error = error.or_else(|| parsed.as_ref().and_then(|v| v.get("error")).cloned());
        Ok(CliOut {
            out,
            json: parsed,
            error,
        })
    }

    /// `capturemann --json --socket <endpoint> --no-start <args>`; the daemon must be running.
    pub fn cli(&mut self, args: &[&str]) -> R<CliOut> {
        self.cli_inner(args, true, 60, true)
    }

    /// The same with a longer time limit (`--wait` commands).
    pub fn cli_long(&mut self, args: &[&str], secs: u64) -> R<CliOut> {
        self.cli_inner(args, true, secs, true)
    }

    /// Plain-text output (no `--json`).
    pub fn cli_text(&mut self, args: &[&str]) -> R<CliOut> {
        self.cli_inner(args, false, 60, true)
    }

    /// Like [`Ctx::cli`] but a failure ends the case with the command's error text.
    pub fn cli_ok(&mut self, args: &[&str]) -> R<Value> {
        let r = self.cli(args)?;
        if !r.ok() {
            return fail(format!(
                "`capturemann {}` failed ({}): {}",
                args.join(" "),
                proc::exit_text(r.out.code),
                crate::util::cut(&crate::util::one_line(&r.out.text()), 400)
            ));
        }
        r.json.ok_or_else(|| {
            Stop::Fail(format!(
                "`capturemann {}` printed no JSON: {}",
                args.join(" "),
                crate::util::cut(&r.out.stdout, 300)
            ))
        })
    }

    /// Settings the daemon reads at start (the app-only keys cannot be set over the bus): writes
    /// `settings.json` in the sandbox config folder. Call before [`Ctx::start_daemon`].
    pub fn write_settings(&self, settings: &Value) -> R {
        std::fs::write(
            self.config.join("settings.json"),
            serde_json::to_vec_pretty(settings)?,
        )?;
        Ok(())
    }

    /// Lets agents start recordings without an on-screen confirmation, the way the user can in
    /// Settings (OD-24, `agents.tool_policy`), because nobody is there to click.
    pub fn allow_recording_without_dialog(&self) -> R {
        self.write_settings(&serde_json::json!({
            "agents.tool_policy": { "start_recording": "allow" }
        }))
    }

    /// Starts `cm-daemon` on the sandbox endpoint and waits until it answers.
    pub fn start_daemon(&mut self) -> R<u32> {
        if self.daemon.is_some() {
            return fail("the case started a daemon twice");
        }
        let log_out = std::fs::File::create(self.root.join("daemon.out.txt"))?;
        let log_err = std::fs::File::create(self.root.join("daemon.err.txt"))?;
        let mut c = self.product_cmd("cm-daemon");
        c.arg("--library")
            .arg(&self.library)
            .args(["--socket", &self.endpoint.clone()]);
        c.stdin(Stdio::null())
            .stdout(Stdio::from(log_out))
            .stderr(Stdio::from(log_err));
        let child = c
            .spawn()
            .map_err(|e| Stop::Fail(format!("cm-daemon did not start: {e}")))?;
        let pid = child.id();
        if let Ok(mut t) = self.tracker.lock() {
            t.push(pid);
        }
        self.daemon = Some(Daemon { child, pid });
        let deadline = self.secs(45);
        let start = std::time::Instant::now();
        loop {
            if let Some(d) = self.daemon.as_mut() {
                if let Ok(Some(status)) = d.child.try_wait() {
                    let tail = self.daemon_log_tail();
                    self.daemon = None;
                    return fail(format!(
                        "cm-daemon exited at start ({}): {tail}",
                        proc::exit_text(status.code())
                    ));
                }
            }
            let r = self.cli_inner(&["daemon", "status"], true, 20, true)?;
            if r.ok()
                && r.json
                    .as_ref()
                    .and_then(|v| v.get("running"))
                    .and_then(Value::as_bool)
                    == Some(true)
            {
                self.note(format!(
                    "daemon pid {pid} answered after {} ms",
                    start.elapsed().as_millis()
                ));
                return Ok(pid);
            }
            if start.elapsed() > deadline {
                let tail = self.daemon_log_tail();
                self.kill_daemon();
                return fail(format!(
                    "cm-daemon did not answer within {} s: {tail}",
                    deadline.as_secs()
                ));
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }

    /// The end of the daemon's own output files, for failure messages.
    pub fn daemon_log_tail(&self) -> String {
        let mut s = String::new();
        for f in ["daemon.err.txt", "daemon.out.txt"] {
            if let Ok(t) = std::fs::read_to_string(self.root.join(f)) {
                let lines: Vec<&str> = t.lines().collect();
                let from = lines.len().saturating_sub(8);
                s.push_str(&lines[from..].join(" | "));
            }
        }
        crate::util::cut(&s, 600)
    }

    /// Stops the daemon: SIGTERM and a clean exit where the system has signals, a kill on Windows (a
    /// hidden process has no Ctrl+C). Returns whether the exit was graceful.
    pub fn stop_daemon(&mut self) -> R<bool> {
        let Some(mut d) = self.daemon.take() else {
            return Ok(true);
        };
        let graceful = if sys::PLATFORM == "windows" {
            sys::kill_tree(d.pid);
            false
        } else {
            let mut k = Command::new("kill");
            k.args(["-TERM", &d.pid.to_string()]);
            let _ = proc::run(k, Duration::from_secs(5), &self.tracker);
            let t = self.secs(20);
            let start = std::time::Instant::now();
            let mut done = false;
            while start.elapsed() < t {
                if matches!(d.child.try_wait(), Ok(Some(_))) {
                    done = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            if !done {
                sys::kill_tree(d.pid);
            }
            done
        };
        let _ = d.child.wait();
        if let Ok(mut t) = self.tracker.lock() {
            t.retain(|p| *p != d.pid);
        }
        Ok(graceful)
    }

    /// Kills the daemon the hard way (kill -9, taskkill /F) and forgets it.
    pub fn kill_daemon(&mut self) {
        if let Some(mut d) = self.daemon.take() {
            sys::kill_tree(d.pid);
            let _ = d.child.kill();
            let _ = d.child.wait();
            if let Ok(mut t) = self.tracker.lock() {
                t.retain(|p| *p != d.pid);
            }
        }
    }

    /// Whether the daemon answers.
    pub fn daemon_answers(&mut self) -> bool {
        self.cli_inner(&["daemon", "status"], true, 15, true)
            .map(|r| r.ok())
            .unwrap_or(false)
    }

    /// Every file below a folder (recursive), sorted.
    pub fn files_under(dir: &Path) -> Vec<PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(d) = stack.pop() {
            if let Ok(rd) = std::fs::read_dir(&d) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.is_dir() {
                        stack.push(p);
                    } else {
                        out.push(p);
                    }
                }
            }
        }
        out.sort();
        out
    }

    /// The bundles in the sandbox library with this extension (`cmrec`, `cmshot`), newest first.
    pub fn bundles(&self, ext: &str) -> Vec<PathBuf> {
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&self.library)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some(ext) && p.is_dir())
            .map(|p| {
                (
                    std::fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH),
                    p,
                )
            })
            .collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.0));
        v.into_iter().map(|(_, p)| p).collect()
    }

    /// Writes bytes into the sandbox and returns the path.
    pub fn write_file(&self, rel: &str, bytes: &[u8]) -> R<PathBuf> {
        let p = self.root.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&p, bytes)?;
        Ok(p)
    }

    /// The first display of `list_targets` that is primary: (id, width, height, origin x, origin y).
    pub fn primary_display(&mut self) -> R<Display> {
        let v = self.cli_ok(&["targets", "--displays-only"])?;
        let ds = displays(&v);
        check_nonempty(&ds)?;
        Ok(ds
            .iter()
            .find(|d| d.primary)
            .cloned()
            .unwrap_or_else(|| ds[0].clone()))
    }
}

fn check_nonempty(ds: &[Display]) -> R {
    if ds.is_empty() {
        return fail("`targets` lists no display (is this a desktop session?)");
    }
    Ok(())
}

/// A display of `list_targets`.
#[derive(Debug, Clone)]
pub struct Display {
    pub id: u64,
    pub name: String,
    pub w: i64,
    pub h: i64,
    pub x: i64,
    pub y: i64,
    pub scale: f64,
    pub primary: bool,
}

pub fn displays(v: &Value) -> Vec<Display> {
    v.get("displays")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|d| {
                    let size = d.pointer("/bounds/size").and_then(Value::as_array);
                    let num = |p: &str| d.pointer(p).and_then(Value::as_i64).unwrap_or(0);
                    Display {
                        id: d.get("id").and_then(Value::as_u64).unwrap_or(0),
                        name: d
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_owned(),
                        w: size
                            .and_then(|s| s.first())
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                        h: size
                            .and_then(|s| s.get(1))
                            .and_then(Value::as_i64)
                            .unwrap_or(0),
                        x: num("/bounds/origin/x"),
                        y: num("/bounds/origin/y"),
                        scale: d.get("scale").and_then(Value::as_f64).unwrap_or(0.0),
                        primary: d
                            .get("is_primary")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displays_parse_like_list_targets() {
        let v = serde_json::json!({"displays":[{"id":7,"name":"Display 1","bounds":{"origin":{"x":-1920,"y":0},"size":[1920,1080]},"scale":1.5,"is_primary":true}]});
        let d = displays(&v);
        assert_eq!(d.len(), 1);
        assert_eq!((d[0].id, d[0].w, d[0].h, d[0].x), (7, 1920, 1080, -1920));
        assert!(d[0].primary && d[0].scale == 1.5);
    }

    #[test]
    fn error_object_is_found_on_either_stream() {
        let o = Output {
            code: Some(2),
            stderr: "note\n{\"error\":{\"code\":\"not_found\",\"message\":\"x\"}}\n".into(),
            ..Output::default()
        };
        let e = parse_error(&o).unwrap();
        assert_eq!(e["code"], "not_found");
        let o = Output {
            code: Some(2),
            stdout: "{\"error\":{\"code\":\"invalid_input\"}}".into(),
            ..Output::default()
        };
        assert_eq!(parse_error(&o).unwrap()["code"], "invalid_input");
        assert!(parse_error(&Output::default()).is_none());
    }
}
