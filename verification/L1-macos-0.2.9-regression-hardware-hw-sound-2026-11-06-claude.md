# L1 macos 0.2.9 regression report (hardware)

- **Date:** 2026-11-06T23:51:31Z to 2026-11-06T23:53:55Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit unknown, package folder `/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --package-dir /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261106T235130Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera`, tier `hardware`
- **Result:** 51 passed, 1 failed, 25 skipped of 77 cases
- **SHA-256 of the files under test:**
  - `capturemann` `44d5da6176022e08436ec37d8e5a1a4a5eb3adb8525b7901cecffa74a4ed1ea5`
  - `cm-mcp` `c893a7aac9a70d69d914a15d3e2dfae8b5fb2839f89a56c38bc1b6d242a83ee9`
  - `cm-daemon` `34dfeceeee3973515d173c4806c68deb8dcf4e03dc6285f5148f5b56a1f3d3f4`
  - `cm-desktop` `28a0e768dafb1d0bfea4fbd47405658078b46400bc5cbb332ddf1ab38c1a99be`
  - `cm-overlay` `babd5f55c5922679cf648820aa6b64f6d1708a251a0115a9fb8234abbfc7f345`
  - `ffmpeg` `e52a04d18cc2205e66d763904d608b54eddd94738b6d3f96a10235beb545ce05`
  - `ffprobe` `ad4c6850e0864724e1bcaf60d8d22669202b2a5f95a31e10a3afdbe1c45873cc`
  - `llama-server` `d9e916fb218a72a445c4d0e51508ec02c1d1d5bbdd2796bf90a2c06423afde07`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | PASS | the package holds the programs, FFmpeg, the helper and the licences |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | FAIL | diagnostics reports app, system, models and engines, FFmpeg and the helper: `CaptureMann 0.2.9` not in the App section; the llama-server line does not say `bundled with this build` |
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
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for macos |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder |
| `import.audio` | PASS | a WAV file imports as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
| `export.presets` | PASS | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
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
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: not implemented for macos |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: give --soak-minutes <n> to run a long recording |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `installer.dmg` | SKIP | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing: needs --installer <setup.exe> |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: not implemented for macos |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for macos |
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
| `regress.encoder_unavailable_wording` | SKIP | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle: the machine recorded a 2x2 region: could not provoke the encoder refusal |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | PASS | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `mcp.errors`, `desktop.settings_applied` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-1 | PASS | `rec.mic`, `rec.system_audio` |
| AUD-2 | PASS | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
| CAM-1 | PASS | `rec.camera` |
| CAM-2 | PASS | `rec.camera` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | PASS | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | FAIL | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | PASS | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | not run | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | PASS | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
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
| REC-3 | PASS | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | not run | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | PASS | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
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
| bug: hardware_encoder_unavailable text named a Mac | not run | `regress.encoder_unavailable_wording` |
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

### `pkg.diagnostics`: diagnostics reports app, system, models and engines, FFmpeg and the helper

- `CaptureMann 0.2.9` not in the App section; the llama-server line does not say `bundled with this build`

Evidence:

