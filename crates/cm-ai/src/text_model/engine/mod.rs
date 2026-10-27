//! The built-in text engine (AI-7, provider `embedded`): llama.cpp's `llama-server` (MIT) run as a
//! **helper process**, like FFmpeg, and never linked into the app (whisper.cpp and llama.cpp both
//! embed ggml; two copies in one process clash).
//!
//! * [`EngineManager`] owns at most one helper. It starts lazily on the first request
//!   ([`TextEngine::acquire`]): find the helper, check the memory, pick a free loopback port, spawn
//!   `llama-server --host 127.0.0.1 --port <p> --model <gguf> --ctx-size <n> ...`, wait for
//!   `GET /health`. Later requests reuse it. After [`DEFAULT_IDLE_MS`] without a request
//!   ([`TextEngine::reap_idle`], called by a small thread) it is stopped to give the memory back.
//!   A helper that died is restarted once for the request that found it dead.
//! * The helper only ever listens on `127.0.0.1`. The client that talks to it is the ordinary
//!   OpenAI-compatible [`HttpTextModel`] as a `local` provider, so the loopback check of AI-7 still
//!   guards every request. The helper makes no network request of its own: it is given a local
//!   GGUF file and no URL, and `llama-server` has no telemetry.
//! * The helper dies with the app: it is stopped on drop and on [`TextEngine::stop`], and a
//!   watchdog in the shell that starts it ends it when this process disappears, even after a
//!   crash or a kill -9 (unix).
//! * Everything with an effect (finding the helper, ports, processes, health, memory, time) is
//!   behind [`Platform`], so the tests run a scripted fake and never start `llama-server`.
//!
//! Errors are [`TextModelError`]s: [`ErrorKind::EngineMissing`] (`job_failed`, reason
//! `engine_missing`), [`ErrorKind::NotEnoughMemory`] (`not_enough_memory`) and
//! [`ErrorKind::EngineFailed`] (`engine_failed`).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use cm_media::tools::in_app_bundle;
use serde::Serialize;

use super::{
    ErrorKind, JsonReply, JsonRequest, Provider, TextModel, TextModelConfig, TextModelError,
};

/// Environment variable naming the helper executable.
pub const ENV_LLAMA_SERVER: &str = "CM_LLAMA_SERVER";
/// The helper's file name.
pub const HELPER_NAME: &str = "llama-server";
/// The helper is stopped after this long without a request (10 minutes).
pub const DEFAULT_IDLE_MS: u64 = 10 * 60 * 1000;
/// How long the helper may take to load the model and answer `/health` (large models from a slow
/// disk take a while).
pub const DEFAULT_START_TIMEOUT_MS: u64 = 180_000;
/// Milliseconds between two `/health` polls.
const POLL_MS: u64 = 250;
/// How often the reaper thread looks.
const REAP_EVERY_MS: u64 = 15_000;

/// What to run: the model file and its limits (from the catalogue and the settings).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSpec {
    pub model_id: String,
    pub model_path: PathBuf,
    /// Memory the model needs while it runs, in MB (0: unknown, no check).
    pub ram_mb: u32,
    /// `--ctx-size`.
    pub context_tokens: u32,
}

/// The helper's command line for `spec` on `port`.
pub fn engine_args(spec: &EngineSpec, port: u16) -> Vec<String> {
    vec![
        "--host".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--model".into(),
        spec.model_path.display().to_string(),
        "--alias".into(),
        spec.model_id.clone(),
        "--ctx-size".into(),
        spec.context_tokens.to_string(),
        // Metal: all layers on the GPU (unified memory).
        "--n-gpu-layers".into(),
        "999".into(),
        // One request at a time: the whole context belongs to it.
        "--parallel".into(),
        "1".into(),
        "--no-webui".into(),
        // No thinking: a reasoning model (Qwen3.5) otherwise reasons without end at temperature 0
        // and spends the whole token budget before the JSON starts (seen on Oct 27, 2026: two
        // 4096-token requests, no report). Notes need the answer, not the reasoning.
        "--reasoning".into(),
        "off".into(),
    ]
}

/// The state the AI page and `get_text_model` show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineState {
    /// Nothing is running (never started, stopped when idle, stopped by the user).
    Stopped,
    /// Loading a model.
    Starting,
    Running,
    /// The last start failed or the helper died twice.
    Failed,
}

