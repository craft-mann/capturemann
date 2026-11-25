# L1 macos 0.3.7 regression report (hardware)

- **Date:** 2026-11-25T09:33:22Z to 2026-11-25T09:35:38Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 10465fe2, package folder `/private/tmp/cm-pub0.3.7/app/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --package-dir /private/tmp/cm-pub0.3.7/app/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261125T093322Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --installer /tmp/cm-pub0.3.7/CaptureMann-v0.3.7-macos-arm64.dmg --commit 10465fe2`, tier `hardware`
- **Result:** 57 passed, 0 failed, 31 skipped of 88 cases
- **SHA-256 of the files under test:**
  - `capturemann` `990ed6c76fca9853cfee3d346a3d3b5fe17159a6bdffdb29730061e414b5a3f7`
  - `cm-mcp` `2fedf76ceca28e1fb9afc139c8507eb79e0dfdb3504feff08d60919475ffa0a0`
  - `cm-daemon` `de805bbf1586752dba44323a58db8a11d6f1eb15488f7732f2adcbcef39c7a7b`
  - `cm-desktop` `3100f7671a8c589afa9878dea2fe1fe2f14dbe23e4d5320916f9840eeac1ed29`
  - `cm-overlay` `28789e1a130b798b1782afcbea2138f116a9beec495d5fc07f56114bfd030e70`
  - `ffmpeg` `d37819b0a821dbc5a1b21520fffdae36955ba4451d638f322211ad977b54219a`
  - `ffprobe` `50c9ef50ee08d81baa818e996bafdc13ce094b55e40e1fb7fb833b1dfeef9d3b`
  - `llama-server` `77df2ea73554984ace331f13b311eaa66038be679e3040814b1c05c29a48ba53`
  - `CaptureMann-v0.3.7-macos-arm64.dmg` `15e8845f7265eee6bbc714241e77245b6032ff119f48dc1cb930012cbddcb2a6`

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
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for macos |
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for macos |
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
| `guide.real_clicks` | SKIP | two injected pointer clicks become two steps with their click points in physical px; no keystroke is recorded; Markdown export holds two images: not implemented for macos |
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
| `rec.mic` | SKIP | a microphone track is recorded for the length of the recording: needs --allow mic |
| `rec.system_audio` | SKIP | system audio contains a tone played by another process: needs --allow sound |
| `rec.camera` | SKIP | a camera track keeps at least 24 fps in a lit room: needs --allow camera |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: not implemented for macos |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: needs --allow mic |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `installer.dmg` | PASS | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: not implemented for macos |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: keys cannot be injected: System Events refused the key press (grant Accessibility to the program that runs the suite): 3... |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for macos |
| `desktop.hud_drag` | SKIP | during a recording a pointer drag on the HUD bar moves the HUD window: not implemented for macos |
| `desktop.recording_banner` | SKIP | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video: not implemented for macos |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for macos |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for macos |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for macos |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): the menu-bar probe needs the Accessibility permission for the program that runs the suite |
| `linux.session` | SKIP | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal: not implemented for macos |
| `linux.displays` | SKIP | xrandr and the product list the same monitors: not implemented for macos |
| `linux.desktop_stays_up` | SKIP | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: not implemented for macos |
| `linux.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for macos |
| `linux.tray_icon` | SKIP | the tray icon registers as a StatusNotifierItem while the app runs: not implemented for macos |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: not implemented for macos |
| `linux.vaapi_encoder` | SKIP | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: not implemented for macos |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: not implemented for macos |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: not implemented for macos |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) |
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
| AUD-1 | not run | `rec.mic`, `rec.system_audio` |
| AUD-2 | not run | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
| BLK-1 | PASS | `bulk.export_images` |
| BRD-1 | PASS | `board.create` |
| CAM-1 | not run | `rec.camera` |
| CAM-2 | not run | `rec.camera` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | not run | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| EXP-5.AC3 | PASS | `export.estimate`, `export.estimate_mcp` |
| EXP-5.AC4 | PASS | `export.estimate` |
| GDE-1 | not run | `guide.real_clicks` |
| GDE-1.AC1 | not run | `guide.real_clicks` |
| GDE-1.AC2 | not run | `guide.real_clicks` |
| GDE-1.AC5 | PASS | `guide.refusals` |
| GDE-2 | PASS | `guide.edit_export` |
| GDE-3 | PASS | `guide.edit_export` |
| GDE-3.AC1 | PASS | `guide.edit_export`, `guide.real_clicks` |
| GDE-4 | PASS | `guide.edit_export`, `guide.refusals` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
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
| NFR-2 | PASS | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-9 | not run | `guide.real_clicks` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | PASS | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | not run | `desktop.tray_icon`, `linux.tray_icon` |
| REC-5.AC18 | not run | `desktop.hud_drag` |
| REC-7 | PASS | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug B3: the HUD could not be dragged on Windows | not run | `desktop.hud_drag` |
| bug class: camera 10 fps in low light | not run | `rec.camera` |
| bug class: crash at start | not run | `linux.desktop_stays_up` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | not run | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
| bug: 54 % late frames on a static screen | PASS | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | PASS | `import.vfr` |
| bug: Import editing copy not upright | PASS | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | not run | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | PASS | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | PASS | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | PASS | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | PASS | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| make verify-release | PASS | `installer.dmg` |
| named pipe single instance | PASS | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| packaging/macos | PASS | `installer.dmg` |
| platform-acceptance | not run | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

- None in the cases that ran.
## Not run

- keys cannot be injected: System Events refused the key press (grant Accessibility to the program that runs the suite): 36:79: execution error: System Events got an error: osascript is not allowed to send keystrokes. (1002) (1): desktop.hotkey_overlay
- needs --allow camera (1): rec.camera
- needs --allow mic (2): rec.mic, rec.soak
- needs --allow second_monitor (2): shot.second_monitor, rec.external_4k
- needs --allow sound (1): rec.system_audio
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- not implemented for macos (20): shot.window, redact.window_text, guide.real_clicks, rec.each_monitor, installer.silent, desktop.hud_click_through, desktop.hud_drag, desktop.recording_banner, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, linux.vaapi_encoder, linux.deb_install, linux.tarball_install
- the menu-bar probe needs the Accessibility permission for the program that runs the suite (1): desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- ffmpeg at /opt/homebrew/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (354 ms): capturemann: 4490192 bytes; cm-mcp: 10247600 bytes; cm-daemon is not in the app (embedded in cm-desktop); using --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon; cm-desktop: 121083744 bytes; cm-overlay: 8523280 bytes; licence: Some("/private/tmp/cm-pub0.3.7/app/CaptureMann.app/Contents/MacOS/../Resources/LICENSE"), third party: Some("/private/tmp/cm-pub0.3.7/app/CaptureMann.app/Contents/MacOS/../Resources/THIRD_PARTY_LICENSES.md"), notice: Some("/private/tmp/cm-pub0.3....
- `pkg.startup` (1104 ms): capturemann --version: exit 0 (0x00000000) in 19 ms; cm-mcp --bogus: exit 2 (0x00000002) in 19 ms; cm-daemon --bogus: exit 2 (0x00000002) in 639 ms; cm-overlay --help: exit 0 (0x00000000) in 19 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 19 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 19 ms; llama-server --version: exit 0 (0x00000000) in 18 ms
- `pkg.diagnostics` (1265 ms): version 0.3.7; daemon pid 66183 answered after 293 ms; - ffmpeg at /opt/homebrew/bin/ffmpeg; - llama-server not found: looked next to ~/source/repos/capturemann/target/release, in the settings and on PATH; this build should include it; - text recognizer: apple_vision (available); - speaker engine built in: true; - speaker models: not bundled with this build; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.lifecycle` (1228 ms): daemon pid 66277 answered after 293 ms; pid 66277, 101 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1712 ms): daemon pid 66335 answered after 292 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (734 ms): daemon pid 66444 answered after 290 ms; second daemon: exit 0 (0x00000000) in 18 ms; output: [INFO cm_core::logging] daemon 0.3.7 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr66076/6/s6.sock; n...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (957 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [66514]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1125 ms): daemon pid 66575 answered after 290 ms; daemon pid 66592 answered after 292 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (746 ms): daemon pid 66641 answered after 289 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (748 ms): daemon pid 66690 answered after 292 ms; display 1 'Built-in Retina Display' 3024x1964 at (0,0) scale 2 primary; display 2 'S27B80P' 3840x2160 at (-442,-2160) scale 2; 24 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ChatGPT, Claude, Code, Discord, Finder, Google Chrome, Microsoft Outlook, Microsoft Word, Notes, Slack, System Information, System Settings, Telegram, Terminal, WeChat, ‎WhatsApp; log hygiene: 1 WARN line(s) in the case's ...
- `shot.display` (1068 ms): daemon pid 66732 answered after 288 ms; display 1 3024x1964; PNG 3024x1964; capture shot_06H143D7JHPWPCRM; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (840 ms): daemon pid 66774 answered after 289 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (860 ms): daemon pid 66815 answered after 292 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (688 ms): daemon pid 66862 answered after 291 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (715 ms): daemon pid 66909 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (797 ms): daemon pid 66953 answered after 290 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (681 ms): daemon pid 67000 answered after 290 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (900 ms): daemon pid 67040 answered after 290 ms; recogniser Some(String("apple_vision")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `annotate.pack` (801 ms): daemon pid 67083 answered after 292 ms; 9 ops stored in one revision; step keeps style, start and text; outside the hole: white pixels keep 40 % of their brightness (28 points); colour pixels: callout 22098, stamps 2925, label stamp 2117, step badge 518; two spotlights: exit 2 (0x00000002) Some("invalid_input") ops[1]: only one spotlight per image: add the shapes to ops[0] {"code":"invalid_input","data":null,"exit_code":2,"message":"ops[1]: only one...; log hygiene: 1 WARN line(s) in the case's ...
- `import.images` (1379 ms): daemon pid 67130 answered after 293 ms; generated.png: shot_06H143E58M6NJMF9, original.png 120x80, source {"imported_at":"2026-11-25T09:33:42.087245Z","original_name":"generated.png","path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr66076/20/images/generated.png"}; shot.jpg: shot_06H143E6M7MHXFEQ, original.png 1000x260, source {"imported_at":"2026-11-25T09:33:42.436459Z","original_name":"shot.jpg","path":"/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr66076/20/images/shot.jpg"}; G...
- `board.create` (1322 ms): daemon pid 67183 answered after 292 ms; Pair board shot_06H143ECW6VK1X4W: 300x156, layout and 2 sources stored; Free board with 2 slots: 220x90; log hygiene: 1 WARN line(s) in the case's logs
- `bulk.export_images` (2270 ms): daemon pid 67229 answered after 287 ms; 2 JPEG files, longest side 60 px, proportions kept; second run kept the first files: ["tall-2 (2).jpg", "tall-2.jpg", "wide-1 (2).jpg", "wide-1.jpg"]; Blueprint export differs from the plain export, same size; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1957 ms): daemon pid 67274 answered after 292 ms; clip-h264.mp4: rec_06H143ESDRMXGHBM, 320x240, 3.00 s, job 187 ms; clip-h264.mov: rec_06H143ETQTT47NPR, 320x240, 3.00 s, job 180 ms; clip-h264.mkv: rec_06H143EW1YN1C969, 320x240, 3.02 s, job 496 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1269 ms): daemon pid 67363 answered after 290 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1270 ms): daemon pid 67421 answered after 290 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1300 ms): daemon pid 67477 answered after 291 ms; rec_06H143FB5HEZD2T5: kind Some(String("audio")), 2.00 s; mp3 rec_06H143FCEN4T4A14: kind Some(String("audio")), 2.09 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1469 ms): daemon pid 67537 answered after 292 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a GIF picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"GIF files are not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/var/folders/xm/d48xc26519...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (765 ms): daemon pid 67604 answered after 293 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr66076/28/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status"...; log hygiene: 1 WARN line(s) in the case's logs
- `guide.edit_export` (871 ms): daemon pid 67649 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `guide.refusals` (834 ms): daemon pid 67698 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1517 ms): daemon pid 67744 answered after 289 ms; export --preset quick: clip-h264 - quick.mp4 by h264_videotoolbox, 280 ms; h264 320x240 yuv420p range space trc prim , 3.00 s; encoder h264_videotoolbox: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (5451 ms): daemon pid 67807 answered after 291 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_videotoolbox, 666 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1308057 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_videotoolbox, 659 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1559030 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_videotoolbox, 493 ms; squar...
- `export.audio` (1571 ms): daemon pid 67924 answered after 290 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate` (1081 ms): daemon pid 67991 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.estimate_mcp` (1075 ms): daemon pid 68046 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1000 ms): daemon pid 68097 answered after 289 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (757 ms): daemon pid 68153 answered after 290 ms; server capturemann 0.3.7, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (729 ms): daemon pid 68197 answered after 287 ms; 90 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1349 ms): daemon pid 68237 answered after 289 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 541 chars; get_diagnostics: ok, 3573 chars; list_models: ok, 1786 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1344 chars; list_targets: ok, 400 chars; list_devices: ok, 1897 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (970 ms): daemon pid 68379 answered after 290 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (790 ms): daemon pid 68444 answered after 289 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (747 ms): daemon pid 68491 answered after 290 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (748 ms): daemon pid 68533 answered after 289 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7820 ms): daemon pid 68574 answered after 293 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (962 ms): daemon pid 68737 answered after 286 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (903 ms): daemon pid 68805 answered after 288 ms; port 63164: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (13023 ms): daemon pid 68892 answered after 292 ms; started rec_01a2120e6da9_0001; export job job_c8eb25f652ec770b: done; screen: h264 3024x1964 yuv420p r 30.00 avg 30.00 frames None 6.43 s; manifest duration Some(Number(770356)) ticks; manifest 6.42 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.633333, 2.0)); cursor log: 155 lines, header {"clicks":"unavailable","clock":"CLOCK_UPTIME_RAW ns (mach_absolute_time)","move_source":"poll","permission_missin...
- `rec.region` (4718 ms): daemon pid 68975 answered after 286 ms; started rec_01a2120ea0a4_0001; export job job_5dbec6dd9908b3e2: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (8706 ms): daemon pid 69038 answered after 290 ms; started rec_01a2120eb33b_0001; export job job_666f4433763c9b64: done; region 18x18: 18x18 h264 3.33 s; started rec_01a2120ec2e5_0002; export job job_93bee0a5687c6118: done; export job job_666f4433763c9b64: done; region 20x20: 20x20 h264 3.30 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8262 ms): daemon pid 69123 answered after 292 ms; started rec_01a2120ed55b_0001; before the kill: state recording recorded 6.0 s; daemon pid 69168 answered after 289 ms; recovered: 6.00 s of about 6.0 s recorded before the kill; log hygiene: 4 WARN line(s) in the case's logs
- `ai.no_model` (1100 ms): daemon pid 69212 answered after 291 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1237 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with AppleClang 21.0.0.21000334 for Darwin arm64; daemon pid 69267 answered after 293 ms; log hygiene: 1 WARN line(s) in the case's logs
- `installer.dmg` (1805 ms): mounted /tmp/cm-pub0.3.7/CaptureMann-v0.3.7-macos-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr66076/53/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.3.7; installed: capturemann 0.3.7; removed the copy; nothing left
- `desktop.stays_up` (11277 ms): 1 visible window(s) of the app: 'CaptureMann' 1180x793
- `desktop.second_launch` (7357 ms): second launch: exit 0 (0x00000000) after 93 ms; visible windows of the first instance: 1 before, 1 after
- `regress.encoder_unavailable_wording` (743 ms): daemon pid 70146 answered after 292 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
