# L1 macos 0.3.6 regression report (full)

- **Date:** 2026-11-20T08:46:30Z to 2026-11-20T08:48:47Z
- **Who:** claude
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 77d50772, package folder `/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS --daemon target/release/cm-daemon --allow screen --commit 77d50772 --who claude --out /private/tmp/cm-final-mac-reg/full`, tier `full`
- **Result:** 56 passed, 1 failed, 23 skipped of 80 cases
- **QA note:** `export.estimate_mcp` failed with "left programs running: cm-overlay (58515)". That `cm-overlay` was not started by the suite: the maintainer's own CaptureMann-dev (running from `~/Applications`, pid 25353, not stopped) opened a region picker and saved a region screenshot into his library at 08:47:29Z, the minute this case ran. The case passed when run alone straight after (`--only export.estimate_mcp,export.estimate`: 2 passed, 0 failed; not stored). `desktop.hotkey_overlay` is skipped in every tier on purpose: the maintainer's app answers the same hotkey. Package folder: the candidate bundle `CaptureMann-dev.app/Contents/MacOS` (built by `CM_BUNDLE_OUT=... make bundle-dev` of 77d50772) plus a release `cm-daemon` of the same commit (the bundle has none).
- **SHA-256 of the files under test:**
  - `capturemann` `554708f432e1cd5c66dcda3e1b5e1bd993807a2d1f9d7cbeb32e11681033b7cb`
  - `cm-mcp` `9819116d8b0ae3fc510fec16f76e597d9ceccb23ecf5b0b52a009569c11b62fe`
  - `cm-daemon` `83446cfe4a78418c289a87bea2d51d3838d4be2e503ce8ba96f4558fda313622`
  - `cm-desktop` `6465567fb1fa87c70c5e005dabf5a417856dc95fff116d4a498e8df26a6fa238`
  - `cm-overlay` `8757729c48bcfaac431965fa03be2e3a58068b6fc355cb517121e333c36643b1`
  - `ffmpeg` `86c9e6dec80931b95a2bd930c758ae2389cf0f1484f1c4d0e7c8ad251d46da4b`
  - `ffprobe` `14111d96467c4d0a836ca0f8e204628e9759af43dff306146061f49292e20d79`
  - `llama-server` `2e80dd6293117d3fc35e4a0f440096a998f9f9aba884655ad63ffc48506baeaa`

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
| `board.create` | PASS | a Pair board and a Free board of imported pictures are new screenshot captures with the right size and layout; the sources are not changed |
| `bulk.export_images` | PASS | export-images resizes, converts and numbers files without overwriting; the Blueprint step changes the file and never the capture |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux) |
| `import.audio` | PASS | a WAV file and an MP3 file import as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `guide.edit_export` | PASS | a hand-written guide: get, reorder, retitle, undo and redo are one step each; Markdown (relative links, PNGs in order), HTML (self-contained) and PDF export |
| `guide.refusals` | PASS | an unknown guide is not_found, a bad numbering or format is refused, and a start on a display that does not exist leaves no guide in the library |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
| `export.presets` | PASS | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.estimate` | PASS | export --estimate prints About N MB, up to M MB with codec and pixels; a GIF has none; the refusals are the shared ones |
| `export.estimate_mcp` | FAIL | the MCP tool estimate_export answers the same bound and typical size as the CLI: left programs running: cm-overlay (58515) |
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
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: another CaptureMann is running (pid [25353]) and answers the same hotkey: quit it, then run this case |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for macos |
| `desktop.hud_drag` | SKIP | during a recording a pointer drag on the HUD bar moves the HUD window: not implemented for macos |
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
| AGT-1 | FAIL | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `export.estimate_mcp`, `mcp.errors`, `desktop.settings_applied` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack`, `board.create`, `bulk.export_images`, `guide.refusals` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-1 | PASS | `import.images` |
| ANN-11 | PASS | `bulk.export_images` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| ANN-4 | PASS | `annotate.pack` |
| ANN-6 | PASS | `annotate.pack` |
| ANN-7 | PASS | `annotate.pack` |
| AUD-4 | PASS | `export.audio` |
| BLK-1 | PASS | `bulk.export_images` |
| BRD-1 | PASS | `board.create` |
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
| EXP-5.AC3 | FAIL | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| GDE-1.AC5 | PASS | `guide.refusals` |
| GDE-2 | PASS | `guide.edit_export` |
| GDE-3 | PASS | `guide.edit_export` |
| GDE-3.AC1 | PASS | `guide.edit_export` |
| GDE-4 | PASS | `guide.edit_export`, `guide.refusals` |
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
| REC-5.AC18 | not run | `desktop.hud_drag` |
| REC-7 | PASS | `rec.screen_cursor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | not run | `desktop.hud_drag` |
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

### `export.estimate_mcp`: the MCP tool estimate_export answers the same bound and typical size as the CLI

- left programs running: cm-overlay (58515)

Evidence:

```text
daemon pid 58516 answered after 286 ms
log hygiene: 1 WARN line(s) in the case's logs
```

## Not run

- another CaptureMann is running (pid [25353]) and answers the same hotkey: quit it, then run this case (1): desktop.hotkey_overlay
- needs --installer <setup.exe> (1): installer.dmg
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- not implemented for macos (18): shot.window, redact.window_text, installer.silent, desktop.hud_click_through, desktop.hud_drag, desktop.recording_banner, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (347 ms): capturemann: 4491392 bytes; cm-mcp: 10250192 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon target/release/cm-daemon; cm-desktop: 120814976 bytes; cm-overlay: 8521120 bytes; licence: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/../Resources/THIRD_PARTY_LICENSES.md"), notice: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/.....
- `pkg.startup` (482 ms): capturemann --version: exit 0 (0x00000000) in 17 ms; cm-mcp --bogus: exit 2 (0x00000002) in 19 ms; cm-daemon --bogus: exit 2 (0x00000002) in 19 ms; cm-overlay --help: exit 0 (0x00000000) in 19 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 18 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `pkg.diagnostics` (1291 ms): version 0.3.6; daemon pid 56572 answered after 293 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1225 ms): daemon pid 56666 answered after 290 ms; pid 56666, 101 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1825 ms): daemon pid 56721 answered after 292 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (739 ms): daemon pid 56839 answered after 293 ms; second daemon: exit 0 (0x00000000) in 19 ms; output: [INFO cm_core::logging] daemon 0.3.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr56464/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (959 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [56907]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1107 ms): daemon pid 56978 answered after 291 ms; daemon pid 56996 answered after 292 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (747 ms): daemon pid 57040 answered after 293 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (763 ms): daemon pid 57094 answered after 293 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; display 2 'S27B80P' 3840x2160 at (-442,-2160) scale 2; 27 windows; capabilities {"list_windows":true,"capture_window":true,"capture_region":true,"pick":false}; apps: CaptureMann-dev, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Microsoft Word, Notes, Numbers, Slack, System Information, Telegram, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in th...
- `shot.display` (1047 ms): daemon pid 57134 answered after 293 ms; display 1 3024x1964; PNG 3024x1964; capture shot_06GZG8QDHWFEGBC9; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (817 ms): daemon pid 57178 answered after 293 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (849 ms): daemon pid 57232 answered after 290 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (700 ms): daemon pid 57278 answered after 288 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (724 ms): daemon pid 57321 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (808 ms): daemon pid 57362 answered after 289 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (686 ms): daemon pid 57405 answered after 293 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (903 ms): daemon pid 57446 answered after 290 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (813 ms): daemon pid 57489 answered after 293 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","message":"ops[1]: only one spotlight per image: add ...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (1368 ms): daemon pid 57534 answered after 293 ms; generated.png: shot_06GZG8RB36K0EA73, original.png 120x80, source {"path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr56464/20/images/generated.png","imported_at":"2026-11-20T08:46:48.60408Z","original_name":"generated.png"}; shot.jpg: shot_06GZG8RCEW2HF574, original.png 1000x260, source {"path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr56464/20/images/shot.jpg","imported_at":"2026-11-20T08:46:48.95387Z","original_name":"shot.jpg"}; GIF...
- `board.create` (1303 ms): daemon pid 57591 answered after 293 ms; Pair board shot_06GZG8RJMSA79ZH7: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2333 ms): daemon pid 57652 answered after 293 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1961 ms): daemon pid 57710 answered after 290 ms; clip-h264.mp4: rec_06GZG8RZAKH82KHP, 320x240, 3.00 s, job 185 ms; clip-h264.mov: rec_06GZG8S0KW8EWQ5A, 320x240, 3.00 s, job 174 ms; clip-h264.mkv: rec_06GZG8S1XTW6S8MM, 320x240, 3.02 s, job 485 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1276 ms): daemon pid 57799 answered after 292 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1243 ms): daemon pid 57857 answered after 291 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1304 ms): daemon pid 57930 answered after 293 ms; rec_06GZG8SH0A48K6K9: kind Some(String("audio")), 2.00 s; mp3 rec_06GZG8SJ93FA6HH8: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1427 ms): daemon pid 57987 answered after 289 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/var/folders/xm/d48xc26519...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (768 ms): daemon pid 58062 answered after 293 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr56464/28/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (910 ms): daemon pid 58110 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (819 ms): daemon pid 58165 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1536 ms): daemon pid 58212 answered after 292 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 267 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (5489 ms): daemon pid 58277 answered after 292 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_videotoolbox, 657 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1308057 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_videotoolbox, 689 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1559030 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_videotoolbox, 486 ms; squar...
- `export.audio` (1590 ms): daemon pid 58391 answered after 291 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1090 ms): daemon pid 58459 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1361 ms): daemon pid 58794 answered after 621 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (776 ms): daemon pid 58850 answered after 290 ms; server capturemann 0.3.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (736 ms): daemon pid 58892 answered after 292 ms; 90 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1344 ms): daemon pid 58934 answered after 291 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3573 chars; list_models: ok, 1786 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1344 chars; list_targets: ok, 400 chars; list_devices: ok, 1881 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1013 ms): daemon pid 59075 answered after 293 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (854 ms): daemon pid 59147 answered after 293 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (745 ms): daemon pid 59190 answered after 291 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (4650 ms): daemon pid 59230 answered after 286 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7822 ms): daemon pid 59474 answered after 289 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (995 ms): daemon pid 59635 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (895 ms): daemon pid 59706 answered after 293 ms; port 60034: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13097 ms): daemon pid 59772 answered after 291 ms; started rec_01a1f823d4f2_0001; export job job_5e95f0723d29915f: done; screen: h264 3024x1964 yuv420p r 30.00 avg 30.00 frames None 6.47 s; manifest duration Some(Number(773107)) ticks; manifest 6.44 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.633333, 2.0)); cursor log: 45 lines, header {"type":"session","schema":1,"tick_rate":120000,"clock":"CLOCK_UPTIME_RAW ns (mach_absolute_time)","platform":"maco...
- `rec.region` (4820 ms): daemon pid 59858 answered after 292 ms; started rec_01a1f824083b_0001; export job job_ce06bba9f3c575c0: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (8805 ms): daemon pid 59921 answered after 293 ms; started rec_01a1f8241b35_0001; export job job_c237bcce7cd5dc24: done; region 18x18: 18x18 h264 3.37 s; started rec_01a1f8242b26_0002; export job job_6cd38b57c90c6e2c: done; export job job_c237bcce7cd5dc24: done; region 20x20: 20x20 h264 3.27 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8233 ms): daemon pid 60003 answered after 289 ms; started rec_01a1f8243db0_0001; before the kill: state recording recorded 6.0 s; daemon pid 60047 answered after 290 ms; recovered: 6.00 s of about 6.0 s recorded before the kill; log hygiene: 4 WARN line(s) in the case's logs
- `ai.no_model` (1110 ms): daemon pid 60090 answered after 292 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","message":"No speech model is installed, so nothing can be transcribed. Open the AI page in Captur...
- `ai.llama_version` (1244 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 60145 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11302 ms): 1 visible window(s) of the app: 'CaptureMann' 1180x792
- `desktop.second_launch` (7325 ms): second launch: exit 0 (0x00000000) after 56 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.tray_icon` (7224 ms): menu-bar items of the app: 1
- `regress.encoder_unavailable_wording` (735 ms): daemon pid 60762 answered after 287 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