/// A snapshot of the engine.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineStatus {
    pub state: EngineState,
    pub model: Option<String>,
    pub port: Option<u16>,
    /// Requests being served right now.
    pub in_flight: u32,
    /// How many times a dead helper was started again.
    pub restarts: u32,
    /// Where the helper is, when it was found.
    pub helper: Option<String>,
    /// After a failure: the reason code and the message.
    pub reason: Option<String>,
    pub error: Option<String>,
    /// The helper is stopped after this many seconds without a request.
    pub idle_stop_s: u64,
}

/// The engine the embedded provider talks to.
pub trait TextEngine: Send + Sync {
    /// Makes sure the helper runs `spec` and returns its OpenAI-compatible base URL
    /// (`http://127.0.0.1:<port>/v1`). Counts a request in flight: pair it with
    /// [`release`](Self::release). Blocking (it waits for the model to load).
    fn acquire(&self, spec: &EngineSpec) -> Result<String, TextModelError>;
    /// A request ended (whatever the outcome).
    fn release(&self);
    fn status(&self) -> EngineStatus;
    /// Stops the helper now (no-op when stopped).
    fn stop(&self);
    /// Stops the helper when it has been idle long enough; returns whether it did.
    fn reap_idle(&self) -> bool;
}

/// Whether the helper answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Ready,
    /// Up but still loading the model.
    Loading,
    Down,
}

/// A started helper process.
pub trait ChildHandle: Send {
    /// `Some(description)` when the process has ended.
    fn exited(&mut self) -> Option<String>;
    /// Stops it (politely first) and waits.
    fn kill(&mut self);
    /// The end of the helper's log, for error messages.
    fn log_tail(&self) -> String;
}

/// Every effect of the manager.
pub trait Platform: Send + Sync {
    fn locate_helper(&self) -> Result<PathBuf, TextModelError>;
    fn free_port(&self) -> Result<u16, TextModelError>;
    fn spawn(&self, helper: &Path, args: &[String])
        -> Result<Box<dyn ChildHandle>, TextModelError>;
    fn health(&self, port: u16) -> Health;
    /// Memory that can be used now, in MB (`None`: unknown, the guard is skipped).
    fn available_ram_mb(&self) -> Option<u64>;
    fn now_ms(&self) -> u64;
    fn sleep_ms(&self, ms: u64);
}

struct Running {
    child: Box<dyn ChildHandle>,
    port: u16,
    spec: EngineSpec,
}

struct Inner {
    running: Option<Running>,
    state: EngineState,
    in_flight: u32,
    last_used_ms: u64,
    restarts: u32,
    helper: Option<PathBuf>,
    failure: Option<(String, String)>,
}

/// Starts, reuses and stops the helper.
pub struct EngineManager {
    platform: Arc<dyn Platform>,
    inner: Mutex<Inner>,
    idle_ms: u64,
    start_timeout_ms: u64,
}

fn engine_error(kind: ErrorKind, message: impl Into<String>) -> TextModelError {
    TextModelError {
        kind,
        message: message.into(),
    }
}

impl EngineManager {
    pub fn new(platform: Arc<dyn Platform>) -> EngineManager {
        EngineManager {
            platform,
            inner: Mutex::new(Inner {
                running: None,
                state: EngineState::Stopped,
                in_flight: 0,
                last_used_ms: 0,
                restarts: 0,
                helper: None,
                failure: None,
            }),
            idle_ms: DEFAULT_IDLE_MS,
            start_timeout_ms: DEFAULT_START_TIMEOUT_MS,
        }
    }

    /// The real engine, with the thread that stops it when idle. `log_path` receives the helper's
    /// output (replaced at every start).
    pub fn system(log_path: PathBuf) -> Arc<EngineManager> {
        let m = Arc::new(EngineManager::new(Arc::new(SystemPlatform { log_path })));
        EngineManager::spawn_reaper(&m);
        m
    }

    pub fn with_idle_ms(mut self, ms: u64) -> Self {
        self.idle_ms = ms;
        self
    }

    pub fn with_start_timeout_ms(mut self, ms: u64) -> Self {
        self.start_timeout_ms = ms;
        self
    }

