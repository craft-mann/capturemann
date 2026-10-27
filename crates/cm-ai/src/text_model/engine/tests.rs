#![allow(clippy::unwrap_used)]

use std::collections::{HashMap, VecDeque};

use super::*;

/// A scripted platform: no process, no port, no clock. Time only moves when the manager sleeps.
struct Fake {
    st: Mutex<State>,
}

struct State {
    now: u64,
    next_port: u16,
    helper: Result<PathBuf, ErrorKind>,
    health: VecDeque<Health>,
    ram: Option<u64>,
    spawns: Vec<Vec<String>>,
    alive: HashMap<u16, bool>,
    killed: Vec<u16>,
    die_on_spawn: bool,
}

impl Fake {
    fn new() -> Arc<Fake> {
        Arc::new(Fake {
            st: Mutex::new(State {
                now: 1_000,
                next_port: 50_000,
                helper: Ok(PathBuf::from("/fake/llama-server")),
                health: VecDeque::new(),
                ram: Some(64_000),
                spawns: Vec::new(),
                alive: HashMap::new(),
                killed: Vec::new(),
                die_on_spawn: false,
            }),
        })
    }

    fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.st.lock().unwrap())
    }

    fn crash(&self, port: u16) {
        self.with(|s| {
            s.alive.insert(port, false);
        });
    }
}

struct FakeChild {
    fake: Arc<Fake>,
    port: u16,
}

impl ChildHandle for FakeChild {
    fn exited(&mut self) -> Option<String> {
        self.fake
            .with(|s| (!s.alive[&self.port]).then(|| "exit status: 1".to_owned()))
    }
    fn kill(&mut self) {
        self.fake.with(|s| {
            s.alive.insert(self.port, false);
            s.killed.push(self.port);
        });
    }
    fn log_tail(&self) -> String {
        "error: failed to load model".into()
    }
}

struct Handle(Arc<Fake>);

impl Platform for Handle {
    fn locate_helper(&self) -> Result<PathBuf, TextModelError> {
        self.0.with(|s| {
            s.helper
                .clone()
                .map_err(|k| engine_error(k, "The text engine was not found"))
        })
    }
    fn free_port(&self) -> Result<u16, TextModelError> {
        Ok(self.0.with(|s| {
            s.next_port += 1;
            s.next_port
        }))
    }
    fn spawn(&self, _: &Path, args: &[String]) -> Result<Box<dyn ChildHandle>, TextModelError> {
        let port: u16 = args[args.iter().position(|a| a == "--port").unwrap() + 1]
            .parse()
            .unwrap();
        self.0.with(|s| {
            s.spawns.push(args.to_vec());
            let up = !s.die_on_spawn;
            s.alive.insert(port, up);
        });
        Ok(Box::new(FakeChild {
            fake: self.0.clone(),
            port,
        }))
    }
    fn health(&self, _: u16) -> Health {
        self.0
            .with(|s| s.health.pop_front().unwrap_or(Health::Ready))
    }
    fn available_ram_mb(&self) -> Option<u64> {
        self.0.with(|s| s.ram)
    }
    fn now_ms(&self) -> u64 {
        self.0.with(|s| s.now)
    }
    fn sleep_ms(&self, ms: u64) {
        self.0.with(|s| s.now += ms);
    }
}

fn manager(fake: &Arc<Fake>) -> EngineManager {
    EngineManager::new(Arc::new(Handle(fake.clone())))
        .with_idle_ms(60_000)
        .with_start_timeout_ms(5_000)
}

/// A model file that exists, so the "not downloaded" check passes.
fn spec(dir: &tempfile::TempDir, id: &str) -> EngineSpec {
    let p = dir.path().join(format!("{id}.gguf"));
    std::fs::write(&p, b"GGUF").unwrap();
    EngineSpec {
        model_id: id.into(),
        model_path: p,
        ram_mb: 6_000,
        context_tokens: 8192,
    }
}

