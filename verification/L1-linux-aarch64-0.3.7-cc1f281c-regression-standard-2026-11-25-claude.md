# L1 linux 0.3.7 regression report (standard)

- **Date:** 2026-11-25T04:36:46Z to 2026-11-25T04:37:36Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit cc1f281c, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/dev/reg037/standard --package-dir /home/dev/.local/lib/capturemann --installer /home/dev/wt/release-out/cc1f281c-aarch64/capturemann-0.3.7-linux-aarch64.tar.gz --commit cc1f281c --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `standard`
- **Result:** 42 passed, 0 failed, 22 skipped of 64 cases
- **SHA-256 of the files under test:**
  - `capturemann` `9f1fa0e91113c8013b8fb37a98cde3892c53208f41e70cf890e9eadd9b842c29`
  - `cm-mcp` `42c267f642ba9ce9109cbc9a591617b04694bcf37bb9a0e35be6bee47d54a33a`
  - `cm-daemon` `9effdc9b085d03c0fc14d46d17ffa355b093278a27d08e0a28ca5fb4c49b8610`
  - `cm-desktop` `5dc36d8ca8323d38e64e13d98a8eed8e369f006d29d0e2eb23472ccea3e93b82`
  - `cm-overlay` `6575bb8571611cabb99e8e2b2970a2d99b415a826e8295470daff444ed322d68`
  - `ffmpeg` `b40429eeeb88f9992c195e19a95128523bf1b92d3d97a1a50f8fe6aa4e73920c`
  - `ffprobe` `4ef04c4b7ecc24cce38de39733674e9bb1c46fd6c963a850bacba1da9c0c8a44`
  - `llama-server` `e5347cca2583cb287df35af86bfd05f9e8c63e81650c431b3196c84dbfe8f7ff`
  - `capturemann-0.3.7-linux-aarch64.tar.gz` `5b371bdc30e694fb5e4e743dd36077afcd54368bd71017be2392464430d9e32e`

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
| `targets.list` | SKIP | targets lists displays and windows with readable names and real sizes: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.display` | SKIP | a display screenshot is a valid PNG of the display's physical size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.region` | SKIP | a region screenshot has the requested size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.invalid` | SKIP | bad targets are refused with the shared error codes and create no capture: needs a logged-in desktop session (this is session 0 or headless) |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: needs a logged-in desktop session (this is session 0 or headless) |
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
| `mcp.confirm_tier` | SKIP | start_recording and delete_capture do not run without an on-screen confirmation: no desktop session or the screen is locked: no display exists to name in start_recording |
| `mcp.policy_allow` | SKIP | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP): no desktop session: a recording cannot name a display (a headless computer answers platform_rejected) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: vainfo is not installed (apt install vainfo) |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) |
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
| CAP-1 | not run | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | not run | `shot.display` |
| CAP-1.AC2 | not run | `shot.region` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | not run | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
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
| IMP-4: second launch opens Import | not run | `desktop.second_launch`, `linux.second_launch` |
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
| REC-5 tray | not run | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | not run | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | not run | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | not run | `desktop.second_launch` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | not run | `mcp.confirm_tier` |

## Problems found

- None in the cases that ran.
## Not run

- needs a logged-in desktop session (this is session 0 or headless) (13): targets.list, shot.display, shot.region, shot.window, shot.invalid, redact.window_text, desktop.settings_applied, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay
- no desktop session or the screen is locked: no display exists to name in start_recording (1): mcp.confirm_tier
- no desktop session: a recording cannot name a display (a headless computer answers platform_rejected) (1): mcp.policy_allow
- not implemented for linux (5): desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) (1): regress.ffmpeg_missing_wording
- vainfo is not installed (apt install vainfo) (1): linux.vaapi_encoder

## Evidence of passing cases

- `pkg.files` (313 ms): capturemann: 4391944 bytes; cm-mcp: 9644104 bytes; cm-daemon: 100905096 bytes; cm-desktop: 156439496 bytes; cm-overlay: 12817376 bytes; licence: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/SHERPA-ONNX/NOTICE.txt"); note: no README in the package folder
- `pkg.startup` (419 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (789 ms): version 0.3.7; daemon pid 1441039 answered after 282 ms; - video encoder: software VP9 (no usable hardware H.264 encoder was found; expect more CPU use) through ffmpeg at ~/.local/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/.local/lib/capturemann/ffmpeg; - llama-server at ~/.local/lib/capturemann/llama-server (bundled with this build, llama.cpp v0.7.0); - text recognizer: tesseract at /usr/bin/tesseract (tesseract 5....
- `daemon.lifecycle` (1152 ms): daemon pid 1441092 answered after 281 ms; pid 1441092, 101 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1296 ms): daemon pid 1441137 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (649 ms): daemon pid 1441197 answered after 282 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: onnxruntime cpuid_info warning: Unknown CPU vendor. cpuinfo_vendor value: 0 [INFO cm_core::logging] daemon 0.3.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already se...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (856 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [1441250]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1010 ms): daemon pid 1441273 answered after 282 ms; daemon pid 1441312 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (686 ms): daemon pid 1441400 answered after 281 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x...
- `lib.manifest` (648 ms): daemon pid 1441514 answered after 285 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (649 ms): daemon pid 1441558 answered after 283 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (702 ms): daemon pid 1441602 answered after 281 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (632 ms): daemon pid 1441660 answered after 283 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (794 ms): daemon pid 1441697 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (707 ms): daemon pid 1441746 answered after 284 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case'...
- `import.images` (1291 ms): daemon pid 1441811 answered after 283 ms; generated.png: shot_06H11ZH1QVJC2AWG, original.png 120x80, source {"imported_at":"2026-11-25T04:36:59.974267593Z","original_name":"generated.png","path":"/tmp/cmr1440979/16/images/generated.png"}; shot.jpg: shot_06H11ZH2ZS350P1X, original.png 1000x260, source {"imported_at":"2026-11-25T04:37:00.292303411Z","original_name":"shot.jpg","path":"/tmp/cmr1440979/16/images/shot.jpg"}; GIF named .png: exit 2 (0x00000002) {"error":{"code":"invalid_input","message...
- `board.create` (1203 ms): daemon pid 1441905 answered after 283 ms; Pair board shot_06H11ZH8X24117CE: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2214 ms): daemon pid 1441978 answered after 281 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1792 ms): daemon pid 1442056 answered after 287 ms; clip-h264.mp4: rec_06H11ZHM996Y30A0, 320x240, 3.00 s, job 90 ms; clip-h264.mov: rec_06H11ZHNG8QBHREA, 320x240, 3.00 s, job 91 ms; clip-h264.mkv: rec_06H11ZHPP9FKBQWX, 320x240, 3.02 s, job 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1185 ms): daemon pid 1442413 answered after 281 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1172 ms): daemon pid 1442571 answered after 287 ms; stored track: vp9 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; editing copy note: The editing copy is software VP9: this computer has no hardware H.264 encoder (VA-API).; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1209 ms): daemon pid 1442722 answered after 286 ms; rec_06H11ZJ4HTWD392D: kind Some(String("audio")), 2.00 s; mp3 rec_06H11ZJ5NYE5Z70P: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1282 ms): daemon pid 1442863 answered after 287 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr1440979/23/picture...; missing file: exit 2 (0x00000002)...
- `import.dry_run` (681 ms): daemon pid 1443100 answered after 287 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr1440979/24/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mo...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (900 ms): daemon pid 1443157 answered after 283 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (718 ms): daemon pid 1443257 answered after 280 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1698 ms): daemon pid 1443399 answered after 283 ms; export --preset quick: clip-h264 - quick.mp4 by libvpx-vp9, 577 ms; vp9 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; codec note: This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder.; encoder libvpx-vp9: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1472 ms): daemon pid 1443615 answered after 284 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (985 ms): daemon pid 1443793 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (978 ms): daemon pid 1443947 answered after 284 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (934 ms): daemon pid 1444080 answered after 282 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (702 ms): daemon pid 1444205 answered after 284 ms; server capturemann 0.3.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (674 ms): daemon pid 1444256 answered after 283 ms; 90 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (820 ms): daemon pid 1444294 answered after 284 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 499 chars; get_diagnostics: ok, 4988 chars; list_models: ok, 1925 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1302 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (683 ms): daemon pid 1444352 answered after 284 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (689 ms): daemon pid 1444390 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7708 ms): daemon pid 1444435 answered after 282 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (721 ms): daemon pid 1444554 answered after 284 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (821 ms): daemon pid 1444613 answered after 282 ms; port 42521: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1114 ms): daemon pid 1444650 answered after 284 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech ...
- `ai.llama_version` (1155 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux aarch64; daemon pid 1444804 answered after 283 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