    fn spawn_reaper(m: &Arc<EngineManager>) {
        let weak: Weak<EngineManager> = Arc::downgrade(m);
        let _ = std::thread::Builder::new()
            .name("text-engine-reaper".into())
            .spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(REAP_EVERY_MS));
                let Some(m) = weak.upgrade() else { return };
                m.reap_idle();
            });
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn stop_locked(g: &mut Inner) {
        if let Some(mut r) = g.running.take() {
            r.child.kill();
        }
        g.state = EngineState::Stopped;
    }

    fn fail(g: &mut Inner, e: TextModelError) -> TextModelError {
        g.failure = Some((e.reason().to_owned(), e.message.clone()));
        g.state = EngineState::Failed;
        e
    }

    /// Starts the helper for `spec` (the lock is held: other requests wait and then reuse it).
    fn start(&self, g: &mut Inner, spec: &EngineSpec) -> Result<(), TextModelError> {
        let helper = self.platform.locate_helper()?;
        g.helper = Some(helper.clone());
        if spec.ram_mb > 0 {
            if let Some(free) = self.platform.available_ram_mb() {
                if u64::from(spec.ram_mb) > free {
                    return Err(engine_error(
                        ErrorKind::NotEnoughMemory,
                        format!(
                            "The text model `{}` needs about {:.1} GB of free memory and this Mac has {:.1} GB \
                             free. Close other apps, or choose a smaller model on the AI page.",
                            spec.model_id,
                            f64::from(spec.ram_mb) / 1000.0,
                            free as f64 / 1000.0
                        ),
                    ));
                }
            }
        }
        if !spec.model_path.is_file() {
            return Err(TextModelError::no_text_model_named(&spec.model_id));
        }
        let port = self.platform.free_port()?;
        g.state = EngineState::Starting;
        let mut child = self.platform.spawn(&helper, &engine_args(spec, port))?;
        let began = self.platform.now_ms();
        loop {
            if let Some(how) = child.exited() {
                let tail = child.log_tail();
                return Err(engine_error(
                    ErrorKind::EngineFailed,
                    format!(
                        "The text engine stopped while loading `{}` ({how}).{}",
                        spec.model_id,
                        if tail.is_empty() {
                            String::new()
                        } else {
                            format!(" {tail}")
                        }
                    ),
                ));
            }
            match self.platform.health(port) {
                Health::Ready => break,
                Health::Loading | Health::Down => {}
            }
            if self.platform.now_ms().saturating_sub(began) >= self.start_timeout_ms {
                let tail = child.log_tail();
                child.kill();
                return Err(engine_error(
                    ErrorKind::EngineFailed,
                    format!(
                        "The text engine did not finish loading `{}` in {} s.{}",
                        spec.model_id,
                        self.start_timeout_ms / 1000,
                        if tail.is_empty() {
                            String::new()
                        } else {
                            format!(" {tail}")
                        }
                    ),
                ));
            }
            self.platform.sleep_ms(POLL_MS);
        }
        g.running = Some(Running {
            child,
            port,
            spec: spec.clone(),
        });
        g.state = EngineState::Running;
        g.failure = None;
        Ok(())
    }
}

impl TextEngine for EngineManager {
    fn acquire(&self, spec: &EngineSpec) -> Result<String, TextModelError> {
        let mut g = self.lock();
        // A helper that died since the last request: start it again, once.
        let mut crashed = false;
        if let Some(r) = g.running.as_mut() {
            if r.child.exited().is_some() {
                crashed = true;
            }
        }
        if crashed {
            Self::stop_locked(&mut g);
            g.restarts += 1;
        }
        // Another model asked for: only when nothing is being served.
        if let Some(r) = &g.running {
            if r.spec != *spec {
                if g.in_flight > 0 {
                    return Err(engine_error(
                        ErrorKind::EngineFailed,
                        "The text engine is busy with another model. Try again when it has finished.",
                    ));
                }
                Self::stop_locked(&mut g);
            }
        }
        if g.running.is_none() {
            if let Err(e) = self.start(&mut g, spec) {
                // Make sure nothing half-started is left behind.
                Self::stop_locked(&mut g);
                return Err(Self::fail(&mut g, e));
            }
        }
        g.in_flight += 1;
        g.last_used_ms = self.platform.now_ms();
        let port = g.running.as_ref().map_or(0, |r| r.port);
        Ok(format!("http://127.0.0.1:{port}/v1"))
    }

