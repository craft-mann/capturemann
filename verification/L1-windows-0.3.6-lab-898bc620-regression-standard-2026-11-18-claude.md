# L1 windows 0.3.6 regression report (standard)

- **Date:** 2026-11-18T10:07:10Z to 2026-11-18T10:10:03Z
- **Who:** Claude QA agent
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 898bc620, package folder `C:\lab\q36\pkg`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out C:\lab\regress-out\q36-standard --who Claude QA agent --verdict PARTIAL --allow screen --package-dir C:\lab\q36\pkg --commit 898bc620 --whisper-model C:\Users\trung\AppData\Roaming\CaptureMann\ai\models\ggml-medium.bin`, tier `standard`
- **Result:** 52 passed, 5 failed, 7 skipped of 64 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `099a30143872f365ddd94211ec4d403729b179c27989c2b842f23912643cf1be`
  - `cm-mcp.exe` `3c5a3b0bbf48119682e4361647460033d19089f29de251cd15f038d0bee2d229`
  - `cm-daemon.exe` `2530a1495956b7c36075c4acc253b8c72ce7e86d8f44e6edfdba4d81d44d1af3`
  - `cm-desktop.exe` `adc4d802080bbed9a75b319e283db80c3850650ac88733cd3c9857961b1009f0`
  - `cm-overlay.exe` `17726f1c0adff6db2d1b5028a8b88a5b9aadce323dd1580ecdb0462cf3a777d6`
  - `ffmpeg.exe` `08d073edcbbbf6b8c3c11efb9a78a8f21af9c555da86d815fa2fbcb0c2d8d609`
  - `ffprobe.exe` `e09a5180f022cc05bab32a8ba3aed0d54e642cb81a9952a57aa24a121e46102f`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: llama-server.exe is missing (the windows package ships the text engine); no LICENSE file; no THIRD_PARTY_LICENSES file; FFmpeg/SOURCE-OFFER.txt is missing (LGPL source offer); FFmpeg/BUILD-IN... |
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
| `import.audio` | FAIL | a WAV file and an MP3 file import as an audio capture of the right length: import of C:\Users\trung\AppData\Local\Temp\cmr2152\28\tone.mp3 printed no job (exit 2 (0x00000002)): {"error":{"code":"invalid_input","message":"The MP3 file can't be read. It may be d... |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `guide.edit_export` | PASS | a hand-written guide: get, reorder, retitle, undo and redo are one step each; Markdown (relative links, PNGs in order), HTML (self-contained) and PDF export |
| `guide.refusals` | PASS | an unknown guide is not_found, a bad numbering or format is refused, and a start on a display that does not exist leaves no guide in the library |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: the quick export: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709) |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.estimate` | PASS | export --estimate prints About N MB, up to M MB with codec and pixels; a GIF has none; the refusals are the shared ones |
| `export.estimate_mcp` | PASS | the MCP tool estimate_export answers the same bound and typical size as the CLI |
| `export.errors` | PASS | an unknown preset or capture is refused with the shared error codes |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | FAIL | tools/list matches the tool table of the spec (no drift either way): tools the spec table does not list: list_presets |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | PASS | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | FAIL | the bundled llama-server runs --version and is not started by the daemon: the package has no llama-server |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for windows |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: not implemented for windows |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: not implemented for windows |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for windows |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: not implemented for windows |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: not implemented for windows |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: not implemented for windows |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | PASS | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `export.estimate_mcp`, `mcp.errors`, `desktop.settings_applied` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | FAIL | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack`, `board.create`, `bulk.export_images`, `guide.refusals` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model` |
| AI-7 | FAIL | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
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
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | PASS | `desktop.hotkey_overlay` |
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| GDE-1.AC5 | PASS | `guide.refusals` |
| GDE-2 | PASS | `guide.edit_export` |
| GDE-3 | PASS | `guide.edit_export` |
| GDE-3.AC1 | PASS | `guide.edit_export` |
| GDE-4 | PASS | `guide.edit_export`, `guide.refusals` |
| IMP-1 | FAIL | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch`, `linux.second_launch` |
| IMP-6 | PASS | `import.images` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | not run | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | FAIL | `pkg.files` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | FAIL | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | not run | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | FAIL | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | FAIL | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | PASS | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | PASS | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | PASS | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | PASS | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | PASS | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | PASS | `desktop.second_launch` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | FAIL | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- llama-server.exe is missing (the windows package ships the text engine); no LICENSE file; no THIRD_PARTY_LICENSES file; FFmpeg/SOURCE-OFFER.txt is missing (LGPL source offer); FFmpeg/BUILD-INFO.txt is missing (LGPL source offer)

Evidence:

```text
capturemann.exe: 5615104 bytes
cm-mcp.exe: 12380160 bytes
cm-daemon.exe: 48888832 bytes
cm-desktop.exe: 93635072 bytes
cm-overlay.exe: 4628992 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

### `import.audio`: a WAV file and an MP3 file import as an audio capture of the right length