```text
version 0.2.9
daemon pid 80371 answered after 289 ms
- ffmpeg at /opt/homebrew/bin/ffmpeg
- llama-server not found: The text engine was not found: run `make llama` to build it, or set CM_LLAMA_SERVER to a llama-server executable. CaptureMann releases...
- text recognizer: apple_vision (available)
- speaker engine built in: true
```

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (2): shot.second_monitor, rec.external_4k
- needs --installer <setup.exe> (1): installer.dmg
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- not implemented for macos (17): shot.window, redact.window_text, rec.each_monitor, installer.silent, desktop.hud_click_through, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install
- the machine recorded a 2x2 region: could not provoke the encoder refusal (1): regress.encoder_unavailable_wording
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (344 ms): capturemann: 4026416 bytes; cm-mcp: 9570656 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 105674944 bytes; cm-overlay: 5657040 bytes; licence: Some("/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS/../Resources/THI...
- `pkg.startup` (548 ms): capturemann --version: exit 0 (0x00000000) in 19 ms; cm-mcp --bogus: exit 2 (0x00000002) in 73 ms; cm-daemon --bogus: exit 2 (0x00000002) in 19 ms; cm-overlay --help: exit 0 (0x00000000) in 17 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 20 ms; llama-server --version: exit 0 (0x00000000) in 20 ms
- `daemon.lifecycle` (1239 ms): daemon pid 80491 answered after 293 ms; pid 80491, 82 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1273 ms): daemon pid 80557 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (726 ms): daemon pid 80693 answered after 289 ms; second daemon: exit 0 (0x00000000) in 19 ms; output: [INFO cm_core::logging] daemon 0.2.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr80226/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (956 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [80759]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1121 ms): daemon pid 80870 answered after 291 ms; daemon pid 80887 answered after 290 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (754 ms): daemon pid 80949 answered after 292 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (744 ms): daemon pid 81020 answered after 293 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; 19 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Neo, Notes, Slack, System Information, Telegram, Terminal, WeChat; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1092 ms): daemon pid 81061 answered after 290 ms; display 1 3024x1964; PNG 3024x1964; capture shot_06GV6HKJ2HW5VP49; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (856 ms): daemon pid 81131 answered after 293 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (854 ms): daemon pid 81173 answered after 291 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (689 ms): daemon pid 81245 answered after 292 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (707 ms): daemon pid 81302 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (816 ms): daemon pid 81344 answered after 291 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (673 ms): daemon pid 81415 answered after 293 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (913 ms): daemon pid 81457 answered after 289 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (2288 ms): daemon pid 81500 answered after 289 ms; clip-h264.mp4: rec_06GV6HMD871GNEER, 320x240, 3.00 s, job 185 ms; clip-h264.mov: rec_06GV6HMEJZCBFPAB, 320x240, 3.00 s, job 174 ms; clip-h264.mkv: rec_06GV6HMFWCMRFNKA, 320x240, 3.02 s, job 505 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1273 ms): daemon pid 81646 answered after 291 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1267 ms): daemon pid 81704 answered after 289 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (989 ms): daemon pid 81788 answered after 293 ms; rec_06GV6HMZZEGYDS5A: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1463 ms): daemon pid 81835 answered after 290 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file has no content to import.","data":{"reason":"unreadable_file","files":[{"path":"/var/folders/xm/d48xc26519j...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (765 ms): daemon pid 81930 answered after 293 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr80226/24/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1267 ms): daemon pid 82002 answered after 289 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 236 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (4918 ms): daemon pid 82076 answered after 291 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_videotoolbox, 634 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1308057 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_videotoolbox, 632 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1559030 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_videotoolbox, 465 ms; squar...
- `export.audio` (1551 ms): daemon pid 82277 answered after 290 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1014 ms): daemon pid 82346 answered after 289 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (767 ms): daemon pid 82424 answered after 293 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (732 ms): daemon pid 82466 answered after 293 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (904 ms): daemon pid 82533 answered after 293 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3139 chars; list_models: ok, 1779 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1329 chars; list_targets: ok, 264 chars; list_devices: ok, 1881 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (969 ms): daemon pid 82676 answered after 290 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (777 ms): daemon pid 82768 answered after 290 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (735 ms): daemon pid 82810 answered after 291 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (748 ms): daemon pid 82850 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7804 ms): daemon pid 82919 answered after 290 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (961 ms): daemon pid 83150 answered after 292 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (908 ms): daemon pid 83248 answered after 293 ms; port 62656: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13004 ms): daemon pid 83349 answered after 285 ms; started rec_01a1b3470c95_0001; export job job_721e6db0fed21ff5: done; screen: h264 3024x1964 yuv420p r 30.00 avg 30.00 frames None 6.37 s; manifest duration Some(Number(762031)) ticks; manifest 6.35 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.733333, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"CLOCK_UPTIME_RAW ns (mach_absolute_time)","move_source":"event_tap","permission_missi...
- `rec.region` (4717 ms): daemon pid 83602 answered after 288 ms; started rec_01a1b3473f83_0001; export job job_a73a83e34d8d3431: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (8604 ms): daemon pid 83724 answered after 293 ms; started rec_01a1b3475212_0001; export job job_ed53d46889ed6642: done; region 18x18: 18x18 h264 3.30 s; started rec_01a1b34761a9_0002; export job job_00c9d22a49d05ba0: done; export job job_ed53d46889ed6642: done; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8217 ms): daemon pid 83933 answered after 291 ms; started rec_01a1b34773cc_0001; before the kill: state recording recorded 6.0 s; daemon pid 84049 answered after 292 ms; recovered: 6.00 s of about 6.0 s recorded before the kill; log hygiene: 4 WARN line(s) in the case's logs
- `rec.mic` (5661 ms): daemon pid 84120 answered after 293 ms; started rec_01a1b347940b_0001; export job job_42c1a2ff587a69e5: done; mic track 4.21 s; mic level: rms -95.3 dBFS, peak -79.3 dBFS; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (9275 ms): daemon pid 84272 answered after 293 ms; started rec_01a1b347aa40_0001; export job job_6883d096e0b44bdf: done; system audio level: mean -10.5 dB, peak -4.4 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `rec.camera` (7601 ms): daemon pid 84546 answered after 293 ms; started rec_01a1b347ce99_0001; export job job_176000a926da7009: done; camera 1920x1080 h264 measured 30.0 fps, 5.59 s; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1148 ms): daemon pid 84770 answered after 290 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1256 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 84853 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11428 ms): 1 visible window(s) of the app: 'CaptureMann' 1180x793
- `desktop.second_launch` (7342 ms): second launch: exit 0 (0x00000000) after 56 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (5850 ms): hwnd 0x1f387 pid 85691 class 'CaptureMann-dev' title 'CaptureMann overlay' rect [-1, 0, 1513, 982] ex 0x8
- `desktop.tray_icon` (7234 ms): menu-bar items of the app: 1

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