    fn release(&self) {
        let mut g = self.lock();
        g.in_flight = g.in_flight.saturating_sub(1);
        g.last_used_ms = self.platform.now_ms();
    }

    fn status(&self) -> EngineStatus {
        let mut g = self.lock();
        // A helper that died on its own shows as stopped, not running.
        let mut dead = false;
        if let Some(r) = g.running.as_mut() {
            dead = r.child.exited().is_some();
        }
        if dead {
            Self::stop_locked(&mut g);
        }
        let (reason, error) = g.failure.clone().unzip();
        EngineStatus {
            state: g.state,
            model: g.running.as_ref().map(|r| r.spec.model_id.clone()),
            port: g.running.as_ref().map(|r| r.port),
            in_flight: g.in_flight,
            restarts: g.restarts,
            helper: g.helper.as_ref().map(|p| p.display().to_string()),
            reason,
            error,
            idle_stop_s: self.idle_ms / 1000,
        }
    }

    fn stop(&self) {
        let mut g = self.lock();
        Self::stop_locked(&mut g);
        g.failure = None;
    }

    fn reap_idle(&self) -> bool {
        let mut g = self.lock();
        let idle = g.running.is_some()
            && g.in_flight == 0
            && self.platform.now_ms().saturating_sub(g.last_used_ms) >= self.idle_ms;
        if idle {
            Self::stop_locked(&mut g);
        }
        idle
    }
}

impl Drop for EngineManager {
    fn drop(&mut self) {
        let g = self.inner.get_mut().unwrap_or_else(PoisonError::into_inner);
        Self::stop_locked(g);
    }
}

/// The `embedded` provider: asks the engine for the helper's address and talks to it with the
/// ordinary OpenAI-compatible client, as a `local` provider (loopback only).
pub struct EmbeddedTextModel {
    engine: Arc<dyn TextEngine>,
    spec: EngineSpec,
    timeout_s: u64,
}

impl EmbeddedTextModel {
    pub fn new(engine: Arc<dyn TextEngine>, spec: EngineSpec, config: &TextModelConfig) -> Self {
        EmbeddedTextModel {
            engine,
            spec,
            timeout_s: config.timeout_s,
        }
    }

    fn once(
        &self,
        req: &JsonRequest,
        streaming: bool,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        let base = self.engine.acquire(&self.spec)?;
        let cfg = TextModelConfig {
            provider: Provider::Local,
            base_url: base,
            model: self.spec.model_id.clone(),
            context_tokens: self.spec.context_tokens,
            timeout_s: self.timeout_s,
            ..TextModelConfig::default()
        };
        let http = super::HttpTextModel::new(cfg, None);
        let r = if streaming {
            http.complete_json_streaming(req, on_progress)
        } else {
            http.complete_json(req)
        };
        self.engine.release();
        r
    }
}

impl TextModel for EmbeddedTextModel {
    fn complete_json(&self, req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        self.run(req, None)
    }

    fn complete_json_streaming(
        &self,
        req: &JsonRequest,
        on_progress: &mut dyn FnMut(usize) -> bool,
    ) -> Result<JsonReply, TextModelError> {
        self.run(req, Some(on_progress))
    }
}

impl EmbeddedTextModel {
    fn run(
        &self,
        req: &JsonRequest,
        on_progress: Option<&mut dyn FnMut(usize) -> bool>,
    ) -> Result<JsonReply, TextModelError> {
        let mut noop = |_: usize| true;
        let (streaming, hook): (bool, &mut dyn FnMut(usize) -> bool) = match on_progress {
            Some(f) => (true, f),
            None => (false, &mut noop),
        };
        match self.once(req, streaming, &mut *hook) {
            // The helper stopped answering mid-way: acquire notices it is dead, starts it again
            // (once) and the request runs again.
            Err(e) if e.kind == ErrorKind::Unreachable => {
                self.once(req, streaming, hook).map_err(|e2| {
                    if e2.kind == ErrorKind::Unreachable {
                        engine_error(
                            ErrorKind::EngineFailed,
                            "The built-in text engine stopped answering. Try again; if it keeps \
                         happening, restart CaptureMann.",
                        )
                    } else {
                        e2
                    }
                })
            }
            other => other,
        }
    }
}

/// What `build` returns for `embedded`: the process has no engine wired in.
pub struct Unwired;

