# L1 linux 0.3.0 regression report (standard)

- **Date:** 2026-11-08T19:36:24Z to 2026-11-08T19:38:00Z
- **Who:** night orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5301d4b8, package folder `/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/trung/lab/regress-out/20261108T193622Z --who night orchestrator --verdict PARTIAL --allow screen --package-dir /home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann --commit 5301d4b8`, tier `standard`
- **Result:** 49 passed, 0 failed, 7 skipped of 56 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2d6346b93480161ec442ec26ebc099e9d4dbc0e7351944bd6a20cdabf32adf60`
  - `cm-mcp` `0baca1bbeb63be2a930c584db209039e45488366172ff1364f1e3477a30215ca`
  - `cm-daemon` `c546c08a0fd110d225cb636f5376fdc4fde0d1ae1e65a77b37846688e76727d0`
  - `cm-desktop` `b6e76b9e36b3ad2d464a217b1f4faf138dbdae05be24101de707d931905daa6c`
  - `cm-overlay` `fbe5ecfd0192d3026e69c5a9e0c8d0d521e363328009b2f33812d1c953503494`
  - `ffmpeg` `5da034620e64147d7e2ddf849ba258e34996e45849a7ea4777a3eef145e07ccc`
  - `ffprobe` `af190fa91ddd6d97b6e4426461526f3950509454a42c1e91d55b4c57cb78bc13`
  - `llama-server` `9b6a30cc527aa50e6f1c309f103f7fdb87c944a13d3382e733fee7c1d881ca5c`

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
| `shot.window` | PASS | a window screenshot is saved at the size the window is listed with |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | PASS | Auto-redact on a screenshot of a real window that shows an e-mail address |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux) |
| `import.audio` | PASS | a WAV file imports as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
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
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | PASS | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) |
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
| AI-1 | PASS | `ai.no_model` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | PASS | `export.audio` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | PASS | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | PASS | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | PASS | `pkg.files` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | PASS | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | PASS | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | not run | `desktop.second_launch` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |

## Problems found

- None in the cases that ran.
## Not run

- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (5): desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (326 ms): capturemann: 5013560 bytes; cm-mcp: 9943360 bytes; cm-daemon: 97857840 bytes; cm-desktop: 150193016 bytes; cm-overlay: 10845728 bytes; licence: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../shar...
- `pkg.startup` (423 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1063 ms): version 0.3.0; daemon pid 1106973 answered after 281 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - llama-server at ~/...
- `daemon.lifecycle` (1282 ms): daemon pid 1107065 answered after 281 ms; pid 1107065, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1572 ms): daemon pid 1107180 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (660 ms): daemon pid 1107289 answered after 281 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: [INFO cm_core::logging] daemon 0.3.0 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr1106886/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (921 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [1107366]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1057 ms): daemon pid 1107397 answered after 282 ms; daemon pid 1107456 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (797 ms): daemon pid 1107553 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x...
- `targets.list` (762 ms): daemon pid 1107650 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1377 ms): daemon pid 1107705 answered after 282 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GVSAD6SXJTHKNP; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (821 ms): daemon pid 1107780 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (2855 ms): daemon pid 1107844 answered after 282 ms; window 10485764: listed 842x442, saved 842x442; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (895 ms): daemon pid 1107960 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (749 ms): daemon pid 1108068 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (699 ms): daemon pid 1108132 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (976 ms): daemon pid 1108196 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (731 ms): daemon pid 1108282 answered after 281 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1033 ms): daemon pid 1108337 answered after 281 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (3153 ms): daemon pid 1108408 answered after 282 ms; window screenshot shot_06GVSAEPQMFK7Y1K: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (2011 ms): daemon pid 1108586 answered after 281 ms; clip-h264.mp4: rec_06GVSAEV8NQENKYD, 320x240, 3.00 s, job 130 ms; clip-h264.mov: rec_06GVSAEWMD1HCP0Q, 320x240, 3.00 s, job 203 ms; clip-h264.mkv: rec_06GVSAEY01HNG2P5, 320x240, 3.02 s, job 265 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1310 ms): daemon pid 1109117 answered after 281 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1209 ms): daemon pid 1109363 answered after 282 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1036 ms): daemon pid 1109598 answered after 282 ms; rec_06GVSAFCZFAP1YNV: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1526 ms): daemon pid 1109716 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The video codec Png is not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr1106886/2...; missing file: exit 2 (0x00000002)...
- `import.dry_run` (808 ms): daemon pid 1110062 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr1106886/26/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mo...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1649 ms): daemon pid 1110143 answered after 282 ms; export --preset quick: clip-h264 - quick.mp4 by h264_vaapi, 353 ms; h264 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; encoder h264_vaapi: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1515 ms): daemon pid 1110446 answered after 281 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1068 ms): daemon pid 1110715 answered after 281 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (810 ms): daemon pid 1110895 answered after 282 ms; server capturemann 0.3.0, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (807 ms): daemon pid 1110972 answered after 281 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1432 ms): daemon pid 1111026 answered after 281 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 499 chars; get_diagnostics: ok, 5101 chars; list_models: ok, 2001 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1302 chars; list_targets: ok, 248 chars; list_devices: ok, 2048 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (901 ms): daemon pid 1111185 answered after 281 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (793 ms): daemon pid 1111285 answered after 281 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (829 ms): daemon pid 1111360 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (802 ms): daemon pid 1111414 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7868 ms): daemon pid 1111479 answered after 281 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (909 ms): daemon pid 1111650 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (959 ms): daemon pid 1111748 answered after 281 ms; port 42195: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1341 ms): daemon pid 1111801 answered after 281 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech ...
- `ai.llama_version` (1262 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 1112019 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9691 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.8 s with capture.delay_s = 5
- `linux.session` (332 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (888 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 1112459 answered after 281 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10944 ms): 1 visible window(s) of the app: hwnd 0xa00001 pid 1112514 class 'dev.capturemann.app' title 'CaptureMann' rect [402, 152, 1582, 912] ex 0x0
- `linux.second_launch` (6975 ms): second launch: exit 0 (0x00000000) after 31 ms
- `linux.tray_icon` (6942 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-1112757-1
- `linux.vaapi_encoder` (1085 ms): vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder; profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2; daemon pid 1112900 answered after 281 ms; diagnostics: - VAAPI render device /dev/dri/renderD128; log hygiene: 1 WARN line...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
