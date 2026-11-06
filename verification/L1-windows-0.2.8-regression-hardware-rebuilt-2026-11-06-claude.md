# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T11:56:48Z to 2026-11-06T12:02:54Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit e54cc665, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T115646Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --commit e54cc665`, tier `hardware`
- **Result:** 60 passed, 1 failed, 6 skipped of 67 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `cd6278fa32965137536745eb2da1dd9c4ebb2b810add56825a7e89574d90b5eb`
  - `cm-mcp.exe` `df01d78104b136eb976c7dcee714fe0844cfbf844f3b9d4878cb150cfe96f46e`
  - `cm-daemon.exe` `38edeb5a82a3e99cc1ab448afba089684a3043e1f7f6c44745911de655a04f38`
  - `cm-desktop.exe` `b02d3c4c0639858cba2a025c0cc8b9608b3428ff0ae9b62452a17547f75e4511`
  - `cm-overlay.exe` `8350a3e65ba06ac16a5822596fe3eaf89a4ef26841d03af081da26c23b28036a`
  - `ffmpeg.exe` `c0132821863d88ce0f49c568e522eff69741b2714e1a575ac5977b56384dd8e5`
  - `ffprobe.exe` `173b980b6e91c37123fde110367f432604ddb78b405f5533d707a6daa38172b3`
  - `llama-server.exe` `eec30348ea81f2bd4109c98ef13a09a0120218d0d78953ab9f2b784069f12eda`

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
| `rec.region_min` | PASS | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: the camera ran at 10.5 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun) |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: needs --allow second_monitor |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: give --soak-minutes <n> to run a long recording |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | PASS | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models) |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: needs --installer <setup.exe> |
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
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | PASS | `ai.speakers` |
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
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
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
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 tray | PASS | `desktop.tray_icon` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | FAIL | `rec.camera` |
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
| named pipe single instance | PASS | `desktop.second_launch` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- the camera ran at 10.5 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)

Evidence:

```text
daemon pid 2112 answered after 338 ms
started rec_01a1b0ba92d2_0001
export job job_000b5eb89b30cc2b: done
camera 1280x720 h264 measured 10.5 fps, 5.58 s
```

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (4): shot.second_monitor, rec.external_4k, rec.each_monitor, desktop.recording_placement
- needs --installer <setup.exe> (1): installer.silent

## Evidence of passing cases