impl TextModel for Unwired {
    fn complete_json(&self, _req: &JsonRequest) -> Result<JsonReply, TextModelError> {
        Err(engine_error(
            ErrorKind::EngineMissing,
            "The built-in text engine is not available in this program.",
        ))
    }
}

// ---- the real platform ---------------------------------------------------------------------

fn exe_name() -> OsString {
    if cfg!(windows) {
        format!("{HELPER_NAME}.exe").into()
    } else {
        HELPER_NAME.into()
    }
}

/// Where each lookup step looks; injectable so the order is unit tested.
#[derive(Debug, Clone, Default)]
pub struct SearchPaths {
    /// Value of `CM_LLAMA_SERVER` (a file, or a folder holding `llama-server`).
    pub env: Option<OsString>,
    /// Folder of the running executable (`Contents/MacOS` in the app bundle).
    pub exe_dir: Option<PathBuf>,
    /// Value of `PATH`.
    pub path_var: Option<OsString>,
}

impl SearchPaths {
    pub fn from_env() -> SearchPaths {
        SearchPaths {
            env: std::env::var_os(ENV_LLAMA_SERVER).filter(|v| !v.is_empty()),
            exe_dir: std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(Path::to_path_buf)),
            path_var: std::env::var_os("PATH"),
        }
    }
}

/// Finds the helper: `CM_LLAMA_SERVER`, next to the executable (the bundled copy), then `PATH`.
/// Errors with [`ErrorKind::EngineMissing`].
pub fn locate_helper(paths: &SearchPaths) -> Result<PathBuf, TextModelError> {
    let name = exe_name();
    if let Some(v) = &paths.env {
        let p = PathBuf::from(v);
        let candidate = if p.is_dir() { p.join(&name) } else { p };
        return if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(engine_error(
                ErrorKind::EngineMissing,
                format!(
                    "The text engine was not found: {ENV_LLAMA_SERVER} points to `{}`, which is not a \
                     file. Fix or unset {ENV_LLAMA_SERVER}.",
                    candidate.display()
                ),
            ))
        };
    }
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = &paths.exe_dir {
        dirs.push(d.clone());
    }
    if let Some(p) = &paths.path_var {
        dirs.extend(std::env::split_paths(p));
    }
    if let Some(found) = dirs.iter().map(|d| d.join(&name)).find(|p| p.is_file()) {
        return Ok(found);
    }
    // A packaged app that lost its helper is damaged: say so. A developer build gets the hint.
    let message = if paths.exe_dir.as_deref().is_some_and(in_app_bundle) {
        "The text engine was not found: the copy that ships inside CaptureMann is missing, so the \
         app is damaged or incomplete. Reinstall CaptureMann."
            .to_owned()
    } else {
        format!(
            "The text engine was not found: run `make llama` to build it, or set {ENV_LLAMA_SERVER} \
             to a llama-server executable. CaptureMann releases include their own copy."
        )
    };
    Err(engine_error(ErrorKind::EngineMissing, message))
}

/// The real effects.
pub struct SystemPlatform {
    log_path: PathBuf,
}

impl SystemPlatform {
    pub fn new(log_path: PathBuf) -> SystemPlatform {
        SystemPlatform { log_path }
    }
}

struct SystemChild {
    child: std::process::Child,
    log_path: PathBuf,
}

impl ChildHandle for SystemChild {
    fn exited(&mut self) -> Option<String> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => None,
            Err(e) => Some(format!("cannot check it: {e}")),
        }
    }

    fn kill(&mut self) {
        let pid = self.child.id().to_string();
        // SIGTERM first: the shell that started the helper passes it on. Then the hard way.
        let _ = std::process::Command::new("kill")
            .args(["-TERM", &pid])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        for _ in 0..30 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        let _ = std::process::Command::new("pkill")
            .args(["-KILL", "-P", &pid])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn log_tail(&self) -> String {
        let Ok(bytes) = std::fs::read(&self.log_path) else {
            return String::new();
        };
        let tail = &bytes[bytes.len().saturating_sub(400)..];
        let text = String::from_utf8_lossy(tail);
        // The last non-empty line says the most.
        text.lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .map(|l| l.trim().chars().take(240).collect())
            .unwrap_or_default()
    }
}

