# L1 windows 0.3.5 regression report (hardware)

- **Date:** 2026-11-17T12:48:43Z to 2026-11-17T12:54:11Z
- **Who:** claude QA pass 2
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 7cd4e31d, package folder `C:\lab\c035-inst\capturemann-0.3.5-windows-x64`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --package-dir C:\lab\c035-inst\capturemann-0.3.5-windows-x64 --out C:\lab\regress-out\c035-hardware --allow screen,sound,mic,camera --who claude QA pass 2 --verdict PARTIAL --commit 7cd4e31d --whisper-model C:\lab\r6\ggml-small.bin`, tier `hardware`
- **Result:** 68 passed, 1 failed, 19 skipped of 88 cases
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
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
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
| `guide.real_clicks` | SKIP | two injected pointer clicks become two steps with their click points in physical px; no keystroke is recorded; Markdown export holds two images: start_guide is a Confirm tool and this headless daemon has no confirmation UI; the real-click run needs the desktop... |
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
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: the camera ran at 10.1 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun) |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: needs --allow second_monitor |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: give --soak-minutes <n> to run a long recording |
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
| AUD-1 | PASS | `rec.mic`, `rec.system_audio` |
| AUD-2 | PASS | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
| BLK-1 | PASS | `bulk.export_images` |
| BRD-1 | PASS | `board.create` |
| CAM-1 | FAIL | `rec.camera` |
| CAM-2 | FAIL | `rec.camera` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | PASS | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | PASS | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| GDE-1 | not run | `guide.real_clicks` |
| GDE-1.AC1 | not run | `guide.real_clicks` |
| GDE-1.AC2 | not run | `guide.real_clicks` |
| GDE-1.AC5 | PASS | `guide.refusals` |
| GDE-2 | PASS | `guide.edit_export` |
| GDE-3 | PASS | `guide.edit_export` |
| GDE-3.AC1 | PASS | `guide.edit_export`, `guide.real_clicks` |
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
| NFR-2 | PASS | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-9 | not run | `guide.real_clicks` |
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
| REC-3 | PASS | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-5.AC18 | PASS | `desktop.hud_drag` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | PASS | `desktop.hud_drag` |
| bug class: camera 10 fps in low light | FAIL | `rec.camera` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | PASS | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
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

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- the camera ran at 10.1 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)

Evidence:

```text
daemon pid 5764 answered after 337 ms
started rec_01a1e990914a_0001
export job job_5a8125b3f39690ec: done
camera 1280x720 h264 measured 10.1 fps, 5.68 s
```

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (4): shot.second_monitor, rec.external_4k, rec.each_monitor, desktop.recording_placement
- needs --installer <setup.exe> (1): installer.silent
- no --speech <two-voice recording> given (1): ai.speakers
- not implemented for windows (11): installer.dmg, desktop.recording_banner, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install
- start_guide is a Confirm tool and this headless daemon has no confirmation UI; the real-click run needs the desktop app (tests/manual/GDE.md section 4) (1): guide.real_clicks

## Evidence of passing cases

- `pkg.files` (700 ms): capturemann.exe: 5277184 bytes; cm-mcp.exe: 9128448 bytes; cm-daemon.exe: 47647744 bytes; cm-desktop.exe: 90083328 bytes; cm-overlay.exe: 4094976 bytes; licence: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\LICENSE"), third party: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\lab\\c035-inst\\capturemann-0.3.5-windows-x64\\doc\\NOTICE")
- `pkg.startup` (1046 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 36 ms; cm-daemon --bogus: exit 2 (0x00000002) in 52 ms; cm-overlay --help: exit 0 (0x00000000) in 21 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 48 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 37 ms; llama-server --version: exit 0 (0x00000000) in 51 ms
- `pkg.diagnostics` (1308 ms): version 0.3.5; daemon pid 12916 answered after 322 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9818, vendor id 0x1414; - ffmpeg at C:\lab\c035-inst\capturemann-0.3.5-windows-x64\ffmpeg.exe; - llama-server at C:\lab\c035-inst\capturemann-0.3.5-windows-x64\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - tex...
- `daemon.lifecycle` (2217 ms): daemon pid 2648 answered after 322 ms; pid 2648, 97 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2143 ms): daemon pid 9716 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1727 ms): daemon pid 2676 answered after 321 ms; second daemon: exit 0 (0x00000000) in 34 ms; output: [INFO cm_core::logging] daemon 0.3.5 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-12280-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2507 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [7920]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1931 ms): daemon pid 900 answered after 321 ms; daemon pid 9948 answered after 318 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1402 ms): daemon pid 5620 answered after 322 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (1237 ms): daemon pid 9684 answered after 322 ms; display 65537 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Windows PowerShell 5.1, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (2118 ms): daemon pid 3656 answered after 322 ms; display 65537 1920x1080; PNG 1920x1080; capture shot_06GYK3EJA03VZ8VH; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1421 ms): daemon pid 4484 answered after 322 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (5254 ms): daemon pid 5056 answered after 322 ms; window 1836266: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1436 ms): daemon pid 1468 answered after 321 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1334 ms): daemon pid 6352 answered after 322 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1340 ms): daemon pid 9604 answered after 337 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1701 ms): daemon pid 11668 answered after 323 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1265 ms): daemon pid 7648 answered after 337 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1371 ms): daemon pid 11652 answered after 321 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (5668 ms): daemon pid 9884 answered after 322 ms; window screenshot shot_06GYK3HMJRJ9R3MJ: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (1552 ms): daemon pid 10552 answered after 322 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (2148 ms): daemon pid 7788 answered after 322 ms; generated.png: shot_06GYK3J5ZF6GYWZJ, original.png 120x80, source {"imported_at":"2026-11-17T12:49:37.8010181Z","original_name":"generated.png","path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr12280\\22\\images\\generated.png"}; shot.jpg: shot_06GYK3J7RP1RJ6RV, original.png 1000x260, source {"imported_at":"2026-11-17T12:49:38.2600864Z","original_name":"shot.jpg","path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr12280\\22\\images\\shot.jpg"}; GIF named .p...
- `board.create` (2019 ms): daemon pid 12800 answered after 322 ms; Pair board shot_06GYK3JJJ06AKQVN: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2910 ms): daemon pid 9496 answered after 322 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4599 ms): daemon pid 9688 answered after 324 ms; clip-h264.mp4: rec_06GYK3K773V8W7NQ, 320x240, 3.00 s, job 401 ms; clip-h264.mov: rec_06GYK3K9XEJ3WFD6, 320x240, 3.00 s, job 405 ms; clip-h264.mkv: rec_06GYK3KCK4BK6KZ5, 320x240, 3.02 s, job 1772 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3268 ms): daemon pid 4160 answered after 338 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3251 ms): daemon pid 13348 answered after 338 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (2279 ms): daemon pid 13040 answered after 338 ms; rec_06GYK3MR2N4GK9KT: kind Some(String("audio")), 2.00 s; mp3 rec_06GYK3MSMH6QRDVR: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2207 ms): daemon pid 1204 answered after 324 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"format":"gif","message":"GIF files are not supported yet.","reason":"unsupported_format"},"kind":"i...; missing file: exit 2 (0x00000002) So...
- `import.dry_run` (1460 ms): daemon pid 9364 answered after 339 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "needed_bytes": 662456, "other_aud...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (1896 ms): daemon pid 11740 answered after 339 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (1472 ms): daemon pid 9716 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2518 ms): daemon pid 14116 answered after 324 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 420 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (17863 ms): daemon pid 12760 answered after 338 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2640 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2721 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 2051 ms; square-1080: h264 1080x1080 3.00...
- `export.audio` (3113 ms): daemon pid 9676 answered after 322 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (2100 ms): daemon pid 12116 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (2001 ms): daemon pid 7892 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1977 ms): daemon pid 1836 answered after 340 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1357 ms): daemon pid 936 answered after 322 ms; server capturemann 0.3.5, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1301 ms): daemon pid 3008 answered after 321 ms; 89 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1538 ms): daemon pid 9816 answered after 322 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 535 chars; get_diagnostics: ok, 3932 chars; list_models: ok, 1781 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1340 chars; list_targets: ok, 255 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1323 ms): daemon pid 6832 answered after 321 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1320 ms): daemon pid 9916 answered after 321 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1326 ms): daemon pid 14272 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1350 ms): daemon pid 8008 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8299 ms): daemon pid 13160 answered after 339 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1263 ms): daemon pid 12840 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1451 ms): daemon pid 14272 answered after 320 ms; port 55826: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13276 ms): daemon pid 9780 answered after 321 ms; started rec_01a1e98fab26_0001; export job job_d58147485f8ca500: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.43 s; manifest duration Some(Number(773540)) ticks; manifest 6.45 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((2.0, 2.0000000000000004)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_mis...
- `rec.region` (5726 ms): daemon pid 7980 answered after 322 ms; started rec_01a1e98fe0d4_0001; export job job_9bfb2c57341d1482: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (9941 ms): daemon pid 13960 answered after 321 ms; started rec_01a1e98ff8e6_0001; export job job_35901d1160c36f71: failed; region 18x18: 18x18 h264 3.40 s; started rec_01a1e9900a78_0002; export job job_82e497d496bdd23b: failed; export job job_35901d1160c36f71: failed; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8825 ms): daemon pid 4368 answered after 320 ms; started rec_01a1e990211a_0001; before the kill: state recording recorded 6.0 s; daemon pid 12292 answered after 316 ms; recovered: 4.37 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `rec.mic` (7552 ms): daemon pid 14096 answered after 319 ms; started rec_01a1e9904571_0001; export job job_cab69062c0df37d1: done; mic track 4.14 s; mic level: rms -77.4 dBFS, peak -54.0 dBFS; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (10955 ms): daemon pid 8204 answered after 337 ms; started rec_01a1e9906452_0001; export job job_a8ee85e6658c2368: done; system audio level: mean -6.9 dB, peak -0.1 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (2134 ms): daemon pid 11768 answered after 338 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (2147 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 2540 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (18121 ms): daemon pid 2724 answered after 324 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-17T12:52:25.665Z", "depends_on": [], "elapsed_ms": 15497, "error": null, "eta_ms": null, "finished_at": "2026-11-17T12:52:41.167Z", "held": false, "inputs": { "...; library entry after transcription: {"broken":false,"created":"2026-11-17T12:52:25.5096133Z","duration":360000,"height":240,"id":"rec_06GYK46KHWZFZV1X","kind":"recording","publishes":[],"size_bytes":663917,"source":{"imported...
- `desktop.stays_up` (11643 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8006 ms): second launch: exit 0 (0x00000000) after 50 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (7288 ms): hwnd 0x1100dc pid 11700 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (10583 ms): hwnd 0x1d00f6 pid 1168 class 'Zed::Window' title '' rect [369, 813, 1552, 965] ex 0x200188; hwnd 0x2305a6 pid 1168 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x19059e pid 1168 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x3c0558 pid 1168 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(960,889) is the HUD; export job job_33e874bd23834e12: done; log hygiene: 4 WARN line(s) in the case's logs
- `desktop.hud_drag` (15494 ms): a drag from (546,934) moved the HUD by (-90,0); export job job_98458f5fe4577929: done
- `desktop.no_console_flash` (3519 ms): daemon pid 13224 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9847 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5
- `desktop.tray_icon` (12083 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (2100 ms): daemon pid 6220 answered after 269 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr12280\70\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN...
- `regress.encoder_unavailable_wording` (1303 ms): daemon pid 4176 answered after 323 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.

## Notes by the QA agent (not written by `cm-regress`)

- **Verdict: PARTIAL.** 68 passed, 1 failed, 19 skipped. The failure is `rec.camera`: 10.1 fps (dark room). Control: the same case against the installed 0.3.4 (`%LOCALAPPDATA%\Programs\CaptureMann`) with the same `cm-regress.exe` right after, with a white full-screen window lighting the room, also gave 10.1 fps, so it is the room or camera and not a 0.3.5 change (a bug class the earlier 0.3.4 reports show too: reruns passed at 25.9 fps in a lit room). Skipped: the 7 `linux.*` cases, `shot.second_monitor`, `rec.external_4k`, `rec.each_monitor` (no second monitor), `rec.soak` (no `--soak-minutes`), `ai.speakers`, `installer.dmg`, `installer.silent`, `desktop.recording_banner`, `desktop.recording_placement`, `guide.real_clicks` (start_guide is a Confirm tool and the headless daemon has no confirmation UI; the real-click run was done by hand in the QA report).
- **Package under test:** a LAB build of commit `7cd4e31d` made on the ThinkPad with `packaging\windows\build-windows.ps1` (`CM_NO_SPEAKER_MODELS=1`, no sherpa-onnx libraries, so **no speaker engine**; LTO off), unpacked from `C:\wt\out\capturemann-0.3.5-windows-x64.zip` (SHA-256 `2d77b0e89493f5feda166c859c9ea94cde53e2a81d7defebfd2d72b2d975cd6b`) into `C:\lab\c035-inst`. Not the GitHub "Windows package" workflow output, not signed, the installer was not run.
- **`cm-regress.exe`:** built on the laptop from the commit `5678dc90` source of `crates/cm-regress` (it only changes the `guide.edit_export` and `guide.real_clicks` export folder to the sandbox library, because BUG-3 asks for a confirmation outside the library; `git diff 7cd4e31d 5678dc90` outside `verification/` touches only that file and the plan, so the product code is the same). The first run with the older `cm-regress` (built from the `7cd4e31d` source) is kept as `L1-windows-0.3.5-lab-7cd4e31d-regression-<tier>-firstrun-stale-case-2026-11-17-claude.md`: the only product-related failure there was `guide.edit_export` ("user_declined / no_confirmer": the case exported into the sandbox root, outside the trusted folders), the case defect that `5678dc90` fixes; hardware also had the camera failure above.
- A desktop session existed (user `trung` logged in on the console, `desktop session: true`); `--whisper-model C:\lab\r6\ggml-small.bin` was given (full and hardware: `ai.transcribe` ran). Screen lock `C:\lab\screen.lock` held during the run.
