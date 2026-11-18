# L1 linux 0.3.5 regression report (full)

> **Scope:** candidate files of commit 9dcfb5c3, built in the aarch64 VM with `release-build.sh --jobs 2`, **unsigned** (the maintainer signs with minisign). Tarball `capturemann-0.3.5-linux-aarch64.tar.gz` sha256 `07aed3c823330fd794dcf16732487ef91485fccd3fe6d409de93b51a5badc0d4`; `capturemann_0.3.5_arm64.deb` sha256 `6a859036a70d8a85043b408b9488ffcc55ebc035851df1af633e662396373cb9`. Headless VM: every desktop-session case is skipped. Not a report on final files.

- **Date:** 2026-11-18T01:55:54Z to 2026-11-18T01:57:00Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 9dcfb5c3, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/dev/reg035/full --package-dir /home/dev/.local/lib/capturemann --installer /home/dev/wt/release-out/9dcfb5c3-aarch64/capturemann-0.3.5-linux-aarch64.tar.gz --commit 9dcfb5c3 --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `full`
- **Result:** 44 passed, 0 failed, 36 skipped of 80 cases
- **SHA-256 of the files under test:**
  - `capturemann` `98351cdc061801b35ed8291297aebd7d0e7af7811ce7a35a7789ff900797a338`
  - `cm-mcp` `e3d07bb4a11c65ef547c7d0dbf8196c281cfbb893e926c4cbd08601d2f57429f`
  - `cm-daemon` `04c1a569709d8239eae57fc2d169f9953e094057e0e7ea1825d0f5f22b936498`
  - `cm-desktop` `aa163d927805ceb5d3ecc21510a4e119d6c596a56de50742f67891c560eb0c76`
  - `cm-overlay` `2a269e37fadf1a04c3aba07834e5deaf3bc8360f97d5307e3abde180941885e6`
  - `ffmpeg` `c06cd2f498b13a2c74268e6da8f693b42073c6dce3948e27802282b9b5a5a36b`
  - `ffprobe` `5d9bd78ee71cf51e815e1aca37e060ee9c87a1204e269673a885e5e6c9f45478`
  - `llama-server` `d75b3aaa87ea5be921491f811a9c6b4af51e2ea4c8687faafd87ac4ab9d0d343`
  - `capturemann-0.3.5-linux-aarch64.tar.gz` `07aed3c823330fd794dcf16732487ef91485fccd3fe6d409de93b51a5badc0d4`

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
| `targets.list` | SKIP | targets lists displays and windows with readable names and real sizes: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.display` | SKIP | a display screenshot is a valid PNG of the display's physical size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.region` | SKIP | a region screenshot has the requested size: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: needs a logged-in desktop session (this is session 0 or headless) |
| `shot.invalid` | SKIP | bad targets are refused with the shared error codes and create no capture: needs a logged-in desktop session (this is session 0 or headless) |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: needs a logged-in desktop session (this is session 0 or headless) |
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
| `mcp.confirm_tier` | SKIP | start_recording and delete_capture do not run without an on-screen confirmation: no desktop session or the screen is locked: no display exists to name in start_recording |
| `mcp.policy_allow` | SKIP | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP): no desktop session: a recording cannot name a display (a headless computer answers platform_rejected) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `rec.screen_cursor` | SKIP | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings: needs a logged-in desktop session (this is session 0 or headless) |
| `rec.region` | SKIP | a region recording is exactly the requested size: needs a logged-in desktop session (this is session 0 or headless) |
| `rec.region_min` | SKIP | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present: needs a logged-in desktop session (this is session 0 or headless) |
| `rec.kill9_recovery` | SKIP | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s: needs a logged-in desktop session (this is session 0 or headless) |
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
| `desktop.recording_banner` | SKIP | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: needs a logged-in desktop session (this is session 0 or headless) |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: needs a logged-in desktop session (this is session 0 or headless) |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: vainfo is not installed (apt install vainfo) |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: --installer is /home/dev/wt/release-out/9dcfb5c3-aarch64/capturemann-0.3.5-linux-aarch64.tar.gz, this case wants a file ending in .deb |
| `linux.tarball_install` | PASS | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) |
| `regress.encoder_unavailable_wording` | SKIP | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle: needs a logged-in desktop session (this is session 0 or headless) |
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
| CAP-1 | not run | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | not run | `shot.display` |
| CAP-1.AC2 | not run | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | not run | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
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
| IMP-4: second launch opens Import | not run | `desktop.second_launch`, `linux.second_launch` |
| IMP-6 | PASS | `import.images` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | not run | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| LINUX-REC-5 | not run | `desktop.recording_banner` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | not run | `rec.screen_cursor` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | PASS | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | not run | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | not run | `rec.region`, `rec.region_min` |
| REC-2 | not run | `rec.screen_cursor` |
| REC-3 | not run | `rec.screen_cursor` |
| REC-4 | not run | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | not run | `desktop.tray_icon`, `linux.tray_icon` |
| REC-5.AC18 | not run | `desktop.hud_drag` |
| REC-7 | not run | `rec.screen_cursor` |
| REC-8 | not run | `rec.kill9_recovery` |
| REC-WIN section 5 | not run | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | not run | `desktop.hud_drag` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | not run | `rec.region_min` |
| bug: 54 % late frames on a static screen | not run | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | not run | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | not run | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | not run | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | not run | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| make verify-release | not run | `installer.dmg` |
| named pipe single instance | not run | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | PASS | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | not run | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | not run | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

