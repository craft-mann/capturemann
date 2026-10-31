//! Headless daemon: `cm-daemon [--library <dir>] [--socket <path>]`.
//!
//! Serves the JSON-RPC action bus on the default local endpoint (or `CAPTUREMANN_SOCKET`, or `--socket`). With no
//! confirmation UI attached, Confirm-tier actions are always refused with `user_declined` and
//! `data.reason = "no_confirmer"` ("No confirmation UI is attached to this daemon"): a headless
//! daemon never lets an agent record, publish or delete. The publishing engine is not wired yet;
//! the desktop app embeds [`cm_daemon::Daemon`] and supplies the real [`cm_daemon::Confirmer`].
//!
//! It behaves like the app where it can (R2-8): the library folder defaults to the app's
//! (`CAPTUREMANN_LIBRARY`, then `library.path` in `settings.json`, then `~/Movies/CaptureMann`),
//! and settings, including the 60 fps self-test verdict, are kept in the app's `settings.json`
//! (`CAPTUREMANN_CONFIG_DIR` moves it). If an app or another daemon already serves the endpoint,
//! `cm-daemon` says so and exits 0 without touching the library.
//!
//! The recording engine is on (real sources), so `list_devices`, `get_recording_status` and the
//! recording controls work against a recording started by an approved request. At startup the
//! daemon recovers recordings a crash left behind (REC-8). On SIGINT or SIGTERM it finalizes a
//! recording in progress, then stops the job worker (waiting at most 10 s), then exits; a
//! second signal exits at once.

use cm_daemon::{Daemon, Endpoint, IpcServer};
use cm_library::Library;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage: cm-daemon [--library <dir>] [--socket <path>]");
    eprintln!("       (default library: CAPTUREMANN_LIBRARY, the `library.path` setting, then ~/Movies/CaptureMann)");
    ExitCode::from(2)
}

/// Resolves when the process is asked to stop (SIGINT or SIGTERM); a second request exits at
/// once (`wait_again`).
struct Stop {
    #[cfg(unix)]
    term: tokio::signal::unix::Signal,
    #[cfg(unix)]
    int: tokio::signal::unix::Signal,
}

