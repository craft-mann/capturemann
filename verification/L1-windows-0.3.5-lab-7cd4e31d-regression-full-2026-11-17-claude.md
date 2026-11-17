# L1 windows 0.3.5 regression report (full)

- **Date:** 2026-11-17T12:43:41Z to 2026-11-17T12:48:39Z
- **Who:** claude QA pass 2
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 7cd4e31d, package folder `C:\lab\c035-inst\capturemann-0.3.5-windows-x64`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir C:\lab\c035-inst\capturemann-0.3.5-windows-x64 --out C:\lab\regress-out\c035-full --allow screen,models --who claude QA pass 2 --verdict PARTIAL --commit 7cd4e31d --whisper-model C:\lab\r6\ggml-small.bin`, tier `full`
- **Result:** 66 passed, 0 failed, 14 skipped of 80 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `a656ac2695451379a429f391c43b45bcf3ed607e8ceec26e7996911658cb2510`
  - `cm-mcp.exe` `4ce354b32979163b2093db9361a4017087a1835f7c9be8f1d626bd0b5feeea37`
  - `cm-daemon.exe` `a63f56113506bc30841f307bab7ae19957d342ffcb5f8bcb4d7aac394a049236`
  - `cm-desktop.exe` `3cc46ac048a29748e413c22c2b50f98cc5bdadd12941e8aee7ccb964952722a3`
  - `cm-overlay.exe` `09651afdeb9caaca5e9cfbab049a01356cf5cd2d96180da5498ee88b62979611`
  - `ffmpeg.exe` `fac8b792db7bed42aff11860d9049f9ef9b4067346db52106a6ec5c3eb50d837`
  - `ffprobe.exe` `3fe6dd1ed834fe612d2f76cd19ca340fa1ce97c6a66e86764ed482342ecaeefa`
  - `llama-server.exe` `feb878e6896b32671343fce5e25d243099e231d148dbbca2ee016bcecca7d293`

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
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): no --speech <two-voice recording> given |
| `installer.dmg` | SKIP | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing: not implemented for windows |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: needs --installer <setup.exe> |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.hud_click_through` | PASS | during a recording the HUD answers clicks and the area window is click-through |
| `desktop.hud_drag` | PASS | during a recording a pointer drag on the HUD bar moves the HUD window |
| `desktop.recording_banner` | SKIP | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video: not implemented for windows |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: needs --allow second_monitor |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for windows |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: not implemented for windows |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: not implemented for windows |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for windows |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: not implemented for windows |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: not implemented for windows |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: not implemented for windows |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: not implemented for windows |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: not implemented for windows |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
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
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack`, `board.create`, `bulk.export_images`, `guide.refusals` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
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
| CAP-1 overlay | PASS | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | PASS | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
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
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-5.AC18 | PASS | `desktop.hud_drag` |
| REC-7 | PASS | `rec.screen_cursor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | PASS | `desktop.hud_drag` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 54 % late frames on a static screen | PASS | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | PASS | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | PASS | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | PASS | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | PASS | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | PASS | `desktop.no_console_flash` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | PASS | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | PASS | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | PASS | `shot.window` |
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

- needs --allow second_monitor (1): desktop.recording_placement
- needs --installer <setup.exe> (1): installer.silent
- no --speech <two-voice recording> given (1): ai.speakers
- not implemented for windows (11): installer.dmg, desktop.recording_banner, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install

## Evidence of passing cases

