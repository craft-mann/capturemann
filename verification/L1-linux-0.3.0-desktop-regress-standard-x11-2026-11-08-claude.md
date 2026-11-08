> **Desktop agent note (Nov 8, 2026), Ubuntu 24.04.5, GNOME 46, X11 session, debug build of `track/linux-desktop` (not a package).**
> Triage of the 12 FAIL rows for the desktop areas: `pkg.files`, `import.*`, `export.*`, `ai.no_model`, `linux.second_launch` failed only because
> the debug folder has no `ffmpeg` next to the programs (packaging/media areas); after linking the system FFmpeg into the folder,
> `linux.second_launch` PASSES (re-run, same session). `linux.desktop_stays_up` and `linux.tray_icon` PASS: the "left programs running" failure of
> the 0.2.9 report was zombie processes counted as running (fixed in `cm-regress`). `mcp.policy_allow` and `linux.vaapi_encoder` are not desktop areas
> (policy, recording encoder). The report header now names the GPU. Screen cases `shot.display`, `shot.region`, `redact.fixture` PASS. Status PARTIAL.

# L1 linux 0.2.9 regression report (standard)

- **Date:** 2026-11-08T13:57:55Z to 2026-11-08T14:02:05Z
- **Who:** desktop agent (x11-standard)
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit unknown, package folder `/home/trung/wt/desk-target/debug`
- **FFmpeg:** ffmpeg version 6.1.1-3ubuntu5 Copyright (c) 2000-2023 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/trung/wt/regress-x11-standard --who desktop agent (x11-standard) --verdict PARTIAL --package-dir /home/trung/wt/desk-target/debug --allow screen`, tier `standard`
- **Result:** 33 passed, 12 failed, 11 skipped of 56 cases
- **SHA-256 of the files under test:**
  - `capturemann` `9a620aa4c1ab5ee06152c8853cb2aee297e4d6222a74d0e609b6f6ddc399c99c`
  - `cm-mcp` `5a4eaa4f915db93b80da755a12580357c4ad50647fe02ecc44cad93f85e813e8`
  - `cm-daemon` `ddd93eed561b9d1a29fd67978004d29c14d4b9bd0fd4996062b19c0fa24fc759`
  - `cm-desktop` `f3e6abee363755baaf93168e83bae33fbbb2259bc0f003e76fbcba782c4bcae8`
  - `cm-overlay` `75f6ca5ac17476a68fff15122deefa59d132c1b97a5353e4252506127154b3dc`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: ffmpeg is missing (bundled FFmpeg); ffprobe is missing (bundled FFmpeg); llama-server is missing (the linux package ships the text engine); no LICENSE file; no THIRD_PARTY_LICENSES file; FFmp... |
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
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for linux |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for linux |
| `import.formats` | FAIL | mp4, mov and mkv import as recordings with the right size and duration: the package has no ffmpeg next to the programs |
| `import.rotated` | FAIL | a source with a rotation tag imports upright: the package has no ffmpeg next to the programs |
| `import.vfr` | FAIL | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux): the package has no ffmpeg next to the programs |
| `import.audio` | PASS | a WAV file imports as an audio capture of the right length |
| `import.errors` | FAIL | not a media file, a missing file, a file held open and a Unicode name end cleanly: the package has no ffmpeg next to the programs |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: the package has no ffmpeg next to the programs |
| `export.audio` | FAIL | audio export to WAV and M4A has the right codec and length: the package has no ffmpeg next to the programs |
| `export.errors` | FAIL | an unknown preset or capture is refused with the shared error codes: the package has no ffmpeg next to the programs |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | FAIL | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP): with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `ai.no_model` | FAIL | without a model, transcribe and the text model say what to do (exit 2) instead of failing: the package has no ffmpeg next to the programs |
| `ai.llama_version` | SKIP | the bundled llama-server runs --version and is not started by the daemon: this package has no llama-server |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | FAIL | a second launch with a file forwards it to the running app and exits: the package has no ffmpeg next to the programs |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | FAIL | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg) |
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
| AGT-POLICY | FAIL | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | FAIL | `ai.no_model` |
| AI-7 | FAIL | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | FAIL | `export.audio` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | FAIL | `linux.vaapi_encoder` |
| DIAG-1 | FAIL | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay` |
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| IMP-1 | FAIL | `import.formats`, `import.audio` |
| IMP-2 | FAIL | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | FAIL | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | FAIL | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | FAIL | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | PASS | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | FAIL | `pkg.files` |
| OD-24 | FAIL | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | FAIL | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | FAIL | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | FAIL | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | not run | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | FAIL | `import.vfr` |
| bug: Import editing copy not upright | FAIL | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | FAIL | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | FAIL | `shot.invalid`, `export.errors` |
| named pipe single instance | not run | `desktop.second_launch` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- ffmpeg is missing (bundled FFmpeg); ffprobe is missing (bundled FFmpeg); llama-server is missing (the linux package ships the text engine); no LICENSE file; no THIRD_PARTY_LICENSES file; FFmpeg/SOURCE-OFFER.txt is missing (LGPL source offer); FFmpeg/BUILD-INFO.txt is missing (LGPL source offer)

