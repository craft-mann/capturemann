//! CaptureMann desktop app: Studio window (library, settings), global hotkeys, tray icon and
//! the confirmation dialog, hosting the daemon in process.
//!
//! Flags: `--background` starts in the tray with no window (CAP-2.AC5, login item);
//! `--open <capture_id>` opens that capture in the editor at startup;
//! `--demo-toast`, `--demo-agent-toast` and `--demo-confirm` exercise the toast and the
//! confirmation dialog by hand; `--demo-launcher` opens the command launcher (LNC-1). Debug flags: `--theme dark|light` forces the Studio theme and
//! `--page recordings|screenshots|published|destinations|agents|settings` opens that section.
//! `--demo-recording`, `--demo-region`, `--demo-paused`, `--demo-agent`, `--demo-sheet` run the
//! recording UI on the real engine with fake tracks (no screen, microphone or camera is used; see
//! `recording::demo`).

use cm_capture::Capturer;
use cm_daemon::{Confirmer, Endpoint};
use cm_desktop::backend::{self, Backend, RecordingMode, StartConfig};
use cm_desktop::confirm::UiConfirmer;
use cm_desktop::library::LocalClock;
use cm_desktop::overlay;
use cm_desktop::platform;
use cm_desktop::recording::preview::PreviewSource;
use cm_desktop::settings::{self, FileSettings};
use cm_desktop::shortcuts::{Platform, ShortcutMap};
use cm_desktop::theme::ThemePreference;
use cm_desktop::toast::{ToastAction, ToastKind, ToastSpec};
use cm_desktop::ui::app::{self, AppHandle, AppModel, Command, Route};
use cm_desktop::ui::recording::RecState;
use cm_desktop::ui::{paint, studio};
use cm_i18n::{tr, tr_ctx};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use gpui::{App, AppContext as _};
use std::path::PathBuf;
use std::sync::Arc;

#[cfg(target_os = "macos")]
#[global_allocator]
static ALLOC: cm_desktop::alloc::ReturnLargeToOs = cm_desktop::alloc::ReturnLargeToOs;

/// `Backend::start` takes a concrete confirmer; the demo picks one at run time.
struct BoxedConfirmer(Arc<dyn Confirmer>);

#[async_trait::async_trait]
impl Confirmer for BoxedConfirmer {
    async fn confirm(&self, request: &cm_daemon::ConfirmationRequest) -> cm_daemon::Decision {
        self.0.confirm(request).await
    }
}

fn capturer() -> Capturer {
    // Debug builds: CM_DEBUG_SCREEN_PERMISSION=missing / CM_DEBUG_TARGETS_ERROR=... simulate a
    // failing listing (see `permissions`).
    cm_desktop::permissions::debug_capturer().unwrap_or_else(Capturer::system)
}

