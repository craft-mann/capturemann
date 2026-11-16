# L1 linux 0.3.4 regression report (full)

- **Date:** 2026-11-16T14:24:15Z to 2026-11-16T14:26:59Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 596c7956, package folder `/home/trung/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261116T142414Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --installer /home/trung/lab/capturemann-0.3.4-linux-x86_64.tar.gz --commit 596c7956`, tier `full`
- **Result:** 65 passed, 0 failed, 15 skipped of 80 cases
- **SHA-256 of the files under test:**
  - `capturemann` `0c29e0307e59be6f1b016a5b4d02ab6f486a904f091f471b1e8c5d8c02982b07`
  - `cm-mcp` `f153a8b4cd736364478b6279145011aa1119e7f3e88e5925612d9b0fd591998b`
  - `cm-daemon` `6deecf3392ba649c459a544d6e4750792c233b42a1628a9ceb0b2d7d6a886dc7`
  - `cm-desktop` `dd79a10d73fda9fc973ea484ccd80e63044a5eaa04b4ef033d420491a7236d11`
  - `cm-overlay` `b76dce3f8e5fb5db71eadb4880568ca710dd950f52b5859276efb3151ac0553a`
  - `ffmpeg` `6899a667eaea231e9a3512112d31b7e70373537ac132ebec91e57b09e015a1d4`
  - `ffprobe` `2ac4f2794ce5f7f231a3459644fbaac5a28d0a9cd7a2c21e24fe35356429527e`
  - `llama-server` `e18b9918b4fe200ca481fed4878d234933cf6604dbcd899f77983ebb714f8c49`
  - `capturemann-0.3.4-linux-x86_64.tar.gz` `ffce819c9c59a693b97a86b53c7048a3feea9e03c4bf55e00070033422668e11`

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
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `installer.dmg` | SKIP | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing: not implemented for linux |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: not implemented for linux |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for linux |
| `desktop.hud_drag` | SKIP | during a recording a pointer drag on the HUD bar moves the HUD window: not implemented for linux |
| `desktop.recording_banner` | PASS | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | PASS | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: --installer is /home/trung/lab/capturemann-0.3.4-linux-x86_64.tar.gz, this case wants a file ending in .deb |
| `linux.tarball_install` | PASS | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) |
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
| AI-3 | not run | `ai.transcribe` |
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
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | PASS | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay`, `desktop.recording_placement` |
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
| LINUX-1 | PASS | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| LINUX-REC-5 | PASS | `desktop.recording_banner` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | PASS | `rec.screen_cursor` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | PASS | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | PASS | `desktop.recording_banner` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-5.AC18 | not run | `desktop.hud_drag` |
| REC-7 | PASS | `rec.screen_cursor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | not run | `desktop.hud_drag` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 54 % late frames on a static screen | PASS | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | not run | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
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
| named pipe single instance | not run | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | PASS | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

- None in the cases that ran.
## Not run

