# L1 linux 0.3.0 regression report (full)

- **Date:** 2026-11-08T19:39:34Z to 2026-11-08T19:40:26Z
- **Who:** night orchestrator (VM aarch64 headless, tarball)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 5301d4b8, package folder `/home/dev/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /home/dev/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann --out /home/dev/wt/a64-out-full --commit 5301d4b8 --who night orchestrator (VM aarch64 headless, tarball)`, tier `full`
- **Result:** 35 passed, 0 failed, 36 skipped of 71 cases
- **SHA-256 of the files under test:**
  - `capturemann` `44df65952831a463b819dd74b6c03072684256545711dff55fc2cb43db15d6ee`
  - `cm-mcp` `000ff801dec3dbd8c6752468a09b68b87064061f89b5bb590f3f5c9e08ee04a8`
  - `cm-daemon` `68b67363fc567817b0499b2b89efefaedccc15fa76daaabaea240c9ae3202fd6`
  - `cm-desktop` `920c2d417c524c7c0c90348cb06e1f1f919bbcc3cb76d2450c899ecf0264fa6a`
  - `cm-overlay` `ffdbdef99860557058c167fad11469a5d4bb91fa5a07a9dc453ca11fd9f1ef50`
  - `ffmpeg` `b0d3886930b601c5867e7a444859776223b58225dc22dda0544d894cf941bf19`
  - `ffprobe` `1f1810ddb0a8b3317cf84735bafe9baba079b5b9176bf567ff708f45f19e1f1c`
  - `llama-server` `6744d298f21b091c3526feaefe6d706a149b66b9dc67c571799be14832f18c63`

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
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy (hardware H.264, or labelled software VP9 on Linux) |
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
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: needs --installer <setup.exe> |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: needs --installer <setup.exe> |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) |
| `regress.encoder_unavailable_wording` | SKIP | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle: needs a logged-in desktop session (this is session 0 or headless) |
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
| AI-3 | not run | `ai.transcribe` |
| AI-7 | PASS | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-4 | PASS | `export.audio` |
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
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | not run | `desktop.second_launch`, `linux.second_launch` |
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
| PKG-tarball | not run | `linux.tarball_install` |
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
| REC-7 | not run | `rec.screen_cursor` |
| REC-8 | not run | `rec.kill9_recovery` |
| REC-WIN section 5 | not run | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
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
| packaging/linux/install.sh | not run | `linux.tarball_install` |
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

- needs --installer <setup.exe> (2): linux.deb_install, linux.tarball_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- needs a logged-in desktop session (this is session 0 or headless) (19): targets.list, shot.display, shot.region, shot.window, shot.invalid, redact.window_text, rec.screen_cursor, rec.region, rec.region_min, rec.kill9_recovery, desktop.recording_banner, desktop.settings_applied, linux.session, linux.displays, linux.desktop_stays_up, linux.second_launch, linux.tray_icon, linux.hotkey_overlay, regress.encoder_unavailable_wording
- no desktop session or the screen is locked: no display exists to name in start_recording (1): mcp.confirm_tier
- no desktop session: a recording cannot name a display (a headless computer answers platform_rejected) (1): mcp.policy_allow
- not implemented for linux (9): installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.no_console_flash, desktop.recording_placement, desktop.tray_icon
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 (no usable hardware H.264 encoder was found; expec...) (1): regress.ffmpeg_missing_wording
- vainfo is not installed (apt install vainfo) (1): linux.vaapi_encoder

## Evidence of passing cases

- `pkg.files` (321 ms): capturemann: 3945512 bytes; cm-mcp: 9021056 bytes; cm-daemon: 90879656 bytes; cm-desktop: 141330424 bytes; cm-overlay: 10223144 bytes; licence: Some("/home/dev/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/dev/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/dev/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann/../../share/doc/cap...
- `pkg.startup` (433 ms): capturemann --version: exit 0 (0x00000000) in 18 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 16 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (791 ms): version 0.3.0; daemon pid 918628 answered after 282 ms; - video encoder: software VP9 (no usable hardware H.264 encoder was found; expect more CPU use) through ffmpeg at ~/wt/a64pkg/capturemann-0.3.0-linux-aarch64/li...; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann/ffmpeg; - llama-server at ~/wt/a64pkg/capturemann-0.3.0-linux-aarch64/lib/capturemann/llama-server (bundled with this bui...
- `daemon.lifecycle` (1154 ms): daemon pid 918678 answered after 282 ms; pid 918678, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1295 ms): daemon pid 918723 answered after 284 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (649 ms): daemon pid 918782 answered after 281 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: onnxruntime cpuid_info warning: Unknown CPU vendor. cpuinfo_vendor value: 0 [INFO cm_core::logging] daemon 0.3.0 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already se...; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (855 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [918835]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1030 ms): daemon pid 918858 answered after 289 ms; daemon pid 918897 answered after 285 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (693 ms): daemon pid 918962 answered after 284 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x0...
- `lib.manifest` (646 ms): daemon pid 919027 answered after 283 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (648 ms): daemon pid 919071 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (695 ms): daemon pid 919115 answered after 283 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (628 ms): daemon pid 919173 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (783 ms): daemon pid 919210 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1772 ms): daemon pid 919259 answered after 282 ms; clip-h264.mp4: rec_06GVSB49078P8GVY, 320x240, 3.00 s, job 83 ms; clip-h264.mov: rec_06GVSB4A5TD0G2FP, 320x240, 3.00 s, job 79 ms; clip-h264.mkv: rec_06GVSB4BBTR5K8V9, 320x240, 3.02 s, job 274 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (1194 ms): daemon pid 919616 answered after 285 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1175 ms): daemon pid 919774 answered after 281 ms; stored track: vp9 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; editing copy note: The editing copy is software VP9: this computer has no hardware H.264 encoder (VA-API).; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (914 ms): daemon pid 919925 answered after 291 ms; rec_06GVSB4S8JFFNFGR: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1315 ms): daemon pid 920011 answered after 284 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The video codec Png is not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr918566/19...; missing file: exit 2 (0x00000002) ...
- `import.dry_run` (679 ms): daemon pid 920255 answered after 290 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr918566/20/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1441 ms): daemon pid 920312 answered after 288 ms; export --preset quick: clip-h264 - quick.mp4 by libvpx-vp9, 435 ms; vp9 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; codec note: This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder.; encoder libvpx-vp9: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (12679 ms): daemon pid 920524 answered after 285 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by libvpx-vp9, 3535 ms; youtube-1080p: vp9 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1824416 bytes); youtube-1080p: codec note: This export is VP9 video, not H.264: this computer has no VA-API render node (/dev/dri/renderD*), so it has no hardware H.264 encoder.; export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by libvpx-vp9, 2812 ms; vertical-1080: vp9 1080x1920 3.00 ...
- `export.audio` (1491 ms): daemon pid 921133 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (934 ms): daemon pid 921311 answered after 281 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (710 ms): daemon pid 921437 answered after 286 ms; server capturemann 0.3.0, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (682 ms): daemon pid 921488 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (825 ms): daemon pid 921526 answered after 288 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 498 chars; get_diagnostics: ok, 5262 chars; list_models: ok, 1994 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1301 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (700 ms): daemon pid 921589 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (701 ms): daemon pid 921627 answered after 287 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7731 ms): daemon pid 921672 answered after 283 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (726 ms): daemon pid 921791 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (832 ms): daemon pid 921850 answered after 283 ms; port 38789: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1097 ms): daemon pid 921887 answered after 283 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech m...
- `ai.llama_version` (1169 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux aarch64; daemon pid 922041 answered after 284 ms; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