/// A second launch: asks the running instance to show itself. When the lock is held by another
/// copy of the app (a notarized release next to a development build share one library, one
/// socket and one lock), that copy is brought to the front and a dialog says why this one is
/// not starting; before, the second copy quit without a word.
fn show_running_instance(config_dir: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        use cm_desktop::instance::{running_pid, second_launch, SecondLaunch};
        let own = std::env::current_exe()
            .ok()
            .and_then(|e| cm_desktop::ui::app::bundle_of(&e));
        let running = running_pid(config_dir).and_then(|pid| {
            let out = std::process::Command::new("ps")
                .args(["-p", &pid.to_string(), "-o", "comm="])
                .output()
                .ok()?;
            let exe = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            cm_desktop::ui::app::bundle_of(std::path::Path::new(&exe))
        });
        match second_launch(own.as_deref(), running.as_deref()) {
            SecondLaunch::ShowIt => {
                if let Some(bundle) = own {
                    let _ = std::process::Command::new("open").arg(bundle).status();
                }
            }
            SecondLaunch::OtherCopy { bundle } => {
                let _ = std::process::Command::new("open").arg(&bundle).status();
                let name = bundle
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let title = tr!("CaptureMann is already running");
                let message = tr!(
                    "Another copy ({name}) is running and uses the same library and settings. Quit it from its menu bar icon, then open this one.",
                    name = name
                );
                // Arguments, never interpolated into the script.
                let _ = std::process::Command::new("osascript")
                    .args([
                        "-e",
                        "on run argv",
                        "-e",
                        "display alert (item 1 of argv) message (item 2 of argv) giving up after 60",
                        "-e",
                        "end run",
                        &title,
                        &message,
                    ])
                    .status();
            }
            SecondLaunch::Unknown => {
                log::info!("CaptureMann is already running; could not identify which copy");
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = config_dir;
}

fn main() {
    cm_daemon::logs::init_logging("desktop");
    cm_desktop::trace::init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flag = |f: &str| args.iter().any(|a| a == f);
    let open_capture = args
        .iter()
        .position(|a| a == "--open")
        .and_then(|i| args.get(i + 1))
        .cloned();
    let (background, demo_toast, demo_confirm, demo_agent, demo_launcher) = (
        flag("--background"),
        flag("--demo-toast"),
        flag("--demo-confirm"),
        flag("--demo-agent-toast"),
        flag("--demo-launcher"),
    );
    let value_of = |f: &str| {
        args.iter()
            .position(|a| a == f)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    // Debug: `--trigger-capture region|window|display` runs the hotkey's capture path two
    // seconds after startup (benchmarks; pair with `CM_TRACE=1`).
    let trigger = value_of("--trigger-capture").and_then(|v| match v.as_str() {
        "region" => Some(overlay::CaptureMode::Region),
        "window" => Some(overlay::CaptureMode::Window),
        "display" => Some(overlay::CaptureMode::Display),
        _ => None,
    });
    let demo_plan = cm_desktop::recording::demo::DemoPlan::parse(&args);
    let demo_query = value_of("--demo-launcher-query").unwrap_or_default();
    let theme_override = value_of("--theme").and_then(|v| ThemePreference::parse(&v));
    // `--demo-permissions missing|partial|granted|restart|windows`: a fake permission provider so the
    // setup screen and Settings -> Permissions can be looked at without touching the real grants.
    // `--demo-permissions-window` also opens the main window on the setup screen.
    // `--page permissions` is the setup screen, `--page settings-permissions` the Settings page.
    let demo_permissions = value_of("--demo-permissions");
    let demo_permissions_window = flag("--demo-permissions-window");
    let page_is_permissions = value_of("--page").as_deref() == Some("settings-permissions");
    let start_route = value_of("--page").and_then(|v| match v.as_str() {
        "permissions" => Some(Route::Permissions),
        "settings-permissions" => Some(Route::Settings),
        "recordings" => Some(Route::Recordings),
        "audio" => Some(Route::Audio),
        "screenshots" => Some(Route::Screenshots),
        "library" | "all" => Some(Route::Library),
        "published" => Some(Route::Published),
        "destinations" => Some(Route::Destinations),
        "agents" => Some(Route::Agents),
        "ai" => Some(Route::Ai),
        "settings" => Some(Route::Settings),
        _ => None,
    });

    // Read the local UTC offset before any other thread exists.
    let clock = LocalClock::detect();
    cm_desktop::save_folder::remember_offset(clock.offset);

    let config_dir = settings::config_dir();
    // One instance per user (R4-10): a second launch asks the first to show itself and exits.
    // The demo flags and tests (their own config dir, or CAPTUREMANN_ALLOW_MULTIPLE) are exempt.
    let _instance = if args.iter().any(|a| a.starts_with("--demo"))
        || std::env::var_os("CAPTUREMANN_ALLOW_MULTIPLE").is_some()
    {
        None
    } else {
        match cm_desktop::instance::acquire(&config_dir) {
            cm_desktop::instance::Acquired::First(lock) => Some(lock),
            cm_desktop::instance::Acquired::AlreadyRunning => {
                log::info!("CaptureMann is already running; showing it");
                show_running_instance(&config_dir);
                std::process::exit(0);
            }
            cm_desktop::instance::Acquired::Unavailable(e) => {
                log::warn!("single-instance lock unavailable: {e}");
                None
            }
        }
    };
    // CAP-2.AC5: bring the login item in line with the preference (a bundled app only).
    if _instance.is_some() {
        if let Err(e) = cm_desktop::login_item::sync(&config_dir) {
            log::warn!("could not update the login item: {e}");
        }
    }
    let file_settings = Arc::new(FileSettings::load(config_dir.join("settings.json")));
    let snapshot = file_settings.snapshot();
    let library_path = settings::resolve_library_path(
        std::env::var_os("CAPTUREMANN_LIBRARY").map(PathBuf::from),
        &snapshot,
        settings::default_library_dir(dirs::video_dir(), dirs::home_dir()),
    );
    let endpoint = Endpoint::from_env_or_default();

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log::warn!("cannot start the async runtime: {e}");
            std::process::exit(1);
        }
    };

    let (confirmer, confirm_rx) = UiConfirmer::channel();
    // The demo runs the real engine over fake tracks and a fake display list; an agent demo is
    // approved up front so its toast can be seen.
    let (recording, capturer, confirmer_for_daemon): (_, Capturer, Box<dyn Confirmer>) =
        match &demo_plan {
            Some(plan) => (
                RecordingMode::Fake(cm_daemon::testing::FakeSources::new()),
                cm_desktop::recording::demo::fake_capturer(),
                if plan.agent {
                    Box::new(cm_daemon::testing::FixedConfirmer::new(
                        cm_daemon::Decision::Approve,
                    ))
                } else {
                    Box::new(confirmer.clone())
                },
            ),
            None => (
                RecordingMode::System,
                capturer(),
                Box::new(confirmer.clone()),
            ),
        };
    let backend = Arc::new(Backend::start(
        &rt,
        StartConfig {
            library_path,
            fallback_path: settings::fallback_library_dir(dirs::data_dir()),
            settings: file_settings.clone(),
            endpoint: Some(endpoint),
            overlay_binary: overlay::locate(
                std::env::current_exe().ok().as_deref(),
                std::env::var_os("PATH"),
            ),
            recording,
        },
        capturer,
        backend::system_clipboard(),
        BoxedConfirmer(Arc::from(confirmer_for_daemon)),
    ));
    if let Some(root) = &backend.library_root {
        log::info!("library at {}", root.display());
    }
    if let Some(w) = &backend.ipc_warning {
        log::warn!("{w}");
    }
    if !backend.overlay.is_available() {
        log::warn!("cm-overlay not found; capture hotkeys will show an error");
    }

    let handle = rt.handle().clone();
    // The first display listing and freeze in a process pay one-time Core Graphics and GPU
    // setup (about 110 ms in total, measured); do it once in the background so the first
    // capture hotkey does not.
    if let Some(d) = backend.daemon.clone() {
        handle.spawn(async move { backend::warm_up(&d).await });
    }
    let events = backend
        .daemon
        .as_ref()
        .map(|d| backend::forward_events(&handle, d));
    let demo_handle = handle.clone();

    let application = gpui_platform::application().with_assets(paint::Assets);
    application.on_reopen(|cx| {
        if let Some(app) = cx.try_global::<AppHandle>().map(|h| h.0.clone()) {
            app::handle_command(&app, Command::ShowLibrary, cx);
        }
    });
    application.run(move |cx: &mut App| {
        paint::load_fonts(cx);
        platform::set_dock_icon();
        studio::bind_keys(cx);
        studio::install_app_actions(cx);

        let platform = Platform::current();
        let shortcuts = ShortcutMap::from_settings(&file_settings.snapshot(), platform);
        let rec_state = cx.new(|_| {
            RecState::new(if demo_plan.is_some() {
                PreviewSource::Fake
            } else {
                PreviewSource::System
            })
        });
        let model = cx.new(|_| {
            AppModel::new(
                backend.clone(),
                shortcuts.clone(),
                platform,
                clock,
                rec_state.clone(),
            )
        });
        model.update(cx, |m, _| m.theme_override = theme_override);
        if let Some(scenario) = &demo_permissions {
            let (provider, at_start) = cm_desktop::permissions::demo_provider(scenario);
            model.update(cx, |m, _| {
                m.perm_provider = provider;
                m.screen_at_start = at_start;
            });
        }
        if page_is_permissions {
            model.update(cx, |m, _| {
                m.settings_page_request =
                    Some(cm_desktop::ui::settings::model::SettingsSection::Permissions)
            });
        }
        cx.set_global(AppHandle(model.clone()));
        cx.set_global(studio::ThemeGlobal(cm_desktop::theme::Theme::DARK));
        app::apply_theme(&model, cx);
        // The saved UI language, before the first window or the tray is built.
        app::apply_language(&model, cx);
        // M5: read the speech models once, so the "no model yet" notice is right from the start.
        // This only reads the list; it never downloads anything.
        cm_desktop::ui::ai::refresh(&model, cx);
        // Jobs the daemon picked up from its queue at start (a held or interrupted transcription
        // goes on by itself): show them on the tabs.
        cm_desktop::ui::ai::sync_running_jobs(&model, cx);

        // Hotkeys and tray feed one command channel; no polling.
        let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel::<Command>();
        let tx = cmd_tx.clone();
        GlobalHotKeyEvent::set_event_handler(Some(move |ev: GlobalHotKeyEvent| {
            if ev.state == HotKeyState::Pressed {
                cm_desktop::trace::mark("hotkey_event");
                let _ = tx.send(Command::Hotkey(ev.id));
            }
        }));
        let tx = cmd_tx.clone();
        platform::set_menu_handler(move |id| {
            let _ = tx.send(Command::TrayMenu(id));
        });

        let mut startup_problems: Vec<ToastSpec> = Vec::new();
        // R4-16: an unreadable settings file is never reset silently.
        if let Some(spec) = settings::load_report_toast(file_settings.load_report()) {
            startup_problems.push(spec);
        }
        match GlobalHotKeyManager::new() {
            Ok(manager) => {
                let (ids, status) = platform::register_shortcuts(&manager, &shortcuts);
                for (action, res) in &status {
                    if let Err(e) = res {
                        if action.is_available() {
                            log::warn!("could not register {}: {e}", action.label());
                        }
                    }
                }
                model.update(cx, |m, _| {
                    m.hotkey_ids = ids;
                    m.registration = status;
                });
                cx.set_global(platform::HotkeyHolder(manager));
            }
            Err(e) => startup_problems.push(
                ToastSpec::new(ToastKind::Error, tr!("Global shortcuts are unavailable"))
                    .body(e.to_string()),
            ),
        }
        match platform::build_tray() {
            Ok(tray) => model.update(cx, |m, _| {
                // CAP-5: the remembered delay is the checked tray item from the start.
                tray.set_delay(m.capture_delay_s);
                m.tray = Some(tray);
            }),
            Err(e) => startup_problems
                .push(ToastSpec::new(ToastKind::Error, tr!("Menu bar icon unavailable")).body(e)),
        }

        let app2 = model.clone();
        cx.spawn(async move |cx| {
            while let Some(cmd) = cmd_rx.recv().await {
                cm_desktop::trace::mark("command_received");
                cx.update(|cx| app::handle_command(&app2, cmd, cx));
            }
        })
        .detach();

        let app2 = model.clone();
        let mut confirm_rx = confirm_rx;
        cx.spawn(async move |cx| {
            while let Some(msg) = confirm_rx.recv().await {
                cx.update(|cx| app::handle_confirm_msg(&app2, msg, cx));
            }
        })
        .detach();

        if let Some(mut rx) = events {
            let app2 = model.clone();
            cx.spawn(async move |cx| {
                while let Some(ev) = rx.recv().await {
                    if ev.name == "capture.created"
                        || ev.name == "capture.revised"
                        || ev.name == "capture.deleted"
                    {
                        let id = ev
                            .data
                            .get("capture_id")
                            .and_then(|v| v.as_str())
                            .map(str::to_owned);
                        cx.update(|cx| {
                            cm_desktop::ui::app::capture_changed(&app2, id.as_deref(), cx)
                        });
                    } else if ev.name == "publish.done"
                        || (ev.name == "job.done" && ev.data["type"] == "upload")
                    {
                        // A publish or an upload record was added to a manifest: the Published
                        // page, the details page and the library's Copy link reload.
                        cx.update(|cx| cm_desktop::ui::app::capture_changed(&app2, None, cx));
                    } else if ev.name.starts_with("recording.") || ev.name.starts_with("camera.") {
                        cx.update(|cx| cm_desktop::ui::recording::on_event(&app2, &ev, cx));
                    }
                    // M5: model downloads, transcripts and the job events of both.
                    if ev.name.starts_with("model.")
                        || ev.name.starts_with("transcript.")
                        || ev.name.starts_with("translation.")
                        || ev.name.starts_with("job.")
                    {
                        cx.update(|cx| cm_desktop::ui::ai::on_event(&app2, &ev, cx));
                    }
                    // The publish sheet follows its job (PUB-2).
                    if ev.name.starts_with("job.") {
                        cx.update(|cx| cm_desktop::ui::publish_sheet::on_event(&app2, &ev, cx));
                    }
                    // The quick export: progress, result and failure toasts, the details card.
                    if ev.name.starts_with("export.") || ev.name.starts_with("job.") {
                        cx.update(|cx| cm_desktop::ui::export_ui::on_event(&app2, &ev, cx));
                    }
                    // An agent (CLI or MCP) changed the library folder: the Library row follows.
                    if ev.name == "library.move_started" || ev.name == "library.moved" {
                        cx.update(|cx| cm_desktop::ui::library_move_flow::on_event(&app2, &ev, cx));
                    }
                    // The tray's Attention state: failed publishes, an offline library.
                    if ev.name.starts_with("job.")
                        || ev.name == "publish.done"
                        || ev.name.starts_with("library.")
                    {
                        cx.update(|cx| {
                            cm_desktop::ui::upload_ui::on_attention_event(&app2, &ev, cx)
                        });
                    }
                    // Uploads: the completion, failure and "reconnect needed" toasts.
                    if ev.name == "upload.queued" || ev.name.starts_with("job.") {
                        cx.update(|cx| cm_desktop::ui::upload_ui::on_event(&app2, &ev, cx));
                    }
                }
            })
            .detach();
        }

        if let Some(mode) = trigger {
            let tx = cmd_tx.clone();
            cx.spawn(async move |cx| {
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(2))
                    .await;
                cm_desktop::trace::mark("trigger");
                let _ = tx.send(Command::Capture(mode));
            })
            .detach();
        }
        app::start_agent_watch(&model, cx);
        app::start_studio_window_watch(&model, cx);
        // R1-6: bundles copied into the folder while the app was closed appear, deleted ones go.
        app::request_rescan(&model, true, cx);
        // REC-8: tell the user about recordings a crash left behind, once.
        for r in &backend.recovered {
            let spec = cm_desktop::recording::toasts::recovered_toast(r, &clock);
            startup_problems.push(spec);
        }
        // Quit while recording: close the bundle (best effort inside GPUI's 200 ms; the app's own
        // quit paths wait longer, see `app::quit`) and stop the job worker.
        if let Some(daemon) = backend.daemon.clone() {
            let rt = backend.rt.clone();
            cx.on_app_quit(move |_| {
                let (daemon, rt) = (daemon.clone(), rt.clone());
                async move {
                    let _ = rt
                        .spawn(async move { Backend::shutdown(&daemon).await })
                        .await;
                }
            })
            .detach();
        }
        if let Some(plan) = demo_plan.clone() {
            cm_desktop::ui::recording::run_demo(&model, plan, cx);
        }
        // PRM-1: read the states. When the setup screen is due (first launch with a gap, or a
        // required permission is missing) the main window opens on it, also for `--background`
        // launches (the only case where --background opens a window). Never asks the OS by itself.
        // The recording demos skip it.
        let show_setup =
            demo_plan.is_none() && app::prepare_permissions(&model, demo_permissions_window, cx);
        let explicit = start_route
            .clone()
            .or_else(|| open_capture.clone().map(app::Route::Editor));
        if let Some(route) = app::startup_route(show_setup, background, explicit) {
            app::navigate(&model, route, cx);
        }
        for spec in startup_problems {
            app::push_toast(&model, spec, cx);
        }
        if demo_toast {
            app::push_toast(
                &model,
                ToastSpec::new(ToastKind::Success, tr!("Copied to clipboard"))
                    .body(tr!("Saved to your library"))
                    .hint(
                        tr_ctx!("toast hint", "Paste"),
                        vec!["\u{2318}".into(), "V".into()],
                    )
                    .hint(
                        tr!("Open library"),
                        vec!["\u{2325}".into(), "\u{21E7}".into(), "L".into()],
                    )
                    .button(
                        tr_ctx!("toast button", "Annotate"),
                        ToastAction::OpenLibrary,
                    ),
                cx,
            );
        }
        if demo_launcher {
            cm_desktop::ui::launcher_window::open_launcher_with(&model, cx, &demo_query);
        }
        if demo_agent {
            app::push_toast(
                &model,
                ToastSpec::agent("start_recording", "claude-desktop"),
                cx,
            );
        }
        if demo_confirm {
            demo_handle.spawn(async move {
                let d = confirmer.confirm(&backend::demo_confirmation()).await;
                log::info!("demo confirmation answered: {d:?}");
            });
        }
    });
    drop(rt);
}
