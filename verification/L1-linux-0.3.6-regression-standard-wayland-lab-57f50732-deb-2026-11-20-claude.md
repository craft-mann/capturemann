# L1 linux 0.3.6 regression report (standard)

- **Date:** 2026-11-20T15:38:46Z to 2026-11-20T15:40:33Z
- **Who:** claude QA 0.3.6 final
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 57f50732, package folder `/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out /home/trung/lab/regress-out/20261120T153845Z --who claude QA 0.3.6 final --verdict PARTIAL --allow screen --commit 57f50732`, tier `standard`
- **Result:** 55 passed, 0 failed, 9 skipped of 64 cases
- **SHA-256 of the files under test:**
  - `capturemann` `8359808ec816df50c7fa983276334486438bbd8a7dfa634c089358459cc904f7`
  - `cm-mcp` `8bec8ead274aa13d941d3c575ced368f1e007352de1b79190693fd15258d70fd`
  - `cm-daemon` `a68f96d3ceb1d9eb019ec369c888de5aff580c53f21d29d0ec2ccba127abcfca`
  - `cm-desktop` `4930049bf83b045e48c2864a8081f41454bff4d8112b66f695029fcf6a746315`
  - `cm-overlay` `33cb287f06bccef931132b6b0db461c2b754256baa5c8a65aafbbbeffbf30a9c`
  - `ffmpeg` `5ad1e745fa8e7660516df437800cecdc7adc194d6c921f7beae3ea6ed0f2380c`
  - `ffprobe` `bea15d513c2119fe839fc68b0d19937e5912b6e5d66f98eeffab67be65742cf8`
  - `llama-server` `5854ad4a58a3bd9aead8151729a1ba10d7c291069d6203a823a3f098c61a7bd0`

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
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session |
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
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | PASS | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) |
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
| AI-1 | PASS | `ai.no_model` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
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
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | PASS | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
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
| LINUX-1 | PASS | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-licence | PASS | `pkg.files` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `zz.crash_events` |
| bug: settings set not applied to the running app | PASS | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| named pipe single instance | not run | `desktop.second_launch` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |

## Problems found

- None in the cases that ran.
## Not run

- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (5): desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.tray_icon
- this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session (2): shot.window, redact.window_text
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (317 ms): capturemann: 5656688 bytes; cm-mcp: 10890752 bytes; cm-daemon: 108490520 bytes; cm-desktop: 165901928 bytes; cm-overlay: 13460088 bytes; licence: Some("/usr/share/doc/capturemann/LICENSE"), third party: Some("/usr/share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/usr/share/doc/capturemann/SHERPA-ONNX/NOTICE.txt"); note: no README in the package folder
- `pkg.startup` (428 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1155 ms): version 0.3.6; daemon pid 101838 answered after 282 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - llama-server at /opt/capturemann/lib/capturemann/llama-server (bundled with...
- `daemon.lifecycle` (1272 ms): daemon pid 101931 answered after 282 ms; pid 101931, 101 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1659 ms): daemon pid 101996 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (695 ms): daemon pid 102104 answered after 282 ms; second daemon: exit 0 (0x00000000) in 31 ms; output: [INFO cm_core::logging] daemon 0.3.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr101710/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (921 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [102181]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1060 ms): daemon pid 102212 answered after 282 ms; daemon pid 102271 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (799 ms): daemon pid 102368 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x0...
- `targets.list` (1331 ms): daemon pid 102465 answered after 282 ms; display 1 'Built-in display' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":false,"list_windows":false,"pick":true}; apps: ; log hygiene: 2 WARN line(s) in the case's logs
- `shot.display` (1933 ms): daemon pid 102536 answered after 282 ms; display 1 1920x1080; PNG 1920x1080; capture shot_06GZK73BVKDV42R5; log hygiene: 2 WARN line(s) in the case's logs
- `shot.region` (1326 ms): daemon pid 102628 answered after 281 ms; region 100,100,400,300: PNG 400x300; log hygiene: 2 WARN line(s) in the case's logs
- `shot.invalid` (1360 ms): daemon pid 102793 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 1 (0x00000001) Some("not_implemented"); log hygiene: 2 WARN line(s) in the case's logs
- `lib.manifest` (745 ms): daemon pid 102925 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (664 ms): daemon pid 102989 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (923 ms): daemon pid 103053 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (632 ms): daemon pid 103139 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1050 ms): daemon pid 103192 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (822 ms): daemon pid 103301 answered after 282 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's...
- `import.images` (1420 ms): daemon pid 103398 answered after 282 ms; generated.png: shot_06GZK74MACSPCX2Z, original.png 120x80, source {"imported_at":"2026-11-20T15:39:10.828064652Z","original_name":"generated.png","path":"/tmp/cmr101710/22/images/generated.png"}; shot.jpg: shot_06GZK74NTPS773FN, original.png 1000x260, source {"imported_at":"2026-11-20T15:39:11.213827968Z","original_name":"shot.jpg","path":"/tmp/cmr101710/22/images/shot.jpg"}; GIF named .png: exit 2 (0x00000002) {"error":{"code":"invalid_input","message":"...
- `board.create` (1343 ms): daemon pid 103540 answered after 281 ms; Pair board shot_06GZK74W4NYMP14Z: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2709 ms): daemon pid 103649 answered after 282 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1970 ms): daemon pid 103763 answered after 281 ms; clip-h264.mp4: rec_06GZK759XCTPGXGA, 320x240, 3.00 s, job 201 ms; clip-h264.mov: rec_06GZK75B88TCJ1W9, 320x240, 3.00 s, job 137 ms; clip-h264.mkv: rec_06GZK75CJCK1MPG6, 320x240, 3.02 s, job 355 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (957 ms): daemon pid 104292 answered after 282 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1284 ms): daemon pid 104590 answered after 282 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1332 ms): daemon pid 104825 answered after 282 ms; rec_06GZK75TG5Q71TZG: kind Some(String("audio")), 2.00 s; mp3 rec_06GZK75VQ56FP9ZN: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1396 ms): daemon pid 105018 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr101710/29/picture....; missing file: exit 2 (0x00000002) ...
- `import.dry_run` (810 ms): daemon pid 105356 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr101710/30/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (1046 ms): daemon pid 105437 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (1358 ms): daemon pid 105589 answered after 281 ms; log hygiene: 2 WARN line(s) in the case's logs
- `export.quick` (1648 ms): daemon pid 105725 answered after 282 ms; export --preset quick: clip-h264 - quick.mp4 by h264_vaapi, 340 ms; h264 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; encoder h264_vaapi: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1518 ms): daemon pid 106032 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1091 ms): daemon pid 106301 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1097 ms): daemon pid 106546 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1070 ms): daemon pid 106759 answered after 282 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (806 ms): daemon pid 106937 answered after 282 ms; server capturemann 0.3.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (808 ms): daemon pid 107012 answered after 282 ms; 90 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1793 ms): daemon pid 107116 answered after 282 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 498 chars; get_diagnostics: ok, 4863 chars; list_models: ok, 1924 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1301 chars; list_targets: ok, 259 chars; list_devices: ok, 2382 chars; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1328 ms): daemon pid 107233 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.policy_allow` (1338 ms): daemon pid 107343 answered after 282 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 2 WARN line(s) in the case's logs
- `mcp.request_id` (703 ms): daemon pid 107426 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (793 ms): daemon pid 107481 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7863 ms): daemon pid 107546 answered after 282 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1433 ms): daemon pid 107713 answered after 282 ms; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.http` (842 ms): daemon pid 107878 answered after 282 ms; port 40183: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1318 ms): daemon pid 107931 answered after 282 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech m...
- `ai.llama_version` (1264 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 108149 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (10638 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 6.7 s with capture.delay_s = 5; log hygiene: 5 WARN line(s) in the case's logs
- `linux.session` (352 ms): session wayland desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (1282 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 108583 answered after 282 ms; product: display 1 'Built-in display' 1920x1080 at (0,0) scale 1; log hygiene: 2 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10928 ms): window not checked: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session; the app stayed up and its daemon answers; log hygiene: 2 WARN line(s) in the case's logs
- `linux.second_launch` (6971 ms): second launch: exit 0 (0x00000000) after 31 ms; log hygiene: 2 WARN line(s) in the case's logs
- `linux.tray_icon` (6949 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-108927-1; log hygiene: 2 WARN line(s) in the case's logs
- `linux.vaapi_encoder` (1176 ms): vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder; profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2; daemon pid 109016 answered after 282 ms; diagnostics: - VAAPI render device /dev/dri/renderD128; log hygiene: 1 WARN line(...

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
