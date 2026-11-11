# L1 macos 0.3.2 regression report (standard)

- **Date:** 2026-11-11T16:25:18Z to 2026-11-11T16:26:09Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 6f95def3cc9a96af3dcf8b44fe6bca2ce57b8e74, package folder `/Users/trungtran/release-0.3.2/macos-build/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --package-dir /Users/trungtran/release-0.3.2/macos-build/CaptureMann.app/Contents/MacOS --daemon target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261111T162517Z --who cm-regress via regress.sh --verdict PARTIAL --commit 6f95def3cc9a96af3dcf8b44fe6bca2ce57b8e74`, tier `standard`
- **Result:** 36 passed, 1 failed, 21 skipped of 58 cases
- **SHA-256 of the files under test:**
  - `capturemann` `f52f7e4f14f52b77b2e5564693b6d154479c3a50c9561b9600fec5b77bcca28b`
  - `cm-mcp` `99e90f195d93e05be0ee1e3c4023e65e4b96d14ad436e3f874e7d8a8f8c3ee8e`
  - `cm-daemon` `20de0d11f6c192e73cd91f81d67509e4248a3161dfa5049fc58648af24e1d6c8`
  - `cm-desktop` `1b4dba9752b91a03febaaf36eff77ae16dbe1795ee7f8c6f2c17bea9e0b61cfa`
  - `cm-overlay` `234e51d37c299559ed26660b206ed45446cc198af1bb9bf5722c5cd35a47aa8a`
  - `ffmpeg` `305d78779790f45f5cd0b55c4ef31b98aad33a2b4dd46832fbe48c4052bcf55f`
  - `ffprobe` `32700fc8c94dd4e52b0d662001834f5508fe71f272ca1571e2cafeee186be9c1`
  - `llama-server` `e5d61497f7ba17f3af088242e1efa729b588a322d86fe5417522b7a7dcb9c864`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | PASS | the package holds the programs, FFmpeg, the helper and the licences |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | FAIL | diagnostics reports app, system, models and engines, FFmpeg and the helper: `CaptureMann 0.3.2` not in the App section |
