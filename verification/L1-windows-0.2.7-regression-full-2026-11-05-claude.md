# L1 windows 0.2.7 regression report (full)

- **Date:** 2026-11-05T08:23:54Z to 2026-11-05T08:32:09Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\20261105T082349Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen`, tier `full`
- **Result:** 43 passed, 9 failed, 5 skipped of 57 cases
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
| `export.presets` | FAIL | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length: youtube-1080p: tagged space '' range '' by h264_mf |
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
| `rec.screen_cursor` | FAIL | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings: the recording is 7.29 s in the manifest, expected about 6 (3 + 3, the 2 s pause removed) |
| `rec.region` | PASS | a region recording is exactly the requested size |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> |
| `ai.speakers` | SKIP | speaker labels with the speaker models (not driven by this suite yet): needs --whisper-model <ggml file> |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: needs --installer <setup.exe> |
| `desktop.stays_up` | FAIL | cm-desktop starts, stays up for 10 s and leaves no crash event: left programs running: tracked pids [2424] |
| `desktop.second_launch` | FAIL | a second launch with a file forwards it to the running app and exits: 1 other cm-desktop process(es) are still running: the second launch did not hand over |
| `desktop.hotkey_overlay` | FAIL | Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it: left programs running: tracked pids [11092] |
| `desktop.hud_click_through` | FAIL | during a recording the HUD answers clicks and the area window is click-through: left programs running: ffmpeg.exe (3220) tracked pids [15296]; 1 ERROR or panic line(s) in the logs, first: desktop.log: 2026-11-05T08:31:37.1499223Z desktop ERROR gpui-pre-0.3.9/s... |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.tray_icon` | SKIP | the tray icon exists (UI Automation probe): not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
| `regress.encoder_unavailable_wording` | SKIP | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle: the request was refused earlier (platform_rejected), not by the encoder: could not provoke it |
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
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | PASS | `export.audio` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | FAIL | `desktop.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | FAIL | `targets.list`, `desktop.hotkey_overlay` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
| DPI-WIN | FAIL | `desktop.hotkey_overlay` |
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| EXP-2 | FAIL | `export.presets` |
| EXP-3 | FAIL | `export.presets` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | FAIL | `desktop.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | FAIL | `rec.screen_cursor` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | FAIL | `rec.screen_cursor` |
| REC-1.AC | PASS | `rec.region` |
| REC-2 | FAIL | `rec.screen_cursor` |
| REC-3 | FAIL | `rec.screen_cursor` |
| REC-4 | FAIL | `rec.screen_cursor` |
| REC-5 | FAIL | `desktop.hud_click_through` |
| REC-5 tray | not run | `desktop.tray_icon` |
| REC-7 | FAIL | `rec.screen_cursor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN section 5 | not run | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: illegal instruction | FAIL | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 54 % late frames on a static screen | FAIL | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | PASS | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | FAIL | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | FAIL | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | PASS | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | not run | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | FAIL | `rec.screen_cursor`, `zz.crash_events` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | PASS | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | FAIL | `desktop.second_launch` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | FAIL | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

### `daemon.autostart`: the CLI starts a daemon when none runs

- timed out after 135 s

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the h264_mf export is tagged space '' range '', expected bt709 and tv

Evidence:

```text
daemon pid 2224 answered after 489 ms
export --preset quick: clip-h264 - quick.mp4 by h264_mf, 1021 ms
h264 320x240 yuv420p range  space  trc  prim , 3.00 s
```

### `export.presets`: youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length

- youtube-1080p: tagged space '' range '' by h264_mf

Evidence:

```text
daemon pid 7908 answered after 517 ms
export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 9259 ms
youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266260 bytes)
export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 6919 ms
vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667853 bytes)
export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 4902 ms
square-1080: h264 1080x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1631085 bytes)
export --preset webm: clip-h264 - webm.webm by libvpx-vp9, 4385 ms
webm: vp9 320x240 3.01 s matroska,webm fps Some(30.0) (121447 bytes)
export --preset gif: clip-h264 - gif.gif by gif, 11951 ms
gif: gif 800x600 3.00 s gif fps Some(16.666666666666668) (2084503 bytes)
```

### `mcp.confirm_tier`: start_recording and delete_capture do not run without an on-screen confirmation

- start_recording without a confirmation gave Some("not_found") (is_error true)

Evidence:

```text
daemon pid 1856 answered after 492 ms
start_recording: error=true code=Some("not_found") not_found: Display 1 not found
```

### `rec.screen_cursor`: screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings

- the recording is 7.29 s in the manifest, expected about 6 (3 + 3, the 2 s pause removed)

Evidence:

```text
daemon pid 3028 answered after 450 ms
started rec_01a1aad45e24_0001
screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 7.27 s; manifest duration Some(Number(874382)) ticks
```

### `desktop.stays_up`: cm-desktop starts, stays up for 10 s and leaves no crash event

- left programs running: tracked pids [2424]

Evidence:

```text
1 visible window(s) of the app: 'CaptureMann' 1792x992
log hygiene: 12 WARN line(s) in the case's logs
```

### `desktop.second_launch`: a second launch with a file forwards it to the running app and exits

- 1 other cm-desktop process(es) are still running: the second launch did not hand over

Evidence:

```text
second launch: exit 0 (0x00000000) after 105 ms
visible windows of the first instance: 1 before, 1 after
```

### `desktop.hotkey_overlay`: Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it

- left programs running: tracked pids [11092]

Evidence:

```text
hwnd 0x3d076a pid 13608 class 'Window Class' title 'CaptureMann overlay' rect [-3840, -1861, 0, 299] ex 0x40018
hwnd 0x400640 pid 13608 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
log hygiene: 12 WARN line(s) in the case's logs
```

### `desktop.hud_click_through`: during a recording the HUD answers clicks and the area window is click-through

- left programs running: ffmpeg.exe (3220) tracked pids [15296]; 1 ERROR or panic line(s) in the logs, first: desktop.log: 2026-11-05T08:31:37.1499223Z desktop ERROR gpui-pre-0.3.9/src/window.rs:1968: RefCell already borrowed

Evidence:

```text
hwnd 0x3f076a pid 15296 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188
hwnd 0x220480 pid 15296 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8
hwnd 0xb069a pid 15296 class 'Zed::Window' title '' rect [1225, 796, 1907, 1014] ex 0x200188
hwnd 0x10059c pid 15296 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100
WindowFromPoint(1566,905) is the HUD
log hygiene: 12 WARN line(s) in the case's logs
log: desktop.log: 2026-11-05T08:31:37.1499223Z desktop ERROR gpui-pre-0.3.9/src/window.rs:1968: RefCell already borrowed
```

## Not run

- needs --installer <setup.exe> (1): installer.silent
- needs --whisper-model <ggml file> (2): ai.transcribe, ai.speakers
- not implemented: the tray icon lives in the hidden-icons overflow and needs a UI Automation probe; see tests/manual/REC-5.md (1): desktop.tray_icon
- the request was refused earlier (platform_rejected), not by the encoder: could not provoke it (1): regress.encoder_unavailable_wording

## Evidence of passing cases

- `pkg.files` (1319 ms): capturemann.exe: 4556800 bytes; cm-mcp.exe: 8084992 bytes; cm-daemon.exe: 57489920 bytes; cm-desktop.exe: 95506944 bytes; cm-overlay.exe: 2780160 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (2100 ms): capturemann --version: exit 0 (0x00000000) in 73 ms; cm-mcp --bogus: exit 2 (0x00000002) in 76 ms; cm-daemon --bogus: exit 2 (0x00000002) in 90 ms; cm-overlay --help: exit 0 (0x00000000) in 60 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 89 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 91 ms; llama-server --version: exit 0 (0x00000000) in 98 ms
- `pkg.diagnostics` (2392 ms): version 0.2.7; daemon pid 8292 answered after 480 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0); - text recognizer: windows...
- `daemon.lifecycle` (3914 ms): daemon pid 1708 answered after 463 ms; pid 1708, 84 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (4033 ms): daemon pid 15000 answered after 490 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (3460 ms): daemon pid 10028 answered after 472 ms; second daemon: exit 0 (0x00000000) in 98 ms; output: [INFO cm_core::logging] daemon 0.2.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-11964-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `settings.roundtrip` (4328 ms): daemon pid 12080 answered after 935 ms; daemon pid 13860 answered after 537 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (2637 ms): daemon pid 9640 answered after 455 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (2340 ms): daemon pid 8940 answered after 511 ms; display 4327037 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; display 7931565 'S27B80P' 3840x2160 at (-3840,-1861) scale 1.5; 3 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal, cm-desktop; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (3000 ms): daemon pid 3600 answered after 468 ms; display 4327037 1920x1080; PNG 1920x1080; capture shot_06GTNMBQMYFMSC47; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (2606 ms): daemon pid 3060 answered after 472 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (6808 ms): daemon pid 13536 answered after 472 ms; window 5768434: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (2819 ms): daemon pid 2816 answered after 479 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (2432 ms): daemon pid 1912 answered after 483 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (2488 ms): daemon pid 4996 answered after 480 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (3098 ms): daemon pid 11188 answered after 498 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (2281 ms): daemon pid 11212 answered after 482 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (2576 ms): daemon pid 10512 answered after 439 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (7635 ms): daemon pid 2236 answered after 510 ms; window screenshot shot_06GTNMGP90R3P22V: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (9085 ms): daemon pid 3276 answered after 496 ms; clip-h264.mp4: rec_06GTNMHCDQBXZ52Z, 320x240, 3.00 s, job 1006 ms; clip-h264.mov: rec_06GTNMHJ5QRWETR9, 320x240, 3.00 s, job 1059 ms; clip-h264.mkv: rec_06GTNMHR0SXKHAQP, 320x240, 3.02 s, job 3370 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (6101 ms): daemon pid 12640 answered after 550 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (5928 ms): daemon pid 6052 answered after 612 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (3403 ms): daemon pid 8676 answered after 532 ms; rec_06GTNMMAQNQNBTV6: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (4986 ms): daemon pid 10420 answered after 540 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (2861 ms): daemon pid 12932 answered after 554 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (6235 ms): daemon pid 12916 answered after 508 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (3837 ms): daemon pid 2240 answered after 526 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (2551 ms): daemon pid 14980 answered after 513 ms; server capturemann 0.2.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (2425 ms): daemon pid 15000 answered after 516 ms; 77 tools; known drift (spec lists, server lacks): get_cursor_track, export_image, get_platform_rules; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (2749 ms): daemon pid 2968 answered after 493 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 535 chars; get_diagnostics: ok, 3474 chars; list_models: ok, 1774 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1325 chars; list_targets: ok, 400 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (2369 ms): daemon pid 10764 answered after 460 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (2354 ms): daemon pid 14796 answered after 472 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (2462 ms): daemon pid 11188 answered after 490 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (9717 ms): daemon pid 13380 answered after 507 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (2745 ms): daemon pid 3692 answered after 481 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (2735 ms): daemon pid 2768 answered after 478 ms; port 62369: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region` (7654 ms): daemon pid 13404 answered after 548 ms; started rec_01a1aad497fd_0001; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (11418 ms): daemon pid 14768 answered after 478 ms; started rec_01a1aad4b9d8_0001; before the kill: state recording recorded 6.2 s; daemon pid 1420 answered after 474 ms; recovered: 6.97 s of about 6.2 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `ai.no_model` (3993 ms): daemon pid 2980 answered after 482 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (3795 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 5580 answered after 465 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.no_console_flash` (5880 ms): daemon pid 4540 answered after 499 ms; log hygiene: 1 WARN line(s) in the case's logs
- `regress.ffmpeg_missing_wording` (7114 ms): daemon pid 5136 answered after 1475 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr11964\52\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WAR...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
