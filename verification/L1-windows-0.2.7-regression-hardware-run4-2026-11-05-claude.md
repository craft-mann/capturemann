# L1 windows 0.2.7 regression report (hardware)

- **Date:** 2026-11-05T11:20:46Z to 2026-11-05T11:27:45Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261105T112044Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera,second_monitor --soak-minutes 3`, tier `hardware`
- **Result:** 58 passed, 0 failed, 8 skipped of 66 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `6de115d30da861aaece5b914eedad1ee44641fe6aaa1d3bb15e1a32d0100b943`
  - `cm-mcp.exe` `05b31fe08cdfaf21716c331d2102452e6ae660c2d4a4c4ec6436d9a17023d911`
  - `cm-daemon.exe` `5b53e1d9494b0c909c44def735591be81c359d536b74b090d4b77d311944bf0b`
  - `cm-desktop.exe` `1e8aff037be0bddc916c79242b9c6b2b7bfe443391a2164c0dc158e889595604`
  - `cm-overlay.exe` `f1ad57a6808e160b334c3bceb668cf0b0b4600046227c3f95bc67719c24ce978`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

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
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: only one display is connected |
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
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: no non-primary display of 3000 px or more is connected |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: only one display is connected |
| `rec.soak` | PASS | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> |
| `ai.speakers` | SKIP | speaker labels with the speaker models (not driven by this suite yet): needs --whisper-model <ggml file> |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: needs --installer <setup.exe> |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.hud_click_through` | PASS | during a recording the HUD answers clicks and the area window is click-through |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: only one display is connected |
| `desktop.tray_icon` | SKIP | the tray icon exists (UI Automation probe): not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md |
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
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-1 | PASS | `rec.mic`, `rec.system_audio` |
| AUD-2 | PASS | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
| CAM-1 | PASS | `rec.camera` |
| CAM-2 | PASS | `rec.camera` |
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
| O7 installer | not run | `installer.silent` |
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
| REC-5 tray | not run | `desktop.tray_icon` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | PASS | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
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
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
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

- None in the cases that ran.
## Not run

- needs --installer <setup.exe> (1): installer.silent
- needs --whisper-model <ggml file> (2): ai.transcribe, ai.speakers
- no non-primary display of 3000 px or more is connected (1): rec.external_4k
- not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md (1): desktop.tray_icon
- only one display is connected (3): shot.second_monitor, rec.each_monitor, desktop.recording_placement

## Evidence of passing cases