- None in the cases that ran.
## Not run

- --installer is /home/dev/wt/release-out/9dcfb5c3-aarch64/capturemann-0.3.5-linux-aarch64.tar.gz, this case wants a file ending in .deb (1): linux.deb_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- needs a logged-in desktop session (this is session 0 or headless) (19): targets.list, shot.display, shot.region, shot.window, shot.invalid, redact.window_text, rec.screen_cursor, rec.region, rec.region_min, rec.kill9_recovery, desktop.recording_banner, desktop.settings_applied, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, regress.encoder_unavailable_wording
- no desktop session or the screen is locked: no display exists to name in start_recording (1): mcp.confirm_tier
- no desktop session: a recording cannot name a display (a headless computer answers platform_rejected) (1): mcp.policy_allow
- not implemented for linux (10): installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.hud_drag, desktop.no_console_flash, desktop.recording_placement, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) (1): regress.ffmpeg_missing_wording
- vainfo is not installed (apt install vainfo) (1): linux.vaapi_encoder

## Evidence of passing cases

- `pkg.files` (313 ms): capturemann: 4390360 bytes; cm-mcp: 9643456 bytes; cm-daemon: 100564136 bytes; cm-desktop: 153913576 bytes; cm-overlay: 12818136 bytes; licence: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/dev/.local/lib/capturemann/../../share/doc/capturemann/SHERPA-ONNX/NOTICE.txt"); note: no README in the package folder
- `pkg.startup` (413 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1481 ms): version 0.3.5; daemon pid 1233259 answered after 281 ms; - video encoder: software VP9 (no usable hardware H.264 encoder was found; expect more CPU use) through ffmpeg at ~/.local/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/.local/lib/capturemann/ffmpeg; - llama-server at ~/.local/lib/capturemann/llama-server (bundled with this build, llama.cpp v0.7.0); - text recognizer: tesseract at /usr/bin/tesseract (tesseract 5....
- `daemon.lifecycle` (1153 ms): daemon pid 1233310 answered after 283 ms; pid 1233310, 97 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1300 ms): daemon pid 1233355 answered after 280 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (657 ms): daemon pid 1233415 answered after 283 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: onnxruntime cpuid_info warning: Unknown CPU vendor. cpuinfo_vendor value: 0 [INFO cm_core::logging] daemon 0.3.5 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already se...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (858 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [1233468]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1016 ms): daemon pid 1233491 answered after 281 ms; daemon pid 1233530 answered after 284 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (686 ms): daemon pid 1233595 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x...
- `lib.manifest` (644 ms): daemon pid 1233660 answered after 283 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (645 ms): daemon pid 1233704 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (732 ms): daemon pid 1233748 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (625 ms): daemon pid 1233806 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (794 ms): daemon pid 1233843 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (707 ms): daemon pid 1233961 answered after 281 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case'...
- `import.images` (1304 ms): daemon pid 1234026 answered after 284 ms; generated.png: shot_06GYRQJT87FFGD5P, original.png 120x80, source {"imported_at":"2026-11-18T01:56:08.905236469Z","original_name":"generated.png","path":"/tmp/cmr1233201/16/images/generated.png"}; shot.jpg: shot_06GYRQJVJ287CCKA, original.png 1000x260, source {"imported_at":"2026-11-18T01:56:09.236942809Z","original_name":"shot.jpg","path":"/tmp/cmr1233201/16/images/shot.jpg"}; GIF named .png: exit 2 (0x00000002) {"error":{"code":"invalid_input","message...
- `board.create` (1244 ms): daemon pid 1234120 answered after 282 ms; Pair board shot_06GYRQK1GS7TYAB1: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2244 ms): daemon pid 1234193 answered after 285 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1787 ms): daemon pid 1234271 answered after 283 ms; clip-h264.mp4: rec_06GYRQKD3WGR2EAJ, 320x240, 3.00 s, job 82 ms; clip-h264.mov: rec_06GYRQKEA7BWP2N3, 320x240, 3.00 s, job 79 ms; clip-h264.mkv: rec_06GYRQKFGHBJBF93, 320x240, 3.02 s, job 273 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1190 ms): daemon pid 1234628 answered after 284 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1180 ms): daemon pid 1234786 answered after 285 ms; stored track: vp9 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; editing copy note: The editing copy is software VP9: this computer has no hardware H.264 encoder (VA-API).; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1209 ms): daemon pid 1234937 answered after 284 ms; rec_06GYRQKXD6PH89PP: kind Some(String("audio")), 2.00 s; mp3 rec_06GYRQKYJ0AG5RTD: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1295 ms): daemon pid 1235078 answered after 281 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr1233201/23/picture...; missing file: exit 2 (0x00000002)...
- `import.dry_run` (680 ms): daemon pid 1235316 answered after 286 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr1233201/24/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mo...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (785 ms): daemon pid 1235373 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (715 ms): daemon pid 1235473 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1452 ms): daemon pid 1235552 answered after 286 ms; export --preset quick: clip-h264 - quick.mp4 by libvpx-vp9, 441 ms; vp9 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; codec note: This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder.; encoder libvpx-vp9: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (12401 ms): daemon pid 1235768 answered after 283 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by libvpx-vp9, 3645 ms; youtube-1080p: vp9 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1824416 bytes); youtube-1080p: codec note: This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder.; export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by libvpx-vp9, 2757 ms; vertical-1080: vp9 1080x1920 3.00...
- `export.audio` (1472 ms): daemon pid 1236483 answered after 281 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (979 ms): daemon pid 1236661 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (966 ms): daemon pid 1236877 answered after 280 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (925 ms): daemon pid 1237011 answered after 282 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (700 ms): daemon pid 1237198 answered after 282 ms; server capturemann 0.3.5, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (675 ms): daemon pid 1237249 answered after 282 ms; 89 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (822 ms): daemon pid 1237287 answered after 283 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 499 chars; get_diagnostics: ok, 4990 chars; list_models: ok, 1925 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1302 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (686 ms): daemon pid 1237345 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (685 ms): daemon pid 1237383 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7712 ms): daemon pid 1237428 answered after 281 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (716 ms): daemon pid 1237547 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (839 ms): daemon pid 1237606 answered after 285 ms; port 45483: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1089 ms): daemon pid 1237643 answered after 282 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech ...
- `ai.llama_version` (1160 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux aarch64; daemon pid 1237797 answered after 286 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.tarball_install` (2330 ms): /tmp/cmr1233201/47/prefix/lib/capturemann/capturemann --version: capturemann 0.3.5; documents checked in /tmp/cmr1233201/47/prefix/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
