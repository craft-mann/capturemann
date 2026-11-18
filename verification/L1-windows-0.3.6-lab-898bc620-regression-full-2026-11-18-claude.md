# L1 windows 0.3.6 regression report (full)

- **Date:** 2026-11-18T10:10:07Z to 2026-11-18T10:22:22Z
- **Who:** Claude QA agent
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit 898bc620, package folder `C:\lab\q36\pkg`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out C:\lab\regress-out\q36-full --who Claude QA agent --verdict PARTIAL --allow screen --package-dir C:\lab\q36\pkg --commit 898bc620 --whisper-model C:\Users\trung\AppData\Roaming\CaptureMann\ai\models\ggml-medium.bin`, tier `full`
- **Result:** 60 passed, 6 failed, 14 skipped of 80 cases
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
| `import.audio` | FAIL | a WAV file and an MP3 file import as an audio capture of the right length: import of C:\Users\trung\AppData\Local\Temp\cmr13940\28\tone.mp3 printed no job (exit 2 (0x00000002)): {"error":{"code":"invalid_input","message":"The MP3 file can't be read. It may be ... |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `guide.edit_export` | PASS | a hand-written guide: get, reorder, retitle, undo and redo are one step each; Markdown (relative links, PNGs in order), HTML (self-contained) and PDF export |
| `guide.refusals` | PASS | an unknown guide is not_found, a bad numbering or format is refused, and a start on a display that does not exist leaves no guide in the library |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: the quick export: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709) |
| `export.presets` | FAIL | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length: youtube-1080p: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709); vertical-1080: h264_mf export is badly tagged:... |
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
| `rec.screen_cursor` | PASS | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings |
| `rec.region` | PASS | a region recording is exactly the requested size |
| `rec.region_min` | PASS | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | FAIL | the bundled llama-server runs --version and is not started by the daemon: the package has no llama-server |
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
| AGT-2 | FAIL | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | PASS | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo`, `annotate.pack`, `board.create`, `bulk.export_images`, `guide.refusals` |
| AGT-POLICY | PASS | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | PASS | `ai.no_model`, `ai.transcribe` |
| AI-3 | PASS | `ai.transcribe` |
| AI-7 | FAIL | `ai.no_model`, `ai.llama_version` |
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
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| EXP-2 | FAIL | `export.presets` |
| EXP-3 | FAIL | `export.presets` |
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
| LINUX-REC-5 | not run | `desktop.recording_banner` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | PASS | `rec.screen_cursor` |
| NFR-licence | FAIL | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | FAIL | `pkg.files` |
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
| VOC-2 | FAIL | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | PASS | `desktop.hud_drag` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | FAIL | `ai.llama_version` |
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
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
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
| specs/02-agent-interface.md tool table | FAIL | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

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

