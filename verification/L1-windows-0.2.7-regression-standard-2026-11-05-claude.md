# L1 windows 0.2.7 regression report (standard)

- **Date:** 2026-11-05T08:17:29Z to 2026-11-05T08:22:36Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out C:\lab\regress-out\20261105T081726Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen`, tier `standard`
- **Result:** 41 passed, 6 failed, 1 skipped of 48 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `4ce16c07d21d28a65e73c780850ed5072d18cd90a4738bc21ad2efe2c8150518`
  - `cm-mcp.exe` `c8b5462820c2a2ab65e25704328f001f82797de77a0ef4500a12b0ea03b95e57`
  - `cm-daemon.exe` `5c87af51434d35fab0b07a9afb66050e80d6f4c045dfb6678a841e85feb67e09`
  - `cm-desktop.exe` `38445bd66ccd7727ed9918774c8c37de2d1f8eda649f75b1b826d642194574e0`
  - `cm-overlay.exe` `102c291228b4b3fbfc1813e02895e38dfbd70e2d476a6222a11608e49ef358df`
  - `ffmpeg.exe` `08d073edcbbbf6b8c3c11efb9a78a8f21af9c555da86d815fa2fbcb0c2d8d609`
  - `ffprobe.exe` `e09a5180f022cc05bab32a8ba3aed0d54e642cb81a9952a57aa24a121e46102f`
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
| `daemon.autostart` | FAIL | the CLI starts a daemon when none runs: timed out after 135 s |
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
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder |
| `import.audio` | PASS | a WAV file imports as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: the h264_mf export is tagged space '' range '', expected bt709 and tv |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.errors` | PASS | an unknown preset or capture is refused with the shared error codes |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | FAIL | start_recording and delete_capture do not run without an on-screen confirmation: start_recording without a confirmation gave Some("not_found") (is_error true) |
| `mcp.policy_allow` | PASS | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | FAIL | cm-desktop starts, stays up for 10 s and leaves no crash event: left programs running: tracked pids [4036] |
| `desktop.second_launch` | FAIL | a second launch with a file forwards it to the running app and exits: 1 other cm-desktop process(es) are still running: the second launch did not hand over |
| `desktop.hotkey_overlay` | FAIL | Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it: left programs running: tracked pids [4884] |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.tray_icon` | SKIP | the tray icon exists (UI Automation probe): not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged by Windows during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | FAIL | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `mcp.errors` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | PASS | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo` |
| AGT-POLICY | FAIL | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | PASS | `export.audio` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | FAIL | `desktop.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | FAIL | `targets.list`, `desktop.hotkey_overlay` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
| DPI-WIN | FAIL | `desktop.hotkey_overlay` |
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | FAIL | `desktop.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | PASS | `pkg.files` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-5 tray | not run | `desktop.tray_icon` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: illegal instruction | FAIL | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | PASS | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | FAIL | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | PASS | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | PASS | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | FAIL | `desktop.second_launch` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | FAIL | `mcp.confirm_tier` |

## Problems found

### `daemon.autostart`: the CLI starts a daemon when none runs

- timed out after 135 s

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the h264_mf export is tagged space '' range '', expected bt709 and tv

Evidence:

```text
daemon pid 1272 answered after 322 ms
export --preset quick: clip-h264 - quick.mp4 by h264_mf, 340 ms
h264 320x240 yuv420p range  space  trc  prim , 3.00 s
```

### `mcp.confirm_tier`: start_recording and delete_capture do not run without an on-screen confirmation

- start_recording without a confirmation gave Some("not_found") (is_error true)

Evidence:

```text
daemon pid 13004 answered after 321 ms
start_recording: error=true code=Some("not_found") not_found: Display 1 not found
```

### `desktop.stays_up`: cm-desktop starts, stays up for 10 s and leaves no crash event

- left programs running: tracked pids [4036]

Evidence:

```text
1 visible window(s) of the app: 'CaptureMann' 1792x992
log hygiene: 12 WARN line(s) in the case's logs
```

### `desktop.second_launch`: a second launch with a file forwards it to the running app and exits

- 1 other cm-desktop process(es) are still running: the second launch did not hand over

Evidence:

```text
second launch: exit 0 (0x00000000) after 114 ms
visible windows of the first instance: 1 before, 1 after
```

### `desktop.hotkey_overlay`: Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it

- left programs running: tracked pids [4884]

Evidence:

```text
hwnd 0x22068a pid 9360 class 'Window Class' title 'CaptureMann overlay' rect [-3840, -1861, 0, 299] ex 0x40018
hwnd 0x8b00e0 pid 9360 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
log hygiene: 12 WARN line(s) in the case's logs
```

## Not run

- not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md (1): desktop.tray_icon

## Evidence of passing cases

- `pkg.files` (628 ms): capturemann.exe: 4556800 bytes; cm-mcp.exe: 8084992 bytes; cm-daemon.exe: 57489920 bytes; cm-desktop.exe: 95506944 bytes; cm-overlay.exe: 2780160 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (881 ms): capturemann --version: exit 0 (0x00000000) in 32 ms; cm-mcp --bogus: exit 2 (0x00000002) in 34 ms; cm-daemon --bogus: exit 2 (0x00000002) in 35 ms; cm-overlay --help: exit 0 (0x00000000) in 19 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 35 ms; llama-server --version: exit 0 (0x00000000) in 35 ms
- `pkg.diagnostics` (1156 ms): version 0.2.7; daemon pid 11168 answered after 319 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text recognizer: window...
- `daemon.lifecycle` (1977 ms): daemon pid 11216 answered after 320 ms; pid 11216, 84 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1985 ms): daemon pid 14120 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1725 ms): daemon pid 2044 answered after 320 ms; second daemon: exit 0 (0x00000000) in 33 ms; output: [INFO cm_core::logging] daemon 0.2.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-1804-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `settings.roundtrip` (1746 ms): daemon pid 6152 answered after 319 ms; daemon pid 6940 answered after 317 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1251 ms): daemon pid 15100 answered after 319 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (1124 ms): daemon pid 3232 answered after 320 ms; display 4327037 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; display 7931565 'S27B80P' 3840x2160 at (-3840,-1861) scale 1.5; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal, cm-desktop; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1322 ms): daemon pid 8264 answered after 319 ms; display 4327037 1920x1080; PNG 1920x1080; capture shot_06GTNJSA22EGJH1F; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1212 ms): daemon pid 11016 answered after 320 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (3515 ms): daemon pid 4388 answered after 319 ms; window 2557520: listed 1426x720, saved 1426x720; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1400 ms): daemon pid 5392 answered after 322 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1286 ms): daemon pid 6612 answered after 321 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1305 ms): daemon pid 13056 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1504 ms): daemon pid 13116 answered after 321 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1257 ms): daemon pid 5464 answered after 320 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1385 ms): daemon pid 14400 answered after 322 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (4031 ms): daemon pid 10304 answered after 321 ms; window screenshot shot_06GTNJVREYGVG354: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4587 ms): daemon pid 4820 answered after 324 ms; clip-h264.mp4: rec_06GTNJW2D3BSACX9, 320x240, 3.00 s, job 396 ms; clip-h264.mov: rec_06GTNJW559WEX3TH, 320x240, 3.00 s, job 398 ms; clip-h264.mkv: rec_06GTNJW7TWFEPAJD, 320x240, 3.02 s, job 1662 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3225 ms): daemon pid 7876 answered after 321 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (2974 ms): daemon pid 7516 answered after 337 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1589 ms): daemon pid 4044 answered after 322 ms; rec_06GTNJXHQ1NJYSRJ: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2237 ms): daemon pid 5700 answered after 323 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) So...
- `import.dry_run` (1427 ms): daemon pid 13892 answered after 322 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (2829 ms): daemon pid 8452 answered after 322 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1928 ms): daemon pid 10976 answered after 322 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1329 ms): daemon pid 12524 answered after 320 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1285 ms): daemon pid 11440 answered after 322 ms; 77 tools; known drift (spec lists, server lacks): get_cursor_track, export_image, get_platform_rules; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1463 ms): daemon pid 15096 answered after 320 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 3466 chars; list_models: ok, 1773 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1324 chars; list_targets: ok, 400 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1296 ms): daemon pid 7408 answered after 320 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1310 ms): daemon pid 11796 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1299 ms): daemon pid 15100 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (9305 ms): daemon pid 3420 answered after 322 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (2982 ms): daemon pid 176 answered after 501 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (2838 ms): daemon pid 9672 answered after 477 ms; port 62354: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (4909 ms): daemon pid 3960 answered after 536 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (4141 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 4160 answered after 489 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.no_console_flash` (6007 ms): daemon pid 8172 answered after 494 ms; log hygiene: 1 WARN line(s) in the case's logs
- `regress.ffmpeg_missing_wording` (9206 ms): daemon pid 13588 answered after 3288 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr1804\47\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WAR...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