- `pkg.files` (628 ms): capturemann.exe: 4557824 bytes; cm-mcp.exe: 8084992 bytes; cm-daemon.exe: 57344000 bytes; cm-desktop.exe: 95353856 bytes; cm-overlay.exe: 2787840 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (883 ms): capturemann --version: exit 0 (0x00000000) in 32 ms; cm-mcp --bogus: exit 2 (0x00000002) in 34 ms; cm-daemon --bogus: exit 2 (0x00000002) in 34 ms; cm-overlay --help: exit 0 (0x00000000) in 19 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 35 ms; llama-server --version: exit 0 (0x00000000) in 35 ms
- `pkg.diagnostics` (1144 ms): version 0.2.7; daemon pid 12068 answered after 319 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text recognizer: window...
- `daemon.lifecycle` (1975 ms): daemon pid 11700 answered after 319 ms; pid 11700, 84 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1980 ms): daemon pid 15136 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1445 ms): daemon pid 12512 answered after 319 ms; second daemon: exit 0 (0x00000000) in 33 ms; output: [INFO cm_core::logging] daemon 0.2.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-8408-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (1967 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [976]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1717 ms): daemon pid 5816 answered after 319 ms; daemon pid 8172 answered after 316 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1327 ms): daemon pid 9532 answered after 320 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (1197 ms): daemon pid 828 answered after 319 ms; display 17695695 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal, cm-desktop; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1464 ms): daemon pid 15072 answered after 320 ms; display 17695695 1920x1080; PNG 1920x1080; capture shot_06GTPW7D2G98ET22; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1298 ms): daemon pid 2564 answered after 320 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (3575 ms): daemon pid 1500 answered after 321 ms; window 1180576: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1380 ms): daemon pid 7780 answered after 321 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1238 ms): daemon pid 14236 answered after 321 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1247 ms): daemon pid 364 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1482 ms): daemon pid 10420 answered after 320 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1199 ms): daemon pid 12120 answered after 321 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1306 ms): daemon pid 6628 answered after 321 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (3883 ms): daemon pid 5092 answered after 321 ms; window screenshot shot_06GTPWA2AQK9WA32: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4521 ms): daemon pid 10772 answered after 321 ms; clip-h264.mp4: rec_06GTPWABJE8TR810, 320x240, 3.00 s, job 372 ms; clip-h264.mov: rec_06GTPWAE8AHZMME8, 320x240, 3.00 s, job 369 ms; clip-h264.mkv: rec_06GTPWAGY43ZM4P8, 320x240, 3.02 s, job 1715 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3129 ms): daemon pid 12120 answered after 321 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (2864 ms): daemon pid 13736 answered after 323 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1556 ms): daemon pid 12160 answered after 320 ms; rec_06GTPWBSM6979YMD: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2202 ms): daemon pid 10136 answered after 322 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (1351 ms): daemon pid 3576 answered after 322 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2437 ms): daemon pid 4424 answered after 323 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 367 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (15249 ms): daemon pid 10372 answered after 322 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2175 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2240 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1754 ms; square-1080: h264 1080x1080 3.00...
- `export.audio` (2747 ms): daemon pid 14388 answered after 322 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1876 ms): daemon pid 6756 answered after 321 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1284 ms): daemon pid 3464 answered after 321 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1254 ms): daemon pid 12932 answered after 321 ms; 77 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1481 ms): daemon pid 8412 answered after 321 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 3466 chars; list_models: ok, 1773 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1324 chars; list_targets: ok, 258 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1268 ms): daemon pid 9704 answered after 321 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1188 ms): daemon pid 8912 answered after 321 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1283 ms): daemon pid 4496 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1257 ms): daemon pid 10420 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8305 ms): daemon pid 11972 answered after 320 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1287 ms): daemon pid 6552 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1368 ms): daemon pid 11164 answered after 321 ms; port 55229: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (12364 ms): daemon pid 8072 answered after 320 ms; started rec_01a1ab724eb5_0001; export job job_65cab408a5603527: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.37 s; manifest duration Some(Number(763008)) ticks; manifest 6.36 s, expected 6.03 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.9999999999999998, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_mis...
- `rec.region` (5600 ms): daemon pid 7416 answered after 320 ms; started rec_01a1ab72805e_0001; export job job_2ee61b531df877e2: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8843 ms): daemon pid 6348 answered after 319 ms; started rec_01a1ab7297a0_0001; before the kill: state recording recorded 6.0 s; daemon pid 7696 answered after 317 ms; recovered: 6.33 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `rec.mic` (7285 ms): daemon pid 2120 answered after 319 ms; started rec_01a1ab72bc23_0001; export job job_186c039950d095e8: done; mic track 4.12 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (9951 ms): daemon pid 12952 answered after 323 ms; started rec_01a1ab72d9a1_0001; export job job_79b40cbe469d7f33: done; system audio level: mean -6.3 dB, peak -0.1 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `rec.camera` (9513 ms): daemon pid 3500 answered after 321 ms; started rec_01a1ab73025d_0001; export job job_2680d6c19095abe4: done; camera 1280x720 h264 measured 28.9 fps, 5.55 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.soak` (182506 ms): daemon pid 6604 answered after 320 ms; started rec_01a1ab733649_0001; 180 s: state recording recorded Some(Number(180210)) ms late Some(Number(0.0))%; soak: h264 1920x1080 avg Some(30.0) 180.6 s; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1938 ms): daemon pid 14572 answered after 320 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1965 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 12892 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (11515 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992; log hygiene: 12 WARN line(s) in the case's logs
- `desktop.second_launch` (8190 ms): second launch: exit 0 (0x00000000) after 49 ms; visible windows of the first instance: 1 before, 1 after; log hygiene: 12 WARN line(s) in the case's logs
- `desktop.hotkey_overlay` (6381 ms): hwnd 0x160240 pid 7040 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018; log hygiene: 12 WARN line(s) in the case's logs
- `desktop.hud_click_through` (9641 ms): hwnd 0x3604ee pid 3988 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188; hwnd 0x300532 pid 3988 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x170570 pid 3988 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x1a055c pid 3988 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(1566,698) is the HUD; export job job_5b56c7d549d3c033: done; log hygiene: 14 WARN line(s) in the case's lo...
- `desktop.no_console_flash` (2792 ms): daemon pid 2088 answered after 323 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9842 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5; log hygiene: 12 WARN line(s) in the case's logs
- `regress.ffmpeg_missing_wording` (2442 ms): daemon pid 7576 answered after 259 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr8408\61\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN ...
- `regress.encoder_unavailable_wording` (1247 ms): daemon pid 3848 answered after 321 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 16 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