impl Stop {
    fn new() -> std::io::Result<Stop> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            Ok(Stop {
                term: signal(SignalKind::terminate())?,
                int: signal(SignalKind::interrupt())?,
            })
        }
        #[cfg(not(unix))]
        {
            Ok(Stop {})
        }
    }

    async fn wait(&mut self) {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = self.term.recv() => {}
                _ = self.int.recv() => {}
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn capturer() -> Result<cm_capture::Capturer, String> {
    Ok(cm_capture::Capturer::system())
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn capturer() -> Result<cm_capture::Capturer, String> {
    Err("this platform has no screen capture backend (macOS, Windows and Linux only)".to_owned())
}

#[tokio::main]
async fn main() -> ExitCode {
    cm_daemon::logs::init_logging("daemon");
    let mut library: Option<PathBuf> = std::env::var_os("CAPTUREMANN_LIBRARY").map(PathBuf::from);
    let mut endpoint = Endpoint::from_env_or_default();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--library" => match args.next() {
                Some(v) => library = Some(PathBuf::from(v)),
                None => return usage(),
            },
            "--socket" => match args.next() {
                Some(v) => endpoint = Endpoint::from_arg(&v),
                None => return usage(),
            },
            _ => return usage(),
        }
    }
    let Some(library) = library.or_else(cm_daemon::config::default_library_path) else {
        log::error!("cannot determine the library folder; pass --library <dir>");
        return usage();
    };
    // An app (or another daemon) already serving this endpoint: say so, change nothing.
    if let Ok(Ok(_)) = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        cm_daemon::connect(&endpoint),
    )
    .await
    {
        log::warn!(
            "cm-daemon: the CaptureMann app or another daemon is already serving {}; nothing to do",
            endpoint.describe()
        );
        return ExitCode::SUCCESS;
    }
    let mut stop = match Stop::new() {
        Ok(s) => s,
        Err(e) => {
            log::error!("cannot listen for signals: {e}");
            return ExitCode::FAILURE;
        }
    };
    let lib = match Library::open_or_create(&library) {
        Ok(l) => l,
        Err(e) => {
            log::error!("cannot open library: {e}");
            return ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(1));
        }
    };
    let capturer = match capturer() {
        Ok(c) => c,
        Err(e) => {
            log::error!("{e}");
            return ExitCode::FAILURE;
        }
    };
    // Persistent jobs live next to the library index (`.capturemann/jobs.sqlite`).
    let queue = match cm_daemon::library_job_queue(lib.root()) {
        Ok(q) => q,
        Err(e) => {
            log::error!("cannot open the job queue: {e}");
            return ExitCode::from(u8::try_from(e.exit_code()).unwrap_or(1));
        }
    };
    // Settings live in the app's settings file, so a headless run keeps them (and the 60 fps
    // self-test verdict) like the app does.
    let settings = cm_daemon::config::FileSettings::load(cm_daemon::config::settings_path());
    let daemon = Daemon::builder(capturer, lib)
        .settings(settings)
        .jobs(queue)
        .recording_system()
        .ai(cm_daemon::ai_engine::real_services())
        .destinations(cm_storage::DestinationStore::new(
            cm_daemon::config::config_dir(),
        ))
        // Cloud destinations: tokens from the OS keychain (connected in the app).
        .cloud_backend(cm_daemon::storage_cloud::CloudBackend::system())
        // Publish accounts (PUB-1, PUB-2): the app writes the file, tokens are in the keychain.
        .publish_accounts(cm_publish::PublishAccountStore::new(
            cm_daemon::config::config_dir(),
        ))
        .build();
    match daemon.recover_recordings() {
        Ok(r) => {
            for rec in &r.recovered {
                log::warn!(
                    "recovered recording {} (lost up to {} ms)",
                    rec.capture_id,
                    rec.lost_ms
                );
            }
            for (folder, why) in &r.failed {
                log::warn!("could not recover {folder}: {why}");
            }
        }
        Err(e) => log::warn!("crash recovery failed: {e}"),
    }
    // The quick exporter (FFmpeg sidecar): a combined MP4 after every recording (REC-1, EXP-1).
    if let Err(e) = daemon.register_export_handler() {
        log::warn!("export is unavailable: {e}");
    }
    // Uploads to the user's destinations (STO-1, STO-3).
    if let Err(e) = daemon.register_upload_handler() {
        log::warn!("upload is unavailable: {e}");
    }
    // Publishing to YouTube (PUB-2).
    if let Err(e) = daemon.register_publish_handler() {
        log::warn!("publishing is unavailable: {e}");
    }
    // Moving the library folder (LIB-2).
    if let Err(e) = daemon.register_library_move_handler() {
        log::warn!("moving the library is unavailable: {e}");
    }
    // Local AI (M5): the job executors for model downloads and transcription, and the automatic
    // transcription after a recording (only with an installed model; nothing is downloaded
    // silently). The real engine (`ai_engine::real_services`) loads a model only inside a job.
    if let Err(e) = daemon.register_ai_handlers() {
        log::warn!("local AI is unavailable: {e}");
    }
    if let Err(e) = daemon.start_jobs() {
        log::error!("cannot start the job worker: {e}");
        return ExitCode::FAILURE;
    }
    daemon.start_ai_auto_transcribe();
    daemon.start_ai_auto_speakers();
    // Bundles copied in, restored or deleted from outside appear without a rescan (LIB-1).
    if let Err(e) = daemon.start_library_watcher() {
        log::warn!("library watcher is unavailable: {e}");
    }
    let server = match IpcServer::bind(&endpoint) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            log::error!("{e}");
            daemon.shutdown_jobs().await;
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            log::error!("cannot listen on {endpoint:?}: {e}");
            return ExitCode::FAILURE;
        }
    };
    log::warn!(
        "cm-daemon listening on {} (library {})",
        endpoint.describe(),
        library.display()
    );
    let mut failed = false;
    tokio::select! {
        r = server.run(daemon.clone()) => {
            if let Err(e) = r {
                log::error!("server stopped: {e}");
                failed = true;
            }
        }
        () = stop.wait() => {
            log::info!("stopping (a second signal exits at once)");
        }
    }
    // Finalize a recording, then the job worker (bounded); a second signal forces the exit.
    tokio::select! {
        () = async {
            daemon.shutdown_recording().await;
            daemon.shutdown_jobs().await;
        } => {}
        () = stop.wait() => {
            log::warn!("forced exit");
            std::process::exit(130);
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
