# L1 macos 0.3.3 regression report (full)

- **Date:** 2026-11-13T06:09:39Z to 2026-11-13T06:11:45Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 85df0235, package folder `/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261113T060938Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --commit 85df0235`, tier `full`
- **Result:** 53 passed, 0 failed, 22 skipped of 75 cases
- **SHA-256 of the files under test:**
  - `capturemann` `d095ec019cc83c59f1649d7c08e56bb4b898d37bbb4c5a7deb3b9e84acfee7c4`
  - `cm-mcp` `bbc450ae4df5e060651b417cdad40d72b9f93963b9bfed1efe521dc23f00f8be`
  - `cm-daemon` `984594461d80b7fc8d9e199d1e97a623f1f346d3f6a04fe91cb300d7b7ee2155`
  - `cm-desktop` `932095b9431276c18f13edcaacd1b72ecb90eb07adcea465baf2fd18ff6b9fc8`
  - `cm-overlay` `b47e409c2c688144941ac22be2122b33fa55ead376df91060a99495dfc62ac05`
  - `ffmpeg` `93711cc70922899af803ddd4447a30f30a9dedf85e16e5856ba1dcec056662dd`
  - `ffprobe` `7c086c614f127edde9f9c19df260424cf8335fad77b51663c0eec4bcf2e9f234`
  - `llama-server` `8c9379918e2c5c8f4466e55ec56f0c849288aa652ae64fec102acaf392dba19d`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | PASS | the package holds the programs, FFmpeg, the helper and the licences |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | PASS | diagnostics reports app, system, models and engines, FFmpeg and the helper |