- `pkg.files` (686 ms): capturemann.exe: 4626432 bytes; cm-mcp.exe: 8208384 bytes; cm-daemon.exe: 56283648 bytes; cm-desktop.exe: 94332928 bytes; cm-overlay.exe: 2767872 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (957 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 36 ms; cm-daemon --bogus: exit 2 (0x00000002) in 36 ms; cm-overlay --help: exit 0 (0x00000000) in 20 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 36 ms; llama-server --version: exit 0 (0x00000000) in 51 ms
- `pkg.diagnostics` (1492 ms): version 0.2.8; daemon pid 8300 answered after 321 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\ll...
- `daemon.lifecycle` (2109 ms): daemon pid 12500 answered after 321 ms; pid 12500, 86 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2375 ms): daemon pid 3104 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1713 ms): daemon pid 11680 answered after 322 ms; second daemon: exit 0 (0x00000000) in 33 ms; output: [INFO cm_core::logging] daemon 0.2.8 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-7732-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2380 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [16364]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1867 ms): daemon pid 2608 answered after 320 ms; daemon pid 14664 answered after 317 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1336 ms): daemon pid 16240 answered after 320 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (1183 ms): daemon pid 11680 answered after 320 ms; display 35389651 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 1 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (2065 ms): daemon pid 3996 answered after 322 ms; display 35389651 1920x1080; PNG 1920x1080; capture shot_06GV1E2F713MPPHS; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1317 ms): daemon pid 16212 answered after 321 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (4382 ms): daemon pid 15576 answered after 321 ms; window 4327126: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1374 ms): daemon pid 3608 answered after 321 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1227 ms): daemon pid 4104 answered after 321 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1230 ms): daemon pid 8776 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1439 ms): daemon pid 10196 answered after 321 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1177 ms): daemon pid 4732 answered after 321 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1290 ms): daemon pid 14884 answered after 320 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (4667 ms): daemon pid 8180 answered after 320 ms; window screenshot shot_06GV1E564C0RA3F2: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4451 ms): daemon pid 2368 answered after 321 ms; clip-h264.mp4: rec_06GV1E5F7J0CWR16, 320x240, 3.00 s, job 369 ms; clip-h264.mov: rec_06GV1E5HWQSD3191, 320x240, 3.00 s, job 362 ms; clip-h264.mkv: rec_06GV1E5MGKNEQ54N, 320x240, 3.02 s, job 1600 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3124 ms): daemon pid 3220 answered after 321 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3088 ms): daemon pid 16248 answered after 321 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1538 ms): daemon pid 13684 answered after 320 ms; rec_06GV1E6XM4YXSN1Q: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2172 ms): daemon pid 10332 answered after 321 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (1344 ms): daemon pid 6324 answered after 322 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2401 ms): daemon pid 9860 answered after 323 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 358 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (14837 ms): daemon pid 16276 answered after 322 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2127 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2233 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1747 ms; square-1080: h264 1080x1080 3.00...
- `export.audio` (2807 ms): daemon pid 9420 answered after 322 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1888 ms): daemon pid 3880 answered after 321 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1296 ms): daemon pid 13404 answered after 321 ms; server capturemann 0.2.8, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1233 ms): daemon pid 4008 answered after 322 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1646 ms): daemon pid 13788 answered after 321 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 4137 chars; list_models: ok, 2032 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1324 chars; list_targets: ok, 258 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1273 ms): daemon pid 9172 answered after 322 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1265 ms): daemon pid 5896 answered after 321 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1255 ms): daemon pid 8676 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1260 ms): daemon pid 16168 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8311 ms): daemon pid 4660 answered after 323 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1290 ms): daemon pid 9604 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1366 ms): daemon pid 13808 answered after 320 ms; port 64582: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (12992 ms): daemon pid 1708 answered after 319 ms; started rec_01a1b0b9b1f6_0001; export job job_844ffbfcd3efea79: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.37 s; manifest duration Some(Number(763654)) ticks; manifest 6.36 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.9999999999999998, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_mis...
- `rec.region` (5568 ms): daemon pid 13112 answered after 319 ms; started rec_01a1b0b9e606_0001; export job job_56bd584923768d6e: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (9897 ms): daemon pid 6352 answered after 319 ms; started rec_01a1b0b9fd20_0001; export job job_9d295bc3e31033e0: failed; region 18x18: 18x18 h264 3.33 s; started rec_01a1b0ba0e6d_0002; export job job_9631fd58659a330c: failed; export job job_9d295bc3e31033e0: failed; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8806 ms): daemon pid 15728 answered after 320 ms; started rec_01a1b0ba251c_0001; before the kill: state recording recorded 6.0 s; daemon pid 13124 answered after 317 ms; recovered: 6.30 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `rec.mic` (7527 ms): daemon pid 13184 answered after 318 ms; started rec_01a1b0ba4976_0001; export job job_7e5f7f5679ee03fd: done; mic track 4.11 s; mic level: rms -87.7 dBFS, peak -47.2 dBFS; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (10277 ms): daemon pid 12940 answered after 323 ms; started rec_01a1b0ba684b_0001; export job job_4ca73d76378267dd: done; system audio level: mean -6.5 dB, peak -0.1 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (2398 ms): daemon pid 12388 answered after 338 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (2241 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 10864 answered after 338 ms; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (42168 ms): daemon pid 3028 answered after 338 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T11:59:58.092Z", "depends_on": [], "elapsed_ms": 39156, "error": null, "eta_ms": null, "finished_at": "2026-11-06T12:00:37.25Z", "held": false, "inputs": { "a...; library entry after transcription: {"broken":false,"created":"2026-11-06T11:59:57.7919545Z","duration":6015000,"height":null,"id":"rec_06GV1EPR9Q8J61WR","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"imported...
- `ai.speakers` (62411 ms): daemon pid 1108 answered after 338 ms; bundled speaker models: 2 .onnx files; speakers: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T12:01:18.895Z", "depends_on": [], "elapsed_ms": 11645, "error": null, "eta_ms": null, "finished_at": "2026-11-06T12:01:40.513Z", "held": false, "inputs": { "...; speakers listed: 2; speakers on the words: {"1", "2"}; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11792 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8410 ms): second launch: exit 0 (0x00000000) after 50 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (7097 ms): hwnd 0x3705ac pid 3144 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (10096 ms): hwnd 0x3106b2 pid 2368 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188; hwnd 0x1806e2 pid 2368 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x83057a pid 2368 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x2206e0 pid 2368 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(1566,698) is the HUD; export job job_5b166246bdc288a1: done; log hygiene: 1 WARN line(s) in the case's log...
- `desktop.no_console_flash` (3741 ms): daemon pid 8216 answered after 339 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9875 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5
- `desktop.tray_icon` (12097 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (2377 ms): daemon pid 9652 answered after 300 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr7732\60\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN ...
- `regress.encoder_unavailable_wording` (1381 ms): daemon pid 4040 answered after 338 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
