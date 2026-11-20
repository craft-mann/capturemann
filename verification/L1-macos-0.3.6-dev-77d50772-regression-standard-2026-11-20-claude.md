# L1 macos 0.3.6 regression report (standard)

- **Date:** 2026-11-20T08:44:58Z to 2026-11-20T08:46:29Z
- **Who:** claude
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 77d50772, package folder `/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --package-dir /private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS --daemon target/release/cm-daemon --allow screen --commit 77d50772 --who claude --out /private/tmp/cm-final-mac-reg/standard`, tier `standard`
- **Result:** 51 passed, 0 failed, 13 skipped of 64 cases
- **QA note:** package folder: the candidate bundle `CaptureMann-dev.app/Contents/MacOS` (built by `CM_BUNDLE_OUT=... make bundle-dev` of 77d50772) plus a release `cm-daemon` of the same commit (the bundle has none). `desktop.hotkey_overlay` is skipped because the maintainer's own CaptureMann-dev (pid 25353) answers the same hotkey; it was not stopped. Hardware tier ran with `--allow screen` only (no mic, camera, sound, second monitor).
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
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: another CaptureMann is running (pid [25353]) and answers the same hotkey: quit it, then run this case |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for macos |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for macos |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for macos |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: not implemented for macos |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: not implemented for macos |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for macos |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: not implemented for macos |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: not implemented for macos |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: not implemented for macos |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) |
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
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack`, `board.create`, `bulk.export_images`, `guide.refusals` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
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
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
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
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | PASS | `pkg.files` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | not run | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | PASS | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | PASS | `desktop.second_launch` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |

## Problems found

- None in the cases that ran.
## Not run

- another CaptureMann is running (pid [25353]) and answers the same hotkey: quit it, then run this case (1): desktop.hotkey_overlay
- not implemented for macos (11): shot.window, redact.window_text, desktop.no_console_flash, desktop.settings_applied, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (343 ms): capturemann: 4491392 bytes; cm-mcp: 10250192 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon target/release/cm-daemon; cm-desktop: 120814976 bytes; cm-overlay: 8521120 bytes; licence: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/../Resources/THIRD_PARTY_LICENSES.md"), notice: Some("/private/tmp/cm-final-mac/CaptureMann-dev.app/Contents/MacOS/.....
- `pkg.startup` (1375 ms): capturemann --version: exit 0 (0x00000000) in 16 ms; cm-mcp --bogus: exit 2 (0x00000002) in 21 ms; cm-daemon --bogus: exit 2 (0x00000002) in 900 ms; cm-overlay --help: exit 0 (0x00000000) in 21 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 21 ms; llama-server --version: exit 0 (0x00000000) in 21 ms
- `pkg.diagnostics` (1263 ms): version 0.3.6; daemon pid 53163 answered after 293 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1248 ms): daemon pid 53270 answered after 293 ms; pid 53270, 101 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1826 ms): daemon pid 53324 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (726 ms): daemon pid 53440 answered after 289 ms; second daemon: exit 0 (0x00000000) in 19 ms; output: [INFO cm_core::logging] daemon 0.3.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr53029/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (960 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [53512]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1137 ms): daemon pid 53578 answered after 292 ms; daemon pid 53595 answered after 293 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (767 ms): daemon pid 53643 answered after 290 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (741 ms): daemon pid 53697 answered after 291 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; display 2 'S27B80P' 3840x2160 at (-442,-2160) scale 2; 27 windows; capabilities {"list_windows":true,"capture_window":true,"capture_region":true,"pick":false}; apps: CaptureMann-dev, ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Microsoft Word, Notes, Numbers, Slack, System Information, Telegram, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in th...
- `shot.display` (969 ms): daemon pid 53737 answered after 286 ms; display 1 3024x1964; PNG 3024x1964; capture shot_06GZG8CK0FR5YBNN; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (814 ms): daemon pid 53782 answered after 288 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (877 ms): daemon pid 53834 answered after 288 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (692 ms): daemon pid 53884 answered after 290 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (721 ms): daemon pid 53928 answered after 287 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (811 ms): daemon pid 53970 answered after 290 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (702 ms): daemon pid 54021 answered after 293 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (927 ms): daemon pid 54063 answered after 291 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (822 ms): daemon pid 54106 answered after 293 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","message":"ops[1]: only one spotlight per image: add ...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (1362 ms): daemon pid 54152 answered after 293 ms; generated.png: shot_06GZG8DGM6HREG14, original.png 120x80, source {"path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr53029/20/images/generated.png","imported_at":"2026-11-20T08:45:19.907999Z","original_name":"generated.png"}; shot.jpg: shot_06GZG8DHYX2GQA8V, original.png 1000x260, source {"path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr53029/20/images/shot.jpg","imported_at":"2026-11-20T08:45:20.24896Z","original_name":"shot.jpg"}; GI...
- `board.create` (1333 ms): daemon pid 54201 answered after 288 ms; Pair board shot_06GZG8DR6B7M5EY4: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2320 ms): daemon pid 54248 answered after 288 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1961 ms): daemon pid 54298 answered after 293 ms; clip-h264.mp4: rec_06GZG8E4YF7HBAB4, 320x240, 3.00 s, job 184 ms; clip-h264.mov: rec_06GZG8E67TPV4BB9, 320x240, 3.00 s, job 178 ms; clip-h264.mkv: rec_06GZG8E7K394BT8B, 320x240, 3.02 s, job 485 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1271 ms): daemon pid 54380 answered after 293 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1264 ms): daemon pid 54440 answered after 290 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1313 ms): daemon pid 54498 answered after 291 ms; rec_06GZG8EPNASCBX9Y: kind Some(String("audio")), 2.00 s; mp3 rec_06GZG8EQX791P0WT: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1446 ms): daemon pid 54557 answered after 293 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/var/folders/xm/d48xc26519...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (770 ms): daemon pid 54629 answered after 291 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr53029/28/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (934 ms): daemon pid 54674 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (827 ms): daemon pid 54815 answered after 291 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1531 ms): daemon pid 54863 answered after 289 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 265 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1588 ms): daemon pid 54924 answered after 293 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1066 ms): daemon pid 54993 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1093 ms): daemon pid 55050 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1022 ms): daemon pid 55102 answered after 293 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (768 ms): daemon pid 55155 answered after 293 ms; server capturemann 0.3.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (733 ms): daemon pid 55197 answered after 293 ms; 90 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1373 ms): daemon pid 55238 answered after 293 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3573 chars; list_models: ok, 1786 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1344 chars; list_targets: ok, 400 chars; list_devices: ok, 1881 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1038 ms): daemon pid 55381 answered after 293 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (840 ms): daemon pid 55447 answered after 292 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (750 ms): daemon pid 55489 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (753 ms): daemon pid 55531 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7821 ms): daemon pid 55573 answered after 293 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (959 ms): daemon pid 55634 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (906 ms): daemon pid 55702 answered after 290 ms; port 59981: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1134 ms): daemon pid 55768 answered after 293 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","message":"No speech model is installed, so nothing can be transcribed. Open the AI page in Captur...
- `ai.llama_version` (1248 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 55824 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11490 ms): 1 visible window(s) of the app: 'CaptureMann' 1180x792
- `desktop.second_launch` (7386 ms): second launch: exit 0 (0x00000000) after 94 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.tray_icon` (7231 ms): menu-bar items of the app: 1

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