#[test]
fn starts_lazily_on_loopback_and_is_reused() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let s = spec(&dir, "m1");
    assert_eq!(m.status().state, EngineState::Stopped);
    assert!(
        fake.with(|s| s.spawns.is_empty()),
        "nothing starts by itself"
    );

    let url = m.acquire(&s).unwrap();
    m.release();
    assert_eq!(url, "http://127.0.0.1:50001/v1");
    let args = fake.with(|st| st.spawns[0].clone());
    let at = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!(at("--host"), "127.0.0.1");
    assert_eq!(at("--port"), "50001");
    assert_eq!(at("--ctx-size"), "8192");
    assert_eq!(at("--model"), s.model_path.display().to_string());
    assert!(args.iter().any(|a| a == "--n-gpu-layers"), "Metal offload");
    assert!(
        !args.iter().any(|a| a.contains("http")),
        "the helper gets no URL to reach"
    );

    // The second request reuses the process.
    assert_eq!(m.acquire(&s).unwrap(), url);
    m.release();
    assert_eq!(fake.with(|st| st.spawns.len()), 1);
    let st = m.status();
    assert_eq!(st.state, EngineState::Running);
    assert_eq!(st.model.as_deref(), Some("m1"));
    assert_eq!(st.port, Some(50001));
}

#[test]
fn waits_for_the_model_to_load() {
    let fake = Fake::new();
    fake.with(|s| s.health = VecDeque::from([Health::Down, Health::Loading, Health::Loading]));
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let before = fake.with(|s| s.now);
    m.acquire(&spec(&dir, "m1")).unwrap();
    assert!(fake.with(|s| s.now) - before >= 3 * POLL_MS);
    assert_eq!(m.status().state, EngineState::Running);
}

#[test]
fn a_model_that_never_loads_times_out_and_is_stopped() {
    let fake = Fake::new();
    fake.with(|s| s.health = std::iter::repeat_n(Health::Loading, 1000).collect());
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let e = m.acquire(&spec(&dir, "m1")).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineFailed);
    assert_eq!(e.reason(), "engine_failed");
    assert!(e.message.contains("did not finish loading"), "{e}");
    assert!(e.message.contains("failed to load model"), "log tail: {e}");
    assert_eq!(fake.with(|s| s.killed.len()), 1);
    let st = m.status();
    assert_eq!(st.state, EngineState::Failed);
    assert_eq!(st.reason.as_deref(), Some("engine_failed"));
}

#[test]
fn a_helper_that_exits_while_loading_is_a_clear_failure() {
    let fake = Fake::new();
    fake.with(|s| s.die_on_spawn = true);
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let e = m.acquire(&spec(&dir, "m1")).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineFailed);
    assert!(e.message.contains("stopped while loading"), "{e}");
    // The next request tries again from scratch.
    fake.with(|s| s.die_on_spawn = false);
    m.acquire(&spec(&dir, "m1")).unwrap();
    assert_eq!(m.status().state, EngineState::Running);
    assert!(m.status().error.is_none());
}

#[test]
fn a_missing_helper_is_engine_missing_and_nothing_is_spawned() {
    let fake = Fake::new();
    fake.with(|s| s.helper = Err(ErrorKind::EngineMissing));
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let e = m.acquire(&spec(&dir, "m1")).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineMissing);
    assert_eq!(e.reason(), "engine_missing");
    assert!(fake.with(|s| s.spawns.is_empty()));
}

#[test]
fn a_model_file_that_is_gone_is_no_text_model() {
    let fake = Fake::new();
    let m = manager(&fake);
    let e = m
        .acquire(&EngineSpec {
            model_id: "gone".into(),
            model_path: PathBuf::from("/nonexistent/gone.gguf"),
            ram_mb: 0,
            context_tokens: 4096,
        })
        .unwrap_err();
    assert_eq!(e.kind, ErrorKind::NoTextModel);
    assert!(fake.with(|s| s.spawns.is_empty()));
}