- import of C:\Users\trung\AppData\Local\Temp\cmr13940\28\tone.mp3 printed no job (exit 2 (0x00000002)): {"error":{"code":"invalid_input","message":"The MP3 file can't be read. It may be damaged or cut short.","data":{"reason":"unreadable_file","files":[{"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr13940\\28\\tone.mp3","name":"tone.mp3","size_bytes":8396,"title":"tone","kind":"video","status":"ca...

Evidence:

```text
daemon pid 10480 answered after 319 ms
rec_06GYW8WY63Z0NQWY: kind Some(String("audio")), 2.00 s
```

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the quick export: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709)

Evidence:

```text
daemon pid 2860 answered after 319 ms
export --preset quick: clip-h264 - quick.mp4 by h264_mf, 390 ms
h264 320x240 yuv420p range tv space bt709 trc  prim , 3.00 s
```

### `export.presets`: youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length

- youtube-1080p: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709); vertical-1080: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709); square-1080: h264_mf export is badly tagged: color_primaries '' (expected bt709), color_transfer '' (expected bt709)

Evidence:

```text
daemon pid 12984 answered after 322 ms
export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2781 ms
youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266279 bytes)
export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 1961 ms
vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667872 bytes)
export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1549 ms
square-1080: h264 1080x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1631104 bytes)
export --preset webm: clip-h264 - webm.webm by libvpx-vp9, 1317 ms
webm: vp9 320x240 3.01 s matroska,webm fps Some(30.0) (121979 bytes)
export --preset gif: clip-h264 - gif.gif by gif, 3835 ms
gif: gif 800x600 3.00 s gif fps Some(16.666666666666668) (2084503 bytes)
```

### `mcp.tools_list`: tools/list matches the tool table of the spec (no drift either way)

- tools the spec table does not list: list_presets

Evidence:

```text
daemon pid 2436 answered after 320 ms
90 tools
```

### `ai.llama_version`: the bundled llama-server runs --version and is not started by the daemon

- the package has no llama-server

## Not run

- needs --allow second_monitor (1): desktop.recording_placement
- needs --installer <setup.exe> (1): installer.silent
- no --speech <two-voice recording> given (1): ai.speakers
- not implemented for windows (11): installer.dmg, desktop.recording_banner, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install

## Evidence of passing cases

- `pkg.startup` (911 ms): capturemann --version: exit 0 (0x00000000) in 32 ms; cm-mcp --bogus: exit 2 (0x00000002) in 35 ms; cm-daemon --bogus: exit 2 (0x00000002) in 51 ms; cm-overlay --help: exit 0 (0x00000000) in 35 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 32 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 35 ms
- `pkg.diagnostics` (1164 ms): version 0.3.6; daemon pid 3540 answered after 319 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9818, vendor id 0x1414; - ffmpeg at C:\lab\q36\pkg\ffmpeg.exe; - llama-server not found: looked next to C:\lab\q36\pkg, in the settings and on PATH; this build should include it; - text recognizer: windows_ocr (available; reads ...
- `daemon.lifecycle` (1947 ms): daemon pid 12656 answered after 320 ms; pid 12656, 101 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1984 ms): daemon pid 6520 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1568 ms): daemon pid 13884 answered after 318 ms; second daemon: exit 0 (0x00000000) in 49 ms; output: [INFO cm_core::logging] daemon 0.3.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-13940-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2239 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [10416]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1729 ms): daemon pid 12592 answered after 319 ms; daemon pid 12920 answered after 316 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1483 ms): daemon pid 7664 answered after 321 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (1132 ms): daemon pid 9512 answered after 320 ms; display 65537 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; 2 windows; capabilities {"list_windows":true,"capture_window":true,"capture_region":true,"pick":false}; apps: Microsoft Outlook, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1454 ms): daemon pid 15108 answered after 320 ms; display 65537 1920x1080; PNG 1920x1080; capture shot_06GYW8QEY4RDNG67; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1231 ms): daemon pid 13008 answered after 320 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (4961 ms): daemon pid 7252 answered after 319 ms; window 656904: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1285 ms): daemon pid 2188 answered after 319 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1164 ms): daemon pid 8052 answered after 320 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1188 ms): daemon pid 14600 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1387 ms): daemon pid 7752 answered after 319 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1129 ms): daemon pid 3460 answered after 321 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1242 ms): daemon pid 11044 answered after 319 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (5257 ms): daemon pid 8900 answered after 319 ms; window screenshot shot_06GYW8T4TT7DJ2RM: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (1375 ms): daemon pid 8280 answered after 319 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","message":"ops[1]: only one spotlight per image: add ...; log hygiene: 1 WARN line(s) in the case's l...
- `import.images` (2023 ms): daemon pid 11228 answered after 320 ms; generated.png: shot_06GYW8TKP53B9EHJ, original.png 120x80, source {"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr13940\\22\\images\\generated.png","imported_at":"2026-11-18T10:10:55.0363189Z","original_name":"generated.png"}; shot.jpg: shot_06GYW8TNCY297BX3, original.png 1000x260, source {"path":"C:\\Users\\trung\\AppData\\Local\\Temp\\cmr13940\\22\\images\\shot.jpg","imported_at":"2026-11-18T10:10:55.4747013Z","original_name":"shot.jpg"}; GIF named ....
- `board.create` (1889 ms): daemon pid 12140 answered after 321 ms; Pair board shot_06GYW8TZF696V4D4: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2887 ms): daemon pid 740 answered after 320 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4117 ms): daemon pid 15384 answered after 321 ms; clip-h264.mp4: rec_06GYW8VJRYAY4QDG, 320x240, 3.00 s, job 303 ms; clip-h264.mov: rec_06GYW8VNB89W0A90, 320x240, 3.00 s, job 301 ms; clip-h264.mkv: rec_06GYW8VQX9XJAJM4, 320x240, 3.02 s, job 1511 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (2843 ms): daemon pid 12156 answered after 321 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3037 ms): daemon pid 14380 answered after 320 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2021 ms): daemon pid 4536 answered after 322 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"C:\\Users\\trung\\AppData\...; missing file: exit 2 (0x00000002) So...
- `import.dry_run` (1293 ms): daemon pid 4144 answered after 320 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "C:\\Users\\trung\\AppData\\Local\\Temp\\cmr13940\\30\\clips\\clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "rea...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (1687 ms): daemon pid 7076 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (1503 ms): daemon pid 14004 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (2656 ms): daemon pid 4616 answered after 321 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1895 ms): daemon pid 5280 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1822 ms): daemon pid 12104 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1811 ms): daemon pid 10104 answered after 320 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1235 ms): daemon pid 13524 answered after 319 ms; server capturemann 0.3.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1438 ms): daemon pid 6640 answered after 320 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 535 chars; get_diagnostics: ok, 3844 chars; list_models: ok, 1781 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1340 chars; list_targets: ok, 255 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1227 ms): daemon pid 11472 answered after 319 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1215 ms): daemon pid 16024 answered after 319 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1225 ms): daemon pid 12140 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1223 ms): daemon pid 15624 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8289 ms): daemon pid 3308 answered after 320 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1267 ms): daemon pid 10812 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1342 ms): daemon pid 15868 answered after 318 ms; port 52706: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (12347 ms): daemon pid 1364 answered after 318 ms; started rec_01a1ee248c9a_0001; export job job_7f815164ca020809: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.33 s; manifest duration Some(Number(761774)) ticks; manifest 6.35 s, expected 6.03 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.9999999999999998, 2.0)); cursor log: 5 lines, header {"type":"session","schema":1,"tick_rate":120000,"clock":"session host clock (QPC) at hook entry","platform"...
- `rec.region` (5483 ms): daemon pid 14400 answered after 318 ms; started rec_01a1ee24be1b_0001; export job job_8181a7939b3434bf: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (9864 ms): daemon pid 4296 answered after 319 ms; started rec_01a1ee24d4c2_0001; export job job_3772584f20359704: failed; region 18x18: 18x18 h264 3.30 s; started rec_01a1ee24e5ef_0002; export job job_ba85cfc3897d2f18: failed; export job job_3772584f20359704: failed; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8757 ms): daemon pid 1364 answered after 319 ms; started rec_01a1ee24fc84_0001; before the kill: state recording recorded 6.0 s; daemon pid 16040 answered after 316 ms; recovered: 4.30 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `ai.no_model` (1903 ms): daemon pid 720 answered after 321 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","message":"No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureM...
- `ai.transcribe` (484343 ms): daemon pid 11076 answered after 322 ms; transcribe: exit 0 (0x00000000) { "job_id": "job_93f3cd5f0a8ff8ae", "type": "transcribe", "state": "done", "progress": 1.0, "message": "Transcribing 100%", "attempts": 1, "depends_on": [], "inputs": { "capture_id": "rec_06GYW99ESBNV...; library entry after transcription: {"id":"rec_06GYW99ESBNVF4T6","title":"clip-h264","kind":"recording","created":"2026-11-18T10:12:57.0131761Z","duration":360000,"width":320,"height":240,"size_bytes":663927,"broken":false,"...
- `desktop.stays_up` (11567 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8119 ms): second launch: exit 0 (0x00000000) after 34 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (6630 ms): hwnd 0x4005d2 pid 8604 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (9506 ms): hwnd 0x1905da pid 1060 class 'Zed::Window' title '' rect [369, 813, 1552, 965] ex 0x200188; hwnd 0x7061a pid 1060 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0x70642 pid 1060 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x160468 pid 1060 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(960,889) is the HUD; export job job_11c0cd1003b6cbc7: done; log hygiene: 4 WARN line(s) in the case's logs
- `desktop.hud_drag` (14352 ms): a drag from (546,934) moved the HUD by (-90,0); export job job_cea19c645ef70049: done
- `desktop.no_console_flash` (2607 ms): daemon pid 9620 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (9698 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5
- `desktop.tray_icon` (11820 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (1883 ms): daemon pid 3748 answered after 475 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr13940\65\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)","data":null,"exit_code":2}}; log hygiene: 1 WARN...
- `regress.encoder_unavailable_wording` (1135 ms): daemon pid 5796 answered after 318 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