/// Runs the helper under a shell that ends it when this process goes away (even on kill -9) and
/// passes SIGTERM on to it.
#[cfg(unix)]
const WATCHDOG: &str = r#"parent=$1; shift
"$@" &
child=$!
( while kill -0 "$parent" 2>/dev/null && kill -0 "$child" 2>/dev/null; do sleep 2; done; kill "$child" 2>/dev/null ) &
watch=$!
trap 'kill "$child" 2>/dev/null; kill "$watch" 2>/dev/null; wait; exit 0' TERM INT HUP
wait "$child"
code=$?
kill "$watch" 2>/dev/null
exit $code"#;

impl Platform for SystemPlatform {
    fn locate_helper(&self) -> Result<PathBuf, TextModelError> {
        locate_helper(&SearchPaths::from_env())
    }

    fn free_port(&self) -> Result<u16, TextModelError> {
        std::net::TcpListener::bind(("127.0.0.1", 0))
            .and_then(|l| l.local_addr())
            .map(|a| a.port())
            .map_err(|e| {
                engine_error(
                    ErrorKind::EngineFailed,
                    format!("No free port on 127.0.0.1 for the text engine: {e}"),
                )
            })
    }

    fn spawn(
        &self,
        helper: &Path,
        args: &[String],
    ) -> Result<Box<dyn ChildHandle>, TextModelError> {
        if let Some(dir) = self.log_path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let log = std::fs::File::create(&self.log_path).map_err(|e| {
            engine_error(
                ErrorKind::EngineFailed,
                format!("Cannot write the text engine log: {e}"),
            )
        })?;
        let log2 = log
            .try_clone()
            .map_err(|e| engine_error(ErrorKind::EngineFailed, e.to_string()))?;
        #[cfg(unix)]
        let mut cmd = {
            let mut c = std::process::Command::new("/bin/sh");
            c.arg("-c")
                .arg(WATCHDOG)
                .arg("llama-watchdog")
                .arg(std::process::id().to_string())
                .arg(helper)
                .args(args);
            c
        };
        #[cfg(not(unix))]
        let mut cmd = {
            let mut c = std::process::Command::new(helper);
            c.args(args);
            c
        };
        let child = cmd
            .stdin(std::process::Stdio::null())
            .stdout(log)
            .stderr(log2)
            .spawn()
            .map_err(|e| {
                engine_error(
                    ErrorKind::EngineFailed,
                    format!("Cannot start the text engine `{}`: {e}", helper.display()),
                )
            })?;
        Ok(Box::new(SystemChild {
            child,
            log_path: self.log_path.clone(),
        }))
    }

    fn health(&self, port: u16) -> Health {
        super::http::probe_health(port)
    }

    fn available_ram_mb(&self) -> Option<u64> {
        available_ram_mb()
    }

    fn now_ms(&self) -> u64 {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_millis() as u64
    }

    fn sleep_ms(&self, ms: u64) {
        std::thread::sleep(std::time::Duration::from_millis(ms));
    }
}

/// Memory that can be used now, in MB: free, inactive and speculative pages on macOS
/// (`vm_stat`), `MemAvailable` on Linux. `None` when the OS cannot say.
pub fn available_ram_mb() -> Option<u64> {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("vm_stat").output().ok()?;
        parse_vm_stat(&String::from_utf8_lossy(&out.stdout))
    }
    #[cfg(target_os = "linux")]
    {
        let text = std::fs::read_to_string("/proc/meminfo").ok()?;
        let kb: u64 = text
            .lines()
            .find_map(|l| l.strip_prefix("MemAvailable:"))?
            .split_whitespace()
            .next()?
            .parse()
            .ok()?;
        Some(kb / 1000)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        None
    }
}

/// Reads `vm_stat` output (`Mach Virtual Memory Statistics: (page size of 16384 bytes)` and
/// `Pages free: 1234.` lines) into available MB.
pub fn parse_vm_stat(text: &str) -> Option<u64> {
    let page: u64 = text
        .lines()
        .next()?
        .split("page size of")
        .nth(1)?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let pages = |key: &str| -> u64 {
        text.lines()
            .find_map(|l| l.strip_prefix(key))
            .and_then(|v| v.trim().trim_end_matches('.').parse().ok())
            .unwrap_or(0)
    };
    let usable = pages("Pages free:") + pages("Pages inactive:") + pages("Pages speculative:");
    Some(usable * page / 1_000_000)
}

#[cfg(test)]
mod tests;