| `daemon.lifecycle` | PASS | daemon starts on a private endpoint, answers, stops, and the endpoint is gone |
| `daemon.library_created` | PASS | a fresh library is created and lists no captures; no helper program runs while idle |
| `daemon.second_instance` | PASS | a second daemon on a busy endpoint exits cleanly and leaves the first one working |
| `daemon.autostart` | PASS | the CLI starts a daemon when none runs |
| `settings.roundtrip` | PASS | settings set and get round trip, and survive a daemon restart |
| `settings.app_only_refused` | PASS | app-only settings keys are refused to agents and change nothing |
| `targets.list` | SKIP | targets lists displays and windows with readable names and real sizes: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.display` | SKIP | a display screenshot is a valid PNG of the display's physical size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.region` | SKIP | a region screenshot has the requested size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for macos |
| `shot.invalid` | SKIP | bad targets are refused with the shared error codes and create no capture: needs a logged-in desktop session (this is session 0 or headless) |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for macos |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux) |
| `import.audio` | PASS | a WAV file and an MP3 file import as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.estimate` | PASS | export --estimate prints About N MB, up to M MB with codec and pixels; a GIF has none; the refusals are the shared ones |
| `export.estimate_mcp` | PASS | the MCP tool estimate_export answers the same bound and typical size as the CLI |
| `export.errors` | PASS | an unknown preset or capture is refused with the shared error codes |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | SKIP | start_recording and delete_capture do not run without an on-screen confirmation: no desktop session or the screen is locked: no display exists to name in start_recording |
| `mcp.policy_allow` | PASS | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for macos |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for macos |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): needs a logged-in desktop session (this is session 0 or headless) |
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
| AGT-3 | PASS | `lib.annotate_undo` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | PASS | `export.audio` |
| CAP-1 | not run | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | not run | `shot.display` |
| CAP-1.AC2 | not run | `shot.region` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | not run | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | FAIL | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | not run | `desktop.second_launch`, `linux.second_launch` |
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

### `pkg.diagnostics`: diagnostics reports app, system, models and engines, FFmpeg and the helper

- `CaptureMann 0.3.2` not in the App section

Evidence:

```text
version 0.3.2
daemon pid 84643 answered after 300 ms
- ffmpeg at /opt/homebrew/bin/ffmpeg
- llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it
- text recognizer: apple_vision (available)
- speaker engine built in: true
- speaker models: not bundled with this build
```

## Not run

- needs a logged-in desktop session (this is session 0 or headless) (8): targets.list, shot.display, shot.region, shot.invalid, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.tray_icon
- no desktop session or the screen is locked: no display exists to name in start_recording (1): mcp.confirm_tier
- not implemented for macos (11): shot.window, redact.window_text, desktop.no_console_flash, desktop.settings_applied, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (334 ms): capturemann: 4083136 bytes; cm-mcp: 10082192 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon target/release/cm-daemon; cm-desktop: 107274512 bytes; cm-overlay: 5673936 bytes; licence: Some("/Users/trungtran/release-0.3.2/macos-build/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/Users/trungtran/release-0.3.2/macos-build/CaptureMann.app/Contents/MacOS/../Resources/THIRD_PARTY_LICENSES.md"), notice: Some("/Users/trungtran/release-0.3.2/macos-...
- `pkg.startup` (539 ms): capturemann --version: exit 0 (0x00000000) in 22 ms; cm-mcp --bogus: exit 2 (0x00000002) in 21 ms; cm-daemon --bogus: exit 2 (0x00000002) in 23 ms; cm-overlay --help: exit 0 (0x00000000) in 25 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 22 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 25 ms; llama-server --version: exit 0 (0x00000000) in 25 ms
- `daemon.lifecycle` (1255 ms): daemon pid 84736 answered after 300 ms; pid 84736, 87 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1739 ms): daemon pid 84790 answered after 302 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (738 ms): daemon pid 84897 answered after 294 ms; second daemon: exit 0 (0x00000000) in 22 ms; output: [INFO cm_core::logging] daemon 0.3.1 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr84544/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (984 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [84963]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1156 ms): daemon pid 85024 answered after 299 ms; daemon pid 85042 answered after 301 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (792 ms): daemon pid 85087 answered after 305 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `lib.manifest` (699 ms): daemon pid 85132 answered after 293 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (733 ms): daemon pid 85176 answered after 300 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (841 ms): daemon pid 85218 answered after 293 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (693 ms): daemon pid 85264 answered after 303 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (866 ms): daemon pid 85305 answered after 299 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (2302 ms): daemon pid 85347 answered after 301 ms; clip-h264.mp4: rec_06GWPVEERG3NM0RF, 320x240, 3.00 s, job 207 ms; clip-h264.mov: rec_06GWPVEG3G9GS75H, 320x240, 3.00 s, job 185 ms; clip-h264.mkv: rec_06GWPVEHFCKPRC74, 320x240, 3.02 s, job 526 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1303 ms): daemon pid 85429 answered after 303 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1298 ms): daemon pid 85487 answered after 303 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1336 ms): daemon pid 85543 answered after 298 ms; rec_06GWPVF1XV4BDYE2: kind Some(String("audio")), 2.00 s; mp3 rec_06GWPVF3779X4PW2: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1556 ms): daemon pid 85598 answered after 306 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file has no content to import.","data":{"reason":"unreadable_file","files":[{"path":"/var/folders/xm/d48xc26519j...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (809 ms): daemon pid 85667 answered after 301 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr84544/20/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1556 ms): daemon pid 85711 answered after 306 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 257 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1583 ms): daemon pid 85771 answered after 301 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1128 ms): daemon pid 85841 answered after 305 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1084 ms): daemon pid 85895 answered after 296 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1057 ms): daemon pid 85947 answered after 303 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (789 ms): daemon pid 85999 answered after 305 ms; server capturemann 0.3.2, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (747 ms): daemon pid 86041 answered after 306 ms; 81 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1258 ms): daemon pid 86081 answered after 303 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3573 chars; list_models: ok, 1786 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1344 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (799 ms): daemon pid 86225 answered after 305 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (770 ms): daemon pid 86267 answered after 305 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (765 ms): daemon pid 86307 answered after 306 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7849 ms): daemon pid 86348 answered after 305 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (828 ms): daemon pid 86483 answered after 298 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (918 ms): daemon pid 86539 answered after 305 ms; port 57566: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1185 ms): daemon pid 86602 answered after 306 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1274 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 86661 answered after 300 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