- --installer is /home/trung/lab/capturemann-0.3.4-linux-x86_64.tar.gz, this case wants a file ending in .deb (1): linux.deb_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (10): installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.hud_drag, desktop.no_console_flash, desktop.recording_placement, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (313 ms): capturemann: 5589800 bytes; cm-mcp: 10861408 bytes; cm-daemon: 107355136 bytes; cm-desktop: 163334160 bytes; cm-overlay: 13440896 bytes; licence: Some("/home/trung/.local/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/trung/.local/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/trung/.local/lib/capturemann/../../share/doc/capturemann/SHERPA-ONNX/NOTICE.txt"); note: no README in the package folder
- `pkg.startup` (428 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1064 ms): version 0.3.4; daemon pid 83683 answered after 282 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at ~/.local/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/.local/lib/capturemann/ffmpeg; - llama-server at ~/.local/lib/capturemann/llama-server (bundled with this build, llama.cpp v0...
- `daemon.lifecycle` (1270 ms): daemon pid 83775 answered after 281 ms; pid 83775, 97 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1548 ms): daemon pid 83840 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (683 ms): daemon pid 83949 answered after 281 ms; second daemon: exit 0 (0x00000000) in 31 ms; output: [INFO cm_core::logging] daemon 0.3.4 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr83343/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (941 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [84075]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1038 ms): daemon pid 84106 answered after 282 ms; daemon pid 84169 answered after 280 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (799 ms): daemon pid 84419 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (765 ms): daemon pid 84569 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1343 ms): daemon pid 84622 answered after 282 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GY9FP0TG1Q35Q6; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (799 ms): daemon pid 84697 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (2808 ms): daemon pid 84761 answered after 282 ms; window 31457284: listed 842x442, saved 842x442; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (876 ms): daemon pid 84926 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (752 ms): daemon pid 85034 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (674 ms): daemon pid 85098 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (847 ms): daemon pid 85213 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (734 ms): daemon pid 85305 answered after 281 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1034 ms): daemon pid 85499 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (3286 ms): daemon pid 85568 answered after 281 ms; window screenshot shot_06GY9FQFQCBXDWDK: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (806 ms): daemon pid 85746 answered after 282 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (1384 ms): daemon pid 85843 answered after 282 ms; generated.png: shot_06GY9FQQSDQRS29N, original.png 120x80, source {"imported_at":"2026-11-16T14:24:42.979430883Z","original_name":"generated.png","path":"/tmp/cmr83343/22/images/generated.png"}; shot.jpg: shot_06GY9FQSA7BXDBB1, original.png 1000x260, source {"imported_at":"2026-11-16T14:24:43.369317466Z","original_name":"shot.jpg","path":"/tmp/cmr83343/22/images/shot.jpg"}; GIF named .png: exit 2 (0x00000002) {"error":{"code":"invalid_input","message":"gif...
- `board.create` (1325 ms): daemon pid 85987 answered after 281 ms; Pair board shot_06GY9FQZDVT1ENS6: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2705 ms): daemon pid 86145 answered after 282 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1996 ms): daemon pid 86409 answered after 281 ms; clip-h264.mp4: rec_06GY9FRD546VEWHY, 320x240, 3.00 s, job 201 ms; clip-h264.mov: rec_06GY9FREG1WNCNP7, 320x240, 3.00 s, job 217 ms; clip-h264.mkv: rec_06GY9FRFVZBT4EWR, 320x240, 3.02 s, job 350 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1312 ms): daemon pid 86989 answered after 282 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1300 ms): daemon pid 87235 answered after 282 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1231 ms): daemon pid 87515 answered after 282 ms; rec_06GY9FRZ90GW1VGR: kind Some(String("audio")), 2.00 s; mp3 rec_06GY9FS0FJ83AG9Z: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1452 ms): daemon pid 87714 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr83343/29/picture.g...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (809 ms): daemon pid 88052 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr83343/30/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov"...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (982 ms): daemon pid 88133 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (825 ms): daemon pid 88285 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1644 ms): daemon pid 88557 answered after 280 ms; export --preset quick: clip-h264 - quick.mp4 by h264_vaapi, 328 ms; h264 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; encoder h264_vaapi: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (9427 ms): daemon pid 88915 answered after 282 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_vaapi, 1432 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1013013 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_vaapi, 1227 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (767304 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_vaapi, 1092 ms; square-1080: h264 1080x1...
- `export.audio` (1530 ms): daemon pid 89970 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1088 ms): daemon pid 90290 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1173 ms): daemon pid 90535 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1052 ms): daemon pid 90748 answered after 282 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (806 ms): daemon pid 90926 answered after 282 ms; server capturemann 0.3.4, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (781 ms): daemon pid 91001 answered after 281 ms; 89 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1279 ms): daemon pid 91109 answered after 282 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 497 chars; get_diagnostics: ok, 4795 chars; list_models: ok, 1927 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1300 chars; list_targets: ok, 248 chars; list_devices: ok, 2012 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (821 ms): daemon pid 91221 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (711 ms): daemon pid 91322 answered after 281 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (720 ms): daemon pid 91596 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (700 ms): daemon pid 91650 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7744 ms): daemon pid 91715 answered after 282 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (863 ms): daemon pid 92039 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (961 ms): daemon pid 92137 answered after 281 ms; port 46099: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13352 ms): daemon pid 92335 answered after 281 ms; started rec_01a1e4bfaf87_0001; export job job_0ce8187b1529e34d: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.00 s; manifest duration Some(Number(733408)) ticks; manifest 6.11 s, expected 6.03 s (recorded at the pause plus the time after the resume); 3 key frames, gaps Some((2.0, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (CLOCK_MONOTONIC) at event read","move_source":"xi2_raw","permission_mi...
- `rec.region` (5119 ms): daemon pid 93052 answered after 282 ms; started rec_01a1e4bfe3d5_0001; export job job_d5f0e35a4885919a: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (8213 ms): daemon pid 93381 answered after 282 ms; started rec_01a1e4bff7aa_0001; export job job_dbfd07258f9269ff: done; region 18x18: 18x18 h264 3.00 s; started rec_01a1e4c00624_0002; export job job_b694282627a8ad5d: done; export job job_dbfd07258f9269ff: done; region 20x20: 20x20 h264 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8019 ms): daemon pid 94095 answered after 282 ms; started rec_01a1e4c017de_0001; before the kill: state recording recorded 6.0 s; daemon pid 94491 answered after 282 ms; recovered: 4.00 s of about 6.0 s recorded before the kill; log hygiene: 4 WARN line(s) in the case's logs
- `ai.no_model` (1283 ms): daemon pid 94547 answered after 282 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1262 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 94765 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.recording_banner` (9956 ms): hwnd 0x1e00001 pid 94874 class 'dev.capturemann.app' title 'CaptureMann' rect [402, 152, 1582, 912] ex 0x0; record status: recording; export job job_a21619cfda9801b4: done
- `desktop.settings_applied` (9658 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.7 s with capture.delay_s = 5
- `linux.session` (330 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (843 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 96160 answered after 282 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10920 ms): 1 visible window(s) of the app: hwnd 0x1e00001 pid 96213 class 'dev.capturemann.app' title 'CaptureMann' rect [402, 152, 1582, 912] ex 0x0
- `linux.second_launch` (6979 ms): second launch: exit 0 (0x00000000) after 31 ms
- `linux.tray_icon` (6975 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-96907-1
- `linux.vaapi_encoder` (1056 ms): vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder; profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2; daemon pid 97199 answered after 282 ms; diagnostics: - VAAPI render device /dev/dri/renderD128; log hygiene: 1 WARN line(s...
- `linux.tarball_install` (2779 ms): /tmp/cmr83343/65/prefix/lib/capturemann/capturemann --version: capturemann 0.3.4; documents checked in /tmp/cmr83343/65/prefix/share/doc/capturemann
- `regress.encoder_unavailable_wording` (775 ms): daemon pid 97682 answered after 282 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.

## Build, package checks and install of these candidate files (added by the agent after the run)

- **Source:** commit 596c7956b306d59782c6330a2b428f92e50a43ce (an incremental bundle of this commit was fetched into `~/wt/main` of the lab ThinkPad, checked out detached), built with `packaging/linux/release-build.sh --src ~/wt/main --commit 596c7956b306d59782c6330a2b428f92e50a43ce --jobs 3` (clean `git archive` export, caches kept: 22 minutes; step log in `~/wt/release-596-x86_64.log`, exit 0). Machine: ThinkPad T490, Ubuntu 24.04.5, glibc 2.39, rustc 1.99.1, X11 session, Intel UHD 620.
- **Candidate files of commit 596c7956 (x86_64, not signed with minisign, not published):**
  - `capturemann-0.3.4-linux-x86_64.tar.gz` 150,892,639 bytes, SHA-256 `ffce819c9c59a693b97a86b53c7048a3feea9e03c4bf55e00070033422668e11`
  - `capturemann_0.3.4_amd64.deb` 113,304,492 bytes, SHA-256 `2dac7de94cd4654c431ba577a22ecd8fa6471126cfa9aa88b632c849b7b5bcb4`
  - Both hashes were recomputed on the Mac after copying the files off the laptop and match `SHA256SUMS.txt` and the `.sha256` files.
- **`check-package.sh`** (run by `release-build.sh` on both files): PASS for the tarball and the .deb, with `Version: 0.3.4` and `capturemann --version says 0.3.4`, `ffmpeg`, `ffprobe` and `llama-server --version` exit 0, `ldd` resolves (logs in `~/wt/release-out/596c7956-x86_64/logs/`). No stale-binary refusal happened (the build was fresh), so the "must fail on a stale binary" behaviour was not triggered or tested here.
- **Installed** from the tarball with its `install.sh` into `~/.local` (the package folder of this report); `capturemann --version` printed `capturemann 0.3.4`. `cm-regress` was rebuilt from the same source on the laptop and copied to `~/lab/cm-regress`.
- **Runs (X11, `--allow screen`, `--installer` the tarball, recording on):** standard 57 passed, 0 failed, 7 skipped of 64 (`L1-linux-0.3.4-596c7956-regression-standard-2026-11-16-claude.md`); this full tier 65 passed, 0 failed, 15 skipped of 80; the `.deb` case on its own (`--only linux.deb_install --installer <the .deb>`): 1 passed, 0 failed (`L1-linux-0.3.4-596c7956-regression-deb-install-2026-11-16-claude.md`), because a full run takes one installer file and in this run it was the tarball (so `linux.deb_install` is SKIP above).
- **Not run:** Wayland (the session stays X11; GNOME session type is chosen at the login screen, not switched here), the hardware tier (sound, mic, camera), second monitor, AMD/NVIDIA, KDE, other distributions, the aarch64 VM (skipped for time), the `.deb` through an APT repository, `ai.transcribe` and `ai.speakers` (no models given).

## Long Shot on real sites (Firefox X11, not a regression case)

Run from `~/lab/ls-all.sh` (Firefox snap in a fresh profile `lsprof`, muted, nothing played, no login, window 1280x900 at scale 1, `long_shot_hand --window "Mozilla Firefox" --auto --secs 240`, built from the same source with `--no-default-features`). Numbers only, the pictures were not looked at, nothing was debugged. Every run printed `XGetWindowProperty[_NET_WM_DESKTOP] failed (code=1)` once (a message of the window library, not a result).

| Page | Frames | End | Result px | Time |
| --- | --- | --- | --- | --- |
| carchargeos.com | 15 | reached_end | 1276 x 7339 | 46 s |
| en.wikipedia.org Rust (programming language) | 25 | limit_reached | 1276 x 16384 | 36 s |
| youtube.com/@Google/videos | 43 | limit_reached | 1228 x 16384 | 133 s |
| docs.rs/serde | 4 | reached_end | 1276 x 2053 | 12 s |
| paulgraham.com/greatwork.html | 25 | limit_reached | 1280 x 16384 | 27 s |

Wayland Long Shot: not run. After the run the ThinkPad was switched back to Windows 11.