#[test]
fn the_memory_guard_refuses_a_model_that_does_not_fit() {
    let fake = Fake::new();
    fake.with(|s| s.ram = Some(5_000));
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let e = m.acquire(&spec(&dir, "m1")).unwrap_err();
    assert_eq!(e.kind, ErrorKind::NotEnoughMemory);
    assert_eq!(e.reason(), "not_enough_memory");
    assert!(
        e.message.contains("6.0 GB") && e.message.contains("5.0 GB"),
        "{e}"
    );
    assert!(fake.with(|s| s.spawns.is_empty()));
    // Unknown free memory does not block; enough memory does not either.
    fake.with(|s| s.ram = None);
    m.acquire(&spec(&dir, "m1")).unwrap();
}

#[test]
fn stops_after_the_idle_time_but_never_while_a_request_runs() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    m.acquire(&spec(&dir, "m1")).unwrap();
    fake.with(|s| s.now += 3_600_000);
    assert!(!m.reap_idle(), "a request is in flight");
    assert_eq!(m.status().state, EngineState::Running);
    m.release();
    assert!(!m.reap_idle(), "just used");
    fake.with(|s| s.now += 59_000);
    assert!(!m.reap_idle());
    fake.with(|s| s.now += 2_000);
    assert!(m.reap_idle());
    assert_eq!(m.status().state, EngineState::Stopped);
    assert_eq!(fake.with(|s| s.killed.clone()), vec![50_001]);
    // The next request starts it again, on a new port.
    let url = m.acquire(&spec(&dir, "m1")).unwrap();
    assert_eq!(url, "http://127.0.0.1:50002/v1");
}

#[test]
fn a_crashed_helper_is_started_again_once_for_the_request_that_finds_it() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let s = spec(&dir, "m1");
    m.acquire(&s).unwrap();
    m.release();
    fake.crash(50_001);
    assert_eq!(
        m.status().state,
        EngineState::Stopped,
        "a dead helper never shows as running"
    );
    let url = m.acquire(&s).unwrap();
    assert_eq!(url, "http://127.0.0.1:50002/v1");
    assert_eq!(m.status().restarts, 0, "status() already saw it stopped");

    // A crash found by acquire itself counts as a restart.
    m.release();
    fake.crash(50_002);
    let url = m.acquire(&s).unwrap();
    assert_eq!(url, "http://127.0.0.1:50003/v1");
    assert_eq!(m.status().restarts, 1);
}

#[test]
fn another_model_restarts_the_helper_unless_it_is_busy() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = (spec(&dir, "a"), spec(&dir, "b"));
    m.acquire(&a).unwrap();
    let e = m.acquire(&b).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineFailed);
    assert!(e.message.contains("busy"), "{e}");
    assert_eq!(
        m.status().model.as_deref(),
        Some("a"),
        "the running one is untouched"
    );
    m.release();
    m.acquire(&b).unwrap();
    assert_eq!(m.status().model.as_deref(), Some("b"));
    assert_eq!(fake.with(|s| s.killed.len()), 1);
}

#[test]
fn stop_and_drop_end_the_helper() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    m.acquire(&spec(&dir, "m1")).unwrap();
    m.release();
    m.stop();
    assert_eq!(m.status().state, EngineState::Stopped);
    assert_eq!(fake.with(|s| s.killed.len()), 1);
    m.acquire(&spec(&dir, "m1")).unwrap();
    drop(m);
    assert_eq!(
        fake.with(|s| s.killed.len()),
        2,
        "dropping the engine stops the helper"
    );
}