| `daemon.lifecycle` | PASS | daemon starts on a private endpoint, answers, stops, and the endpoint is gone |
| `daemon.library_created` | PASS | a fresh library is created and lists no captures; no helper program runs while idle |
| `daemon.second_instance` | PASS | a second daemon on a busy endpoint exits cleanly and leaves the first one working |
| `daemon.autostart` | PASS | the CLI starts a daemon when none runs |
| `settings.roundtrip` | PASS | settings set and get round trip, and survive a daemon restart |
| `settings.app_only_refused` | PASS | app-only settings keys are refused to agents and change nothing |
| `targets.list` | PASS | targets lists displays and windows with readable names and real sizes |
| `shot.display` | PASS | a display screenshot is a valid PNG of the display's physical size |
| `shot.region` | PASS | a region screenshot has the requested size |
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for macos |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for macos |
| `annotate.pack` | PASS | callout, spotlight, stamps and a step with style, start and text render with the right pixels; a second spotlight is invalid_input naming ops[1] |
| `import.images` | PASS | a PNG and a JPEG import as screenshots; annotate and export work on them; a GIF named .png is refused by its content |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux) |
| `import.audio` | PASS | a WAV file and an MP3 file import as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
| `export.presets` | PASS | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.estimate` | PASS | export --estimate prints About N MB, up to M MB with codec and pixels; a GIF has none; the refusals are the shared ones |
| `export.estimate_mcp` | PASS | the MCP tool estimate_export answers the same bound and typical size as the CLI |
| `export.errors` | PASS | an unknown preset or capture is refused with the shared error codes |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | PASS | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `rec.screen_cursor` | PASS | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings |
| `rec.region` | PASS | a region recording is exactly the requested size |
| `rec.region_min` | PASS | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `installer.dmg` | SKIP | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing: needs --installer <setup.exe> |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: not implemented for macos |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: another CaptureMann is running (pid [13943]) and answers the same hotkey: quit it, then run this case |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for macos |
| `desktop.recording_banner` | SKIP | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video: not implemented for macos |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for macos |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for macos |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for macos |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for macos |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: not implemented for macos |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: not implemented for macos |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for macos |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: not implemented for macos |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: not implemented for macos |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: not implemented for macos |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: not implemented for macos |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: not implemented for macos |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) |
| `regress.encoder_unavailable_wording` | PASS | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | PASS | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `export.estimate_mcp`, `mcp.errors`, `desktop.settings_applied` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-1 | PASS | `import.images` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| ANN-4 | PASS | `annotate.pack` |
| ANN-6 | PASS | `annotate.pack` |
| ANN-7 | PASS | `annotate.pack` |
| AUD-4 | PASS | `export.audio` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch`, `linux.second_launch` |
| IMP-6 | PASS | `import.images` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | not run | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| LINUX-REC-5 | not run | `desktop.recording_banner` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | PASS | `rec.screen_cursor` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-7 | PASS | `rec.screen_cursor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 54 % late frames on a static screen | PASS | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | not run | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | PASS | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | PASS | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| make verify-release | not run | `installer.dmg` |
| named pipe single instance | PASS | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

- None in the cases that ran.
## Not run

- another CaptureMann is running (pid [13943]) and answers the same hotkey: quit it, then run this case (1): desktop.hotkey_overlay
- needs --installer <setup.exe> (1): installer.dmg
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- not implemented for macos (17): shot.window, redact.window_text, installer.silent, desktop.hud_click_through, desktop.recording_banner, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (355 ms): capturemann: 4083024 bytes; cm-mcp: 10130544 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 108603824 bytes; cm-overlay: 5690016 bytes; licence: Some("/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS/../Resources/TH...
- `pkg.startup` (1319 ms): capturemann --version: exit 0 (0x00000000) in 16 ms; cm-mcp --bogus: exit 2 (0x00000002) in 21 ms; cm-daemon --bogus: exit 2 (0x00000002) in 833 ms; cm-overlay --help: exit 0 (0x00000000) in 21 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 16 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 21 ms; llama-server --version: exit 0 (0x00000000) in 21 ms
- `pkg.diagnostics` (1281 ms): version 0.3.3; daemon pid 16108 answered after 291 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1238 ms): daemon pid 16203 answered after 293 ms; pid 16203, 88 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1822 ms): daemon pid 16259 answered after 288 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (726 ms): daemon pid 16377 answered after 292 ms; second daemon: exit 0 (0x00000000) in 18 ms; output: [INFO cm_core::logging] daemon 0.3.3 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr15979/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (975 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [16447]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1089 ms): daemon pid 16510 answered after 290 ms; daemon pid 16530 answered after 285 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (749 ms): daemon pid 16574 answered after 290 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (732 ms): daemon pid 16631 answered after 289 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; display 2 'S27B80P' 3840x2160 at (-442,-2160) scale 2; 21 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: CaptureMann-dev, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Excel, Microsoft Outlook, Microsoft PowerPoint, Notes, Safari, Slack, Telegram, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in ...
- `shot.display` (1002 ms): daemon pid 16672 answered after 290 ms; display 1 3024x1964; PNG 3024x1964; capture shot_06GX71PSXVTJZ6J3; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (812 ms): daemon pid 16715 answered after 291 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (863 ms): daemon pid 16757 answered after 293 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (700 ms): daemon pid 16805 answered after 293 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (693 ms): daemon pid 16847 answered after 291 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (803 ms): daemon pid 16900 answered after 290 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (685 ms): daemon pid 16947 answered after 293 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (896 ms): daemon pid 16988 answered after 291 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (817 ms): daemon pid 17032 answered after 290 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (1370 ms): daemon pid 17079 answered after 291 ms; generated.png: shot_06GX71QQA3KG2B69, original.png 120x80, source {"imported_at":"2026-11-13T06:10:00.146063Z","original_name":"generated.png","path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr15979/20/images/generated.png"}; shot.jpg: shot_06GX71QRNE6FHF2P, original.png 1000x260, source {"imported_at":"2026-11-13T06:10:00.493855Z","original_name":"shot.jpg","path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr15979/20/images/shot.jpg"}; G...
- `import.formats` (2275 ms): daemon pid 17129 answered after 293 ms; clip-h264.mp4: rec_06GX71QXE2YHRVG2, 320x240, 3.00 s, job 190 ms; clip-h264.mov: rec_06GX71QYQ3646SE0, 320x240, 3.00 s, job 175 ms; clip-h264.mkv: rec_06GX71R01HWG4M6Y, 320x240, 3.02 s, job 523 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1277 ms): daemon pid 17223 answered after 289 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1269 ms): daemon pid 17281 answered after 293 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1294 ms): daemon pid 17339 answered after 292 ms; rec_06GX71RG4PTS9YZ6: kind Some(String("audio")), 2.00 s; mp3 rec_06GX71RHB7SEFNDM: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1454 ms): daemon pid 17412 answered after 293 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/var/folders/xm/d48xc26519...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (774 ms): daemon pid 17481 answered after 288 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr15979/26/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1531 ms): daemon pid 17530 answered after 288 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 276 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (5447 ms): daemon pid 17599 answered after 290 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_videotoolbox, 676 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1308057 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_videotoolbox, 699 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1559030 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_videotoolbox, 511 ms; squar...
- `export.audio` (1574 ms): daemon pid 17728 answered after 293 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1089 ms): daemon pid 17799 answered after 291 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1066 ms): daemon pid 17856 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1018 ms): daemon pid 17914 answered after 292 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (760 ms): daemon pid 17981 answered after 288 ms; server capturemann 0.3.3, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (724 ms): daemon pid 18025 answered after 290 ms; 81 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1352 ms): daemon pid 18066 answered after 293 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3571 chars; list_models: ok, 1786 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1344 chars; list_targets: ok, 400 chars; list_devices: ok, 1881 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1006 ms): daemon pid 18210 answered after 288 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (778 ms): daemon pid 18278 answered after 290 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (744 ms): daemon pid 18330 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (749 ms): daemon pid 18373 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7825 ms): daemon pid 18417 answered after 293 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (938 ms): daemon pid 18491 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (905 ms): daemon pid 18574 answered after 293 ms; port 57613: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13088 ms): daemon pid 18638 answered after 287 ms; started rec_01a1d3878c0f_0001; export job job_bfb0b4332d3c1d6a: done; screen: h264 3024x1964 yuv420p r 30.00 avg 30.00 frames None 6.37 s; manifest duration Some(Number(763811)) ticks; manifest 6.37 s, expected 6.05 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.7, 2.0)); cursor log: 74 lines, header {"clicks":"available","clock":"CLOCK_UPTIME_RAW ns (mach_absolute_time)","move_source":"event_tap","permission_missing":...
- `rec.region` (4715 ms): daemon pid 18749 answered after 289 ms; started rec_01a1d387bf3a_0001; export job job_185e72b64e897267: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (8751 ms): daemon pid 18824 answered after 293 ms; started rec_01a1d387d1dd_0001; export job job_cbbd7ad8887024c5: done; region 18x18: 18x18 h264 3.33 s; started rec_01a1d387e182_0002; export job job_f62d9344b70950ea: done; export job job_cbbd7ad8887024c5: done; region 20x20: 20x20 h264 3.27 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8273 ms): daemon pid 18935 answered after 293 ms; started rec_01a1d387f432_0001; before the kill: state recording recorded 6.0 s; daemon pid 19007 answered after 290 ms; recovered: 6.00 s of about 6.0 s recorded before the kill; log hygiene: 4 WARN line(s) in the case's logs
- `ai.no_model` (1098 ms): daemon pid 19049 answered after 291 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1252 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 19109 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11487 ms): 1 visible window(s) of the app: 'CaptureMann' 1180x792
- `desktop.second_launch` (7348 ms): second launch: exit 0 (0x00000000) after 92 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.tray_icon` (7242 ms): menu-bar items of the app: 1
- `regress.encoder_unavailable_wording` (737 ms): daemon pid 19856 answered after 293 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
