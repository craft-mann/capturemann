# L1 windows 0.2.7 regression report (hardware)

- **Date:** 2026-11-05T16:18:24Z to 2026-11-05T16:30:50Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit fe1c01cd779e1fba1771a5bf28362d1a4241e4a5, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261105T161822Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera --soak-minutes 3 --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models C:\lab\r6\spk --installer C:\lab\capturemann-0.2.7-windows-x64-setup.exe --commit fe1c01cd779e1fba1771a5bf28362d1a4241e4a5`, tier `hardware`
- **Result:** 59 passed, 3 failed, 4 skipped of 66 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `ef7398e573534a0afbd8b3731da31974919046e2f7efe30bc41929cc155cbcf8`
  - `cm-mcp.exe` `095b1624e3527ea2501c03dc76d5e1c33b7edbf2f761540d5b48a9a468495f5c`
  - `cm-daemon.exe` `d44c5cea9581a5fc7a1e15243954d532e180cddc52f7d5be954fcb42062b72ef`
  - `cm-desktop.exe` `7f9174c18893ae6811f556261baadd77a237606712234bbd8bc2d7899278a5eb`
  - `cm-overlay.exe` `159bba705329662146143d5b91f3c96c53a94b511e58b8d46fff9e1def80fd87`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`
  - `capturemann-0.2.7-windows-x64-setup.exe` `5bcc69aaa53026e6508dc03983a2db59eab9161058a4881ca42163db8fbab60a`

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
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: the camera ran at 10.8 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun) |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: needs --allow second_monitor |
| `rec.soak` | PASS | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames |
| `ai.no_model` | FAIL | without a model, transcribe and the text model say what to do (exit 2) instead of failing: a model is installed in a brand-new config folder |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | FAIL | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): `capturemann ai speakers list rec_06GTS2Q7S96EACCP` failed (exit 2 (0x00000002)): {"error":{"code":"not_found","data":null,"exit_code":2,"message":"`rec_06GTS2Q7... |
| `installer.silent` | PASS | silent install into a temp folder, the programs run, silent uninstall leaves nothing |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.hud_click_through` | PASS | during a recording the HUD answers clicks and the area window is click-through |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: needs --allow second_monitor |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
| `regress.encoder_unavailable_wording` | PASS | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged by Windows during the whole run |

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
| AI-1 | FAIL | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-7 | FAIL | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | FAIL | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-1 | PASS | `rec.mic`, `rec.system_audio` |
| AUD-2 | PASS | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
| CAM-1 | FAIL | `rec.camera` |
| CAM-2 | FAIL | `rec.camera` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | PASS | `desktop.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
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
| IMP-4: second launch opens Import | PASS | `desktop.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | PASS | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | PASS | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor` |
| REC-1.AC | PASS | `rec.region` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 tray | PASS | `desktop.tray_icon` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | PASS | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | FAIL | `rec.camera` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | PASS | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
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
| named pipe single instance | PASS | `desktop.second_launch` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | PASS | `installer.silent` |

## Problems found

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- the camera ran at 10.8 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)

Evidence:

```text
daemon pid 3396 answered after 322 ms
started rec_01a1ac83c81e_0001
export job job_01b5bef92faf4c0a: done
camera 1280x720 h264 measured 10.8 fps, 5.64 s
```

### `ai.no_model`: without a model, transcribe and the text model say what to do (exit 2) instead of failing

- a model is installed in a brand-new config folder

Evidence:

```text
daemon pid 15108 answered after 338 ms
```

### `ai.speakers`: speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models)

- `capturemann ai speakers list rec_06GTS2Q7S96EACCP` failed (exit 2 (0x00000002)): {"error":{"code":"not_found","data":null,"exit_code":2,"message":"`rec_06GTS2Q7S96EACCP` has no transcript yet; run `transcribe` first"}}

Evidence:

```text
daemon pid 13688 answered after 337 ms
speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-05T16:29:07.641Z", "depends_on": [], "elapsed_ms": 9812, "error": null, "eta_ms": null, "finished_at": "2026-11-05T16:29:17.458Z", "held": false, "inputs": { "b...
```

## Not run

- needs --allow second_monitor (4): shot.second_monitor, rec.external_4k, rec.each_monitor, desktop.recording_placement

## Evidence of passing cases

- `pkg.files` (760 ms): capturemann.exe: 4626432 bytes; cm-mcp.exe: 8208384 bytes; cm-daemon.exe: 57391104 bytes; cm-desktop.exe: 95724032 bytes; cm-overlay.exe: 2787840 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (1105 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 37 ms; cm-daemon --bogus: exit 2 (0x00000002) in 52 ms; cm-overlay --help: exit 0 (0x00000000) in 22 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 48 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 52 ms; llama-server --version: exit 0 (0x00000000) in 52 ms
- `pkg.diagnostics` (1650 ms): version 0.2.7; daemon pid 14756 answered after 337 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\l...
- `daemon.lifecycle` (2256 ms): daemon pid 10056 answered after 337 ms; pid 10056, 86 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2435 ms): daemon pid 5252 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1870 ms): daemon pid 11092 answered after 321 ms; second daemon: exit 0 (0x00000000) in 49 ms; output: [INFO cm_core::logging] daemon 0.2.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-7724-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2735 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [13852]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (2005 ms): daemon pid 10672 answered after 322 ms; daemon pid 2732 answered after 318 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1470 ms): daemon pid 14696 answered after 337 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (1294 ms): daemon pid 3392 answered after 322 ms; display 17695695 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 2 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1623 ms): daemon pid 4568 answered after 323 ms; display 17695695 1920x1080; PNG 1920x1080; capture shot_06GTS0C34NKS4CBV; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1448 ms): daemon pid 6352 answered after 337 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (4693 ms): daemon pid 8888 answered after 322 ms; window 2426452: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1485 ms): daemon pid 11544 answered after 322 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1350 ms): daemon pid 3236 answered after 322 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1360 ms): daemon pid 3692 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1631 ms): daemon pid 3104 answered after 323 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1294 ms): daemon pid 5044 answered after 322 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1429 ms): daemon pid 13440 answered after 337 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (5132 ms): daemon pid 1372 answered after 322 ms; window screenshot shot_06GTS0F0PB0QC7FM: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4908 ms): daemon pid 11544 answered after 339 ms; clip-h264.mp4: rec_06GTS0FBK39D357A, 320x240, 3.00 s, job 447 ms; clip-h264.mov: rec_06GTS0FEBDDG575W, 320x240, 3.00 s, job 443 ms; clip-h264.mkv: rec_06GTS0FH37A4H3YD, 320x240, 3.02 s, job 1895 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3343 ms): daemon pid 4468 answered after 338 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3516 ms): daemon pid 13608 answered after 339 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (2005 ms): daemon pid 3028 answered after 322 ms; rec_06GTS0GZABMP9DTP: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2402 ms): daemon pid 9960 answered after 337 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) So...
- `import.dry_run` (1510 ms): daemon pid 13804 answered after 340 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2603 ms): daemon pid 13688 answered after 338 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 442 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (16976 ms): daemon pid 3104 answered after 322 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2557 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2563 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1869 ms; square-1080: h264 1080x1080 3.00 ...
- `export.audio` (3183 ms): daemon pid 5796 answered after 340 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (2037 ms): daemon pid 956 answered after 338 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1411 ms): daemon pid 6016 answered after 322 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1333 ms): daemon pid 5844 answered after 323 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1800 ms): daemon pid 11796 answered after 337 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 3579 chars; list_models: ok, 1898 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1324 chars; list_targets: ok, 258 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1372 ms): daemon pid 14352 answered after 321 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1369 ms): daemon pid 10436 answered after 322 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1375 ms): daemon pid 7368 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1415 ms): daemon pid 6084 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8330 ms): daemon pid 1400 answered after 338 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1308 ms): daemon pid 14140 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1557 ms): daemon pid 14364 answered after 321 ms; port 62449: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13205 ms): daemon pid 6164 answered after 321 ms; started rec_01a1ac830ba6_0001; export job job_55385f4688b0b0d3: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.47 s; manifest duration Some(Number(777542)) ticks; manifest 6.48 s, expected 6.05 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((2.0, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_missing":null,"pla...
- `rec.region` (5791 ms): daemon pid 3328 answered after 321 ms; started rec_01a1ac83413b_0001; export job job_fddc7f43e95ae864: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8947 ms): daemon pid 8868 answered after 322 ms; started rec_01a1ac8359d0_0001; before the kill: state recording recorded 6.0 s; daemon pid 14924 answered after 316 ms; recovered: 6.43 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `rec.mic` (7659 ms): daemon pid 8332 answered after 319 ms; started rec_01a1ac837ec3_0001; export job job_8b887354499a45b7: done; mic track 4.17 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (10152 ms): daemon pid 14352 answered after 338 ms; started rec_01a1ac839e4e_0001; export job job_6e5f9f05296e29e9: done; system audio level: mean -6.5 dB, peak -0.1 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `rec.soak` (182822 ms): daemon pid 10368 answered after 323 ms; started rec_01a1ac83ef75_0001; 180 s: state recording recorded Some(Number(180220)) ms late Some(Number(0.0))%; soak: h264 1920x1080 avg Some(30.0) 180.7 s; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (2262 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 11016 answered after 337 ms; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (268142 ms): daemon pid 8944 answered after 338 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-05T16:24:39.227Z", "depends_on": [], "elapsed_ms": 265002, "error": null, "eta_ms": null, "finished_at": "2026-11-05T16:29:04.231Z", "held": false, "inputs": { ...; library entry after transcription: {"broken":false,"created":"2026-11-05T16:24:38.9367921Z","duration":6015000,"height":null,"id":"rec_06GTS1PG09V5SJDY","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `installer.silent` (18124 ms): 1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]; saved before: 8 shared key(s), Start menu folder yes, user PATH yes; installer: exit 0 (0x00000000) in 12053 ms; installed: capturemann 0.2.7; daemon pid 13124 answered after 320 ms; uninstaller: exit 0 (0x00000000) in 288 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11897 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8400 ms): second launch: exit 0 (0x00000000) after 50 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (7118 ms): hwnd 0x1005cc pid 8372 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (10647 ms): hwnd 0x2204ba pid 11540 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188; hwnd 0x120546 pid 11540 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x1a05fc pid 11540 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x160656 pid 11540 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(1566,698) is the HUD; export job job_8866ecb8f5ddb1d2: done; log hygiene: 4 WARN line(s) in the case's...
- `desktop.no_console_flash` (3292 ms): daemon pid 10848 answered after 356 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9959 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5
- `desktop.tray_icon` (12278 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (2628 ms): daemon pid 13872 answered after 308 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr7724\60\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN...
- `regress.encoder_unavailable_wording` (1349 ms): daemon pid 14040 answered after 338 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
