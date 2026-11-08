# L1 linux 0.3.0 regression report (standard)

- **Date:** 2026-11-08T17:20:34Z to 2026-11-08T17:22:32Z
- **Who:** night orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit b4ee5cc4, package folder `/home/trung/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/trung/lab/regress-out/20261108T172033Z --who night orchestrator --verdict PARTIAL --allow screen --package-dir /home/trung/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann --commit b4ee5cc4`, tier `standard`
- **Result:** 47 passed, 2 failed, 7 skipped of 56 cases
- **SHA-256 of the files under test:**
  - `capturemann` `528f5fa393e59c071dbe569cf3a13a7f47e514df6ecad2abd1379592e5413c39`
  - `cm-mcp` `4f552566d74db8ff16432bd4079db3d68a0c9d9d89eaa499a15cad577c8ea153`
  - `cm-daemon` `75749d00a55ecf70fde1775b3ec6a605ed366fdaf1fe011d28488a5ca06db9a5`
  - `cm-desktop` `8688f62368a59f4abd2b1de841eb4f9c961c642c34aecbbb9d4141e9d000a319`
  - `cm-overlay` `c1c52eb47cf6bfbb9b0ce742399d2a85e1b6de0e84ebe570f3c21dbfc2ac6020`
  - `ffmpeg` `1f3f6b8963dbd4cd3ffbf75e039bdbf69d5787c108327363b5db6d3d51b55269`
  - `ffprobe` `a3cacff6efe515ac7d9cb5995280c7da35f85e37c86d1199fb55cb73ea78a272`
  - `llama-server` `92486ef745247229baa5240611d1530fd92989c81f77d10e58230eec205d837a`

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
| `desktop.settings_applied` | FAIL | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: no screenshot appeared within 30 s of the forwarded display capture request |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | FAIL | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | FAIL | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `mcp.errors`, `desktop.settings_applied` |
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
| D-R2 software VP9 fallback | FAIL | `linux.vaapi_encoder` |
| DIAG-1 | FAIL | `pkg.diagnostics`, `linux.vaapi_encoder` |
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
| REC-1 | FAIL | `linux.vaapi_encoder` |
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
| bug: settings set not applied to the running app | FAIL | `desktop.settings_applied` |
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

### `desktop.settings_applied`: settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long

- no screenshot appeared within 30 s of the forwarded display capture request

Evidence:

```text
forwarded capture request: exit 0 (0x00000000)
```

### `linux.vaapi_encoder`: VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding

- vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder

Evidence:

```text
vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder
profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2
daemon pid 961766 answered after 282 ms
diagnostics: - VAAPI render device /dev/dri/renderD128
```

## Not run

- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (5): desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (334 ms): capturemann: 5013576 bytes; cm-mcp: 9943552 bytes; cm-daemon: 97855616 bytes; cm-desktop: 150001520 bytes; cm-overlay: 10837880 bytes; licence: Some("/home/trung/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/trung/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/trung/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/...
- `pkg.startup` (437 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1069 ms): version 0.3.0; daemon pid 954572 answered after 282 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at ~/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/wt/x64pkg/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - llama-server at ~/wt/x6...
- `daemon.lifecycle` (1271 ms): daemon pid 954672 answered after 282 ms; pid 954672, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1565 ms): daemon pid 954737 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (673 ms): daemon pid 954843 answered after 282 ms; second daemon: exit 0 (0x00000000) in 31 ms; output: [INFO cm_core::logging] daemon 0.3.0 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr954438/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (921 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [954973]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1067 ms): daemon pid 955005 answered after 282 ms; daemon pid 955064 answered after 283 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (800 ms): daemon pid 955161 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x0...
- `targets.list` (765 ms): daemon pid 955258 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1364 ms): daemon pid 955311 answered after 282 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GVRBA8XR18HKC2; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (800 ms): daemon pid 955386 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (2861 ms): daemon pid 955452 answered after 282 ms; window 10485764: listed 842x442, saved 842x442; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (888 ms): daemon pid 955565 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (747 ms): daemon pid 955673 answered after 281 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (661 ms): daemon pid 955739 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (861 ms): daemon pid 955805 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (733 ms): daemon pid 955942 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1033 ms): daemon pid 955995 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (3166 ms): daemon pid 956064 answered after 282 ms; window screenshot shot_06GVRBBR41KDJWV8: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1985 ms): daemon pid 956246 answered after 282 ms; clip-h264.mp4: rec_06GVRBBWMZTPXV7N, 320x240, 3.00 s, job 143 ms; clip-h264.mov: rec_06GVRBBXZX47114E, 320x240, 3.00 s, job 208 ms; clip-h264.mkv: rec_06GVRBBZBMEW98GQ, 320x240, 3.02 s, job 340 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1293 ms): daemon pid 956775 answered after 282 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1296 ms): daemon pid 957023 answered after 282 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (922 ms): daemon pid 957258 answered after 281 ms; rec_06GVRBCEK3584FBY: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1397 ms): daemon pid 957376 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The video codec Png is not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr954438/25...; missing file: exit 2 (0x00000002) ...
- `import.dry_run` (795 ms): daemon pid 957720 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr954438/26/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1599 ms): daemon pid 957801 answered after 282 ms; export --preset quick: clip-h264 - quick.mp4 by h264_vaapi, 330 ms; h264 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; encoder h264_vaapi: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1512 ms): daemon pid 958106 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (970 ms): daemon pid 958430 answered after 281 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (806 ms): daemon pid 958607 answered after 281 ms; server capturemann 0.3.0, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (791 ms): daemon pid 958682 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1251 ms): daemon pid 958738 answered after 281 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 498 chars; get_diagnostics: ok, 5105 chars; list_models: ok, 1996 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1301 chars; list_targets: ok, 248 chars; list_devices: ok, 2039 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (828 ms): daemon pid 958905 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (793 ms): daemon pid 959006 answered after 282 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (716 ms): daemon pid 959078 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (799 ms): daemon pid 959132 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7845 ms): daemon pid 959197 answered after 282 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (862 ms): daemon pid 959422 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (941 ms): daemon pid 959520 answered after 282 ms; port 43197: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1191 ms): daemon pid 959573 answered after 281 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech m...
- `ai.llama_version` (1263 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 959793 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (336 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (851 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 961312 answered after 281 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10934 ms): 1 visible window(s) of the app: hwnd 0xa00001 pid 961365 class 'dev.capturemann.app' title 'CaptureMann' rect [402, 152, 1582, 912] ex 0x0
- `linux.second_launch` (6983 ms): second launch: exit 0 (0x00000000) after 31 ms
- `linux.tray_icon` (6945 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-961673-1

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