- import of C:\Users\trung\AppData\Local\Temp\cmr2152\28\tone.mp3 printed no job (exit 2 (0x00000002)): {"error":{"code":"invalid_input","message":"The MP3 file can't be read. It may be damaged or cut short.","data":{"reason":"unreadable_file","files":[{"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr2152\\28\\tone.mp3","name":"tone.mp3","size_bytes":8396,"title":"tone","kind":"video","status":"can...

Evidence:

```text
daemon pid 6272 answered after 320 ms
rec_06GYW875H4B0YX6T: kind Some(String("audio")), 2.00 s
```

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the quick export: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709)

Evidence:

```text
daemon pid 14788 answered after 321 ms
export --preset quick: clip-h264 - quick.mp4 by h264_mf, 316 ms
h264 320x240 yuv420p range tv space bt709 trc  prim , 3.00 s
```

### `mcp.tools_list`: tools/list matches the tool table of the spec (no drift either way)

- tools the spec table does not list: list_presets

Evidence:

```text
daemon pid 12560 answered after 320 ms
90 tools
```

### `ai.llama_version`: the bundled llama-server runs --version and is not started by the daemon

- the package has no llama-server

## Not run

- not implemented for windows (7): linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder

## Evidence of passing cases

- `pkg.startup` (871 ms): capturemann --version: exit 0 (0x00000000) in 32 ms; cm-mcp --bogus: exit 2 (0x00000002) in 34 ms; cm-daemon --bogus: exit 2 (0x00000002) in 49 ms; cm-overlay --help: exit 0 (0x00000000) in 35 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 32 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 35 ms
- `pkg.diagnostics` (1137 ms): version 0.3.6; daemon pid 12784 answered after 319 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9818, vendor id 0x1414; - ffmpeg at C:\lab\q36\pkg\ffmpeg.exe; - llama-server not found: looked next to C:\lab\q36\pkg, in the settings and on PATH; this build should include it; - text recognizer: windows_ocr (available; reads...
- `daemon.lifecycle` (1930 ms): daemon pid 2692 answered after 319 ms; pid 2692, 101 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1942 ms): daemon pid 15764 answered after 317 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1443 ms): daemon pid 9788 answered after 319 ms; second daemon: exit 0 (0x00000000) in 48 ms; output: [INFO cm_core::logging] daemon 0.3.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-2152-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2030 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [14344]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1697 ms): daemon pid 1992 answered after 320 ms; daemon pid 3304 answered after 316 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1238 ms): daemon pid 7220 answered after 319 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (1113 ms): daemon pid 15500 answered after 321 ms; display 65537 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 2 windows; capabilities {"list_windows":true,"capture_window":true,"capture_region":true,"pick":false}; apps: Microsoft Outlook, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1520 ms): daemon pid 14820 answered after 319 ms; display 65537 1920x1080; PNG 1920x1080; capture shot_06GYW81JXX259PVS; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1250 ms): daemon pid 12912 answered after 319 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (5797 ms): daemon pid 14364 answered after 320 ms; window 2033060: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1307 ms): daemon pid 1540 answered after 320 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1173 ms): daemon pid 7740 answered after 320 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1188 ms): daemon pid 9896 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1388 ms): daemon pid 14660 answered after 320 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1133 ms): daemon pid 13428 answered after 319 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1240 ms): daemon pid 7752 answered after 319 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (5242 ms): daemon pid 6132 answered after 320 ms; window screenshot shot_06GYW84D8713JNNF: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (1397 ms): daemon pid 11580 answered after 318 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","message":"ops[1]: only one spotlight per image: add ...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (2001 ms): daemon pid 5612 answered after 320 ms; generated.png: shot_06GYW84W71EVZ0E9, original.png 120x80, source {"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr2152\\22\\images\\generated.png","imported_at":"2026-11-18T10:07:56.9930385Z","original_name":"generated.png"}; shot.jpg: shot_06GYW84XY3ZZPXZV, original.png 1000x260, source {"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr2152\\22\\images\\shot.jpg","imported_at":"2026-11-18T10:07:57.4354619Z","original_name":"shot.jpg"}; GIF named .png...
- `board.create` (1898 ms): daemon pid 11656 answered after 319 ms; Pair board shot_06GYW857W9V4BR9Z: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2884 ms): daemon pid 13220 answered after 321 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4118 ms): daemon pid 16132 answered after 320 ms; clip-h264.mp4: rec_06GYW85V7V00V7MT, 320x240, 3.00 s, job 302 ms; clip-h264.mov: rec_06GYW85XSCK3A8GB, 320x240, 3.00 s, job 349 ms; clip-h264.mkv: rec_06GYW860DC6CTWQE, 320x240, 3.02 s, job 1537 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (2795 ms): daemon pid 14084 answered after 319 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (2799 ms): daemon pid 14380 answered after 320 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1999 ms): daemon pid 15596 answered after 320 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"C:\\Users\\trung\\AppData\...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (1288 ms): daemon pid 14736 answered after 320 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "C:\\Users\\trung\\AppData\\Local\\Temp\\cmr2152\\30\\clips\\clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "read...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (1685 ms): daemon pid 14724 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (1344 ms): daemon pid 14184 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (2631 ms): daemon pid 10148 answered after 319 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1921 ms): daemon pid 16248 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1833 ms): daemon pid 8104 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1792 ms): daemon pid 10904 answered after 319 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1234 ms): daemon pid 14608 answered after 319 ms; server capturemann 0.3.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1401 ms): daemon pid 9852 answered after 319 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 3836 chars; list_models: ok, 1780 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1339 chars; list_targets: ok, 255 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1208 ms): daemon pid 5424 answered after 319 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1205 ms): daemon pid 2328 answered after 319 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1203 ms): daemon pid 7712 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1200 ms): daemon pid 10012 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8276 ms): daemon pid 11916 answered after 321 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1275 ms): daemon pid 15636 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1351 ms): daemon pid 14716 answered after 319 ms; port 52663: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1992 ms): daemon pid 12436 answered after 319 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","message":"No speech model is installed, so nothing can be transcribed. Open the AI page in Captur...
- `desktop.stays_up` (12244 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (9943 ms): second launch: exit 0 (0x00000000) after 51 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (7193 ms): hwnd 0x5103e2 pid 11572 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.no_console_flash` (2618 ms): daemon pid 10376 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9743 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.tray_icon` (11841 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (1907 ms): daemon pid 10652 answered after 479 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr2152\56\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)","data":null,"exit_code":2}}; log hygiene: 1 WARN...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