Evidence:

```text
capturemann: 16905360 bytes
cm-mcp: 34246544 bytes
cm-daemon: 232731072 bytes
cm-desktop: 359485120 bytes
cm-overlay: 41679704 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

### `import.formats`: mp4, mov and mkv import as recordings with the right size and duration

- the package has no ffmpeg next to the programs

### `import.rotated`: a source with a rotation tag imports upright

- the package has no ffmpeg next to the programs

### `import.vfr`: a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux)

- the package has no ffmpeg next to the programs

### `import.errors`: not a media file, a missing file, a file held open and a Unicode name end cleanly

- the package has no ffmpeg next to the programs

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the package has no ffmpeg next to the programs

### `export.audio`: audio export to WAV and M4A has the right codec and length

- the package has no ffmpeg next to the programs

### `export.errors`: an unknown preset or capture is refused with the shared error codes

- the package has no ffmpeg next to the programs

### `mcp.policy_allow`: agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP)

- with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked)

Evidence:

```text
daemon pid 269597 answered after 281 ms
CLI with policy allow: killed by a signal None
```

### `ai.no_model`: without a model, transcribe and the text model say what to do (exit 2) instead of failing

- the package has no ffmpeg next to the programs

### `linux.second_launch`: a second launch with a file forwards it to the running app and exits

- the package has no ffmpeg next to the programs

### `linux.vaapi_encoder`: VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding

- vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder

Evidence:

```text
vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder
profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2
daemon pid 273383 answered after 281 ms
diagnostics: - VAAPI render device /dev/dri/renderD128
```

## Not run

- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (8): shot.window, redact.window_text, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.settings_applied, desktop.tray_icon
- this package has no llama-server (1): ai.llama_version
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.startup` (378 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (826 ms): version 0.2.9; daemon pid 265371 answered after 281 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg; - ffmpeg at /usr/bin/ffmpeg; - llama-server not found: looked next to ~/wt/desk-target/debug, in the settings and on PATH; this build should include it; - text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4); - speaker engine built in: true; - speaker models: not b...
- `daemon.lifecycle` (1259 ms): daemon pid 265513 answered after 281 ms; pid 265513, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1396 ms): daemon pid 265604 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (675 ms): daemon pid 265691 answered after 281 ms; second daemon: exit 0 (0x00000000) in 30 ms; output: [INFO cm_core::logging] daemon 0.2.9 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr258765/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (899 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [265782]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1034 ms): daemon pid 265887 answered after 281 ms; daemon pid 265949 answered after 281 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (791 ms): daemon pid 266055 answered after 281 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x0...
- `targets.list` (744 ms): daemon pid 266163 answered after 281 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 1 windows; capabilities {"list_windows":true,"capture_window":true,"capture_region":true,"pick":false}; apps: xdg-desktop-portal-gnome; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (11163 ms): daemon pid 266226 answered after 281 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GVPX8W8E59QDKY; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1388 ms): daemon pid 267514 answered after 281 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (753 ms): daemon pid 267730 answered after 281 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (802 ms): daemon pid 267863 answered after 298 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (715 ms): daemon pid 268010 answered after 297 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (2420 ms): daemon pid 268090 answered after 296 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (769 ms): daemon pid 268199 answered after 296 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (6122 ms): daemon pid 268266 answered after 296 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1653 ms): daemon pid 268796 answered after 296 ms; rec_06GVPXC0HCXW3C2Z: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.dry_run` (896 ms): daemon pid 269033 answered after 281 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr258765/24/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov...; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (808 ms): daemon pid 269152 answered after 281 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (862 ms): daemon pid 269235 answered after 281 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1315 ms): daemon pid 269301 answered after 281 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 498 chars; get_diagnostics: ok, 4462 chars; list_models: ok, 1743 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1301 chars; list_targets: ok, 248 chars; list_devices: ok, 1280 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (812 ms): daemon pid 269511 answered after 281 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (723 ms): daemon pid 271961 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (841 ms): daemon pid 272017 answered after 296 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8077 ms): daemon pid 272156 answered after 296 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (797 ms): daemon pid 272336 answered after 296 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (988 ms): daemon pid 272435 answered after 296 ms; port 40037: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (1362 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (777 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 272548 answered after 296 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10964 ms): 1 visible window(s) of the app: hwnd 0x2800001 pid 272668 class 'dev.capturemann.app' title 'CaptureMann' rect [66, 69, 1246, 829] ex 0x0
- `linux.tray_icon` (6944 ms): tray items: 2 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-272993-1

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