#[test]
fn the_helper_is_looked_for_in_the_documented_order() {
    let dir = tempfile::tempdir().unwrap();
    let exe_dir = dir.path().join("exe");
    let path_dir = dir.path().join("path");
    let env_dir = dir.path().join("env");
    for d in [&exe_dir, &path_dir, &env_dir] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join(HELPER_NAME), b"x").unwrap();
    }
    let paths = |env: Option<&Path>| SearchPaths {
        env: env.map(|p| p.as_os_str().to_owned()),
        exe_dir: Some(exe_dir.clone()),
        path_var: Some(std::env::join_paths([&path_dir]).unwrap()),
    };
    // CM_LLAMA_SERVER wins (a folder or the file), then next to the executable, then PATH.
    assert_eq!(
        locate_helper(&paths(Some(&env_dir))).unwrap(),
        env_dir.join(HELPER_NAME)
    );
    assert_eq!(
        locate_helper(&paths(Some(&env_dir.join(HELPER_NAME)))).unwrap(),
        env_dir.join(HELPER_NAME)
    );
    assert_eq!(
        locate_helper(&paths(None)).unwrap(),
        exe_dir.join(HELPER_NAME)
    );
    std::fs::remove_file(exe_dir.join(HELPER_NAME)).unwrap();
    assert_eq!(
        locate_helper(&paths(None)).unwrap(),
        path_dir.join(HELPER_NAME)
    );
    std::fs::remove_file(path_dir.join(HELPER_NAME)).unwrap();
    let e = locate_helper(&paths(None)).unwrap_err();
    assert_eq!(e.kind, ErrorKind::EngineMissing);
    assert!(e.message.contains("make llama"), "{e}");
    // A bad CM_LLAMA_SERVER is named, not silently skipped.
    let e = locate_helper(&paths(Some(&dir.path().join("nope")))).unwrap_err();
    assert!(e.message.contains("CM_LLAMA_SERVER"), "{e}");
    // A damaged app bundle says so.
    let bundle = dir.path().join("CaptureMann.app/Contents/MacOS");
    std::fs::create_dir_all(&bundle).unwrap();
    let e = locate_helper(&SearchPaths {
        env: None,
        exe_dir: Some(bundle),
        path_var: None,
    })
    .unwrap_err();
    assert!(e.message.contains("damaged"), "{e}");
}

#[test]
fn vm_stat_is_read_as_available_megabytes() {
    let text = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\n\
                Pages free:                               1000.\n\
                Pages active:                             5000.\n\
                Pages inactive:                           2000.\n\
                Pages speculative:                         500.\n\
                Pages purgeable:                           300.\n";
    // (1000 + 2000 + 500) pages of 16384 bytes.
    assert_eq!(parse_vm_stat(text), Some(3500 * 16384 / 1_000_000));
    assert_eq!(parse_vm_stat("garbage"), None);
}

#[test]
fn only_loopback_addresses_come_out_of_the_manager() {
    let fake = Fake::new();
    let m = manager(&fake);
    let dir = tempfile::tempdir().unwrap();
    let url = m.acquire(&spec(&dir, "m1")).unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    assert!(super::super::url_host(&url).is_some_and(|h| super::super::is_loopback_host(&h)));
}

#[cfg(unix)]
#[test]
fn the_real_platform_runs_stops_and_reports_a_fake_helper() {
    // A shell script stands in for llama-server: it only sleeps. Nothing is downloaded or loaded.
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("llama-server");
    std::fs::write(&helper, "#!/bin/sh\necho started \"$@\"\nsleep 30\n").unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let log = dir.path().join("logs/engine.log");
    let p = SystemPlatform::new(log.clone());
    let mut child = p.spawn(&helper, &["--port".into(), "1".into()]).unwrap();
    assert!(child.exited().is_none());
    // Wait until the script has really started (a loaded machine can be slow), then stop it.
    for _ in 0..100 {
        if std::fs::read_to_string(&log).is_ok_and(|t| t.contains("started")) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    child.kill();
    assert!(child.exited().is_some(), "stopped");
    let text = std::fs::read_to_string(&log).unwrap();
    assert!(text.contains("started --port 1"), "{text}");

    // One that exits at once is seen as exited with its last log line.
    std::fs::write(&helper, "#!/bin/sh\necho 'boom: bad model' >&2\nexit 3\n").unwrap();
    let mut child = p.spawn(&helper, &[]).unwrap();
    let mut how = None;
    for _ in 0..50 {
        how = child.exited();
        if how.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(how.unwrap().contains('3'));
    assert_eq!(child.log_tail(), "boom: bad model");
}