- `pkg.files` (713 ms): capturemann.exe: 5277184 bytes; cm-mcp.exe: 9128448 bytes; cm-daemon.exe: 47647744 bytes; cm-desktop.exe: 90083328 bytes; cm-overlay.exe: 4094976 bytes; licence: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\LICENSE"), third party: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\NOTICE")
- `pkg.startup` (1059 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 36 ms; cm-daemon --bogus: exit 2 (0x00000002) in 51 ms; cm-overlay --help: exit 0 (0x00000000) in 22 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 36 ms; llama-server --version: exit 0 (0x00000000) in 51 ms
- `pkg.diagnostics` (1295 ms): version 0.3.5; daemon pid 8768 answered after 323 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9818, vendor id 0x1414; - ffmpeg at C:\lab\c035-inst\capturemann-0.3.5-windows-x64\ffmpeg.exe; - llama-server at C:\lab\c035-inst\capturemann-0.3.5-windows-x64\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text...
- `daemon.lifecycle` (2161 ms): daemon pid 8848 answered after 322 ms; pid 8848, 97 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2138 ms): daemon pid 3436 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1745 ms): daemon pid 11696 answered after 321 ms; second daemon: exit 0 (0x00000000) in 34 ms; output: [INFO cm_core::logging] daemon 0.3.5 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-7864-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2499 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [7460]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1941 ms): daemon pid 8540 answered after 322 ms; daemon pid 9408 answered after 317 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1704 ms): daemon pid 1900 answered after 354 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (1292 ms): daemon pid 7424 answered after 323 ms; display 65537 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Windows PowerShell 5.1, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (2164 ms): daemon pid 2652 answered after 322 ms; display 65537 1920x1080; PNG 1920x1080; capture shot_06GYK29QYZCJ6ZQA; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1428 ms): daemon pid 7296 answered after 322 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (5289 ms): daemon pid 8264 answered after 322 ms; window 1705250: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1443 ms): daemon pid 3932 answered after 323 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1287 ms): daemon pid 9408 answered after 322 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1317 ms): daemon pid 1176 answered after 337 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1549 ms): daemon pid 13404 answered after 321 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1244 ms): daemon pid 9576 answered after 322 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1379 ms): daemon pid 12480 answered after 321 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (5621 ms): daemon pid 8344 answered after 322 ms; window screenshot shot_06GYK2CSE6HH7724: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (1556 ms): daemon pid 7072 answered after 322 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's l...
- `import.images` (2154 ms): daemon pid 868 answered after 321 ms; generated.png: shot_06GYK2DATMCPFZB1, original.png 120x80, source {"imported_at":"2026-11-17T12:44:35.9361968Z","original_name":"generated.png","path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr7864\\22\\images\\generated.png"}; shot.jpg: shot_06GYK2DCKNKWB7E3, original.png 1000x260, source {"imported_at":"2026-11-17T12:44:36.3935827Z","original_name":"shot.jpg","path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr7864\\22\\images\\shot.jpg"}; GIF named .png:...
- `board.create` (2040 ms): daemon pid 2444 answered after 322 ms; Pair board shot_06GYK2DQDRXNN7WF: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2915 ms): daemon pid 14204 answered after 321 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4574 ms): daemon pid 5844 answered after 322 ms; clip-h264.mp4: rec_06GYK2EC5RTG6PKH, 320x240, 3.00 s, job 407 ms; clip-h264.mov: rec_06GYK2EEVZS27ZB3, 320x240, 3.00 s, job 413 ms; clip-h264.mkv: rec_06GYK2EHJ41GAG4C, 320x240, 3.02 s, job 1794 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3296 ms): daemon pid 1580 answered after 339 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3206 ms): daemon pid 9560 answered after 324 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (2267 ms): daemon pid 1640 answered after 322 ms; rec_06GYK2FWRGQW1EP4: kind Some(String("audio")), 2.00 s; mp3 rec_06GYK2FYAH5ME7DQ: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2587 ms): daemon pid 9576 answered after 324 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"format":"gif","message":"GIF files are not supported yet.","reason":"unsupported_format"},"kind":"i...; missing file: exit 2 (0x00000002) So...
- `import.dry_run` (1506 ms): daemon pid 9304 answered after 341 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "needed_bytes": 662456, "other_aud...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (1929 ms): daemon pid 3516 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (1471 ms): daemon pid 14324 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2563 ms): daemon pid 12708 answered after 337 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 427 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (17783 ms): daemon pid 12980 answered after 338 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2641 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2740 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 2083 ms; square-1080: h264 1080x1080 3.00...
- `export.audio` (3110 ms): daemon pid 7052 answered after 337 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (2091 ms): daemon pid 1960 answered after 339 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (2017 ms): daemon pid 10360 answered after 337 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1965 ms): daemon pid 12932 answered after 337 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1349 ms): daemon pid 13972 answered after 322 ms; server capturemann 0.3.5, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1296 ms): daemon pid 9560 answered after 322 ms; 89 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1537 ms): daemon pid 8180 answered after 321 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 3924 chars; list_models: ok, 1780 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1339 chars; list_targets: ok, 255 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1350 ms): daemon pid 5900 answered after 338 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1328 ms): daemon pid 2368 answered after 322 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1328 ms): daemon pid 1960 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1329 ms): daemon pid 12908 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8322 ms): daemon pid 8004 answered after 339 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1268 ms): daemon pid 11656 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1444 ms): daemon pid 11916 answered after 319 ms; port 55785: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13352 ms): daemon pid 4892 answered after 320 ms; started rec_01a1e98b0b7e_0001; export job job_c0f7ed2367e4b9b6: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.47 s; manifest duration Some(Number(774745)) ticks; manifest 6.46 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((2.0, 2.0000000000000004)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_mis...
- `rec.region` (5728 ms): daemon pid 8016 answered after 322 ms; started rec_01a1e98b418d_0001; export job job_58b204da16c36cd3: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (9940 ms): daemon pid 9876 answered after 321 ms; started rec_01a1e98b599b_0001; export job job_de392d0ba74aaed0: failed; region 18x18: 18x18 h264 3.43 s; started rec_01a1e98b6b2c_0002; export job job_413195272a5489f7: failed; export job job_de392d0ba74aaed0: failed; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8821 ms): daemon pid 6744 answered after 321 ms; started rec_01a1e98b81c9_0001; before the kill: state recording recorded 6.0 s; daemon pid 5556 answered after 316 ms; recovered: 6.37 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `ai.no_model` (2029 ms): daemon pid 13300 answered after 321 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (2212 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 1468 answered after 336 ms; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (18481 ms): daemon pid 9440 answered after 324 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-17T12:46:51.687Z", "depends_on": [], "elapsed_ms": 15847, "error": null, "eta_ms": null, "finished_at": "2026-11-17T12:47:07.539Z", "held": false, "inputs": { "...; library entry after transcription: {"broken":false,"created":"2026-11-17T12:46:51.5288808Z","duration":360000,"height":240,"id":"rec_06GYK2XTZ45VXQD7","kind":"recording","publishes":[],"size_bytes":663916,"source":{"imported...
- `desktop.stays_up` (11634 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8030 ms): second launch: exit 0 (0x00000000) after 34 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (7155 ms): hwnd 0xe0038 pid 13400 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (10588 ms): hwnd 0x16057e pid 8536 class 'Zed::Window' title '' rect [369, 813, 1552, 965] ex 0x200188; hwnd 0xb0586 pid 8536 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x1004e2 pid 8536 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x1104da pid 8536 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(960,889) is the HUD; export job job_cf30692a568e5cda: done; log hygiene: 4 WARN line(s) in the case's logs
- `desktop.hud_drag` (15563 ms): a drag from (546,934) moved the HUD by (-90,0); export job job_5999ab4ce781213f: done
- `desktop.no_console_flash` (3502 ms): daemon pid 13332 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9998 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5
- `desktop.tray_icon` (13401 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (2078 ms): daemon pid 11000 answered after 249 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr7864\65\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN...
- `regress.encoder_unavailable_wording` (1291 ms): daemon pid 12544 answered after 322 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.

## Notes by the QA agent (not written by `cm-regress`)

- **Verdict: PARTIAL.** 66 passed, 0 failed, 14 skipped (the 7 `linux.*` cases, `installer.dmg`, `installer.silent` (no installer given), `ai.speakers` (no two-voice speech given; the lab build has no speaker engine), `desktop.recording_banner` (not implemented for Windows), `desktop.recording_placement` and `shot.second_monitor` (no second monitor allowed)).
- **Package under test:** a LAB build of commit `7cd4e31d` made on the ThinkPad with `packaging\windows\build-windows.ps1` (`CM_NO_SPEAKER_MODELS=1`, no sherpa-onnx libraries, so **no speaker engine**; LTO off), unpacked from `C:\wt\out\capturemann-0.3.5-windows-x64.zip` (SHA-256 `2d77b0e89493f5feda166c859c9ea94cde53e2a81d7defebfd2d72b2d975cd6b`) into `C:\lab\c035-inst`. Not the GitHub "Windows package" workflow output, not signed, the installer was not run.
- **`cm-regress.exe`:** built on the laptop from the commit `5678dc90` source of `crates/cm-regress` (it only changes the `guide.edit_export` and `guide.real_clicks` export folder to the sandbox library, because BUG-3 asks for a confirmation outside the library; `git diff 7cd4e31d 5678dc90` outside `verification/` touches only that file and the plan, so the product code is the same). The first run with the older `cm-regress` (built from the `7cd4e31d` source) is kept as `L1-windows-0.3.5-lab-7cd4e31d-regression-<tier>-firstrun-stale-case-2026-11-17-claude.md`: the only product-related failure there was `guide.edit_export` ("user_declined / no_confirmer": the case exported into the sandbox root, outside the trusted folders), the case defect that `5678dc90` fixes; hardware also had the camera failure above.
- A desktop session existed (user `trung` logged in on the console, `desktop session: true`); `--whisper-model C:\lab\r6\ggml-small.bin` was given (full and hardware: `ai.transcribe` ran). Screen lock `C:\lab\screen.lock` held during the run.
