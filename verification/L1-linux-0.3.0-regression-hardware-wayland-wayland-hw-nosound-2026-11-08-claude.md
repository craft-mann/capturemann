# L1 linux 0.3.0 regression report (hardware)

- **Date:** 2026-11-08T20:19:23Z to 2026-11-08T20:22:27Z
- **Who:** night orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5301d4b8, package folder `/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out /home/trung/lab/regress-out/20261108T201922Z --who night orchestrator --verdict PARTIAL --allow screen,mic,camera --package-dir /home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann --commit 5301d4b8`, tier `hardware`
- **Result:** 51 passed, 4 failed, 23 skipped of 78 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2d6346b93480161ec442ec26ebc099e9d4dbc0e7351944bd6a20cdabf32adf60`
  - `cm-mcp` `0baca1bbeb63be2a930c584db209039e45488366172ff1364f1e3477a30215ca`
  - `cm-daemon` `c546c08a0fd110d225cb636f5376fdc4fde0d1ae1e65a77b37846688e76727d0`
  - `cm-desktop` `b6e76b9e36b3ad2d464a217b1f4faf138dbdae05be24101de707d931905daa6c`
  - `cm-overlay` `fbe5ecfd0192d3026e69c5a9e0c8d0d521e363328009b2f33812d1c953503494`
  - `ffmpeg` `5da034620e64147d7e2ddf849ba258e34996e45849a7ea4777a3eef145e07ccc`
  - `ffprobe` `af190fa91ddd6d97b6e4426461526f3950509454a42c1e91d55b4c57cb78bc13`
  - `llama-server` `9b6a30cc527aa50e6f1c309f103f7fdb87c944a13d3382e733fee7c1d881ca5c`

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
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
| `shot.invalid` | FAIL | bad targets are refused with the shared error codes and create no capture: unknown window: exit Some(1), expected 2 |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session |
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
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | PASS | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `rec.screen_cursor` | FAIL | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings: no cursor log in the bundle; files: ["regress recording - quick.mp4", "manifest.json", "screen.mp4", "session.json"] |
| `rec.region` | PASS | a region recording is exactly the requested size |
| `rec.region_min` | PASS | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | SKIP | system audio contains a tone played by another process: needs --allow sound |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: not implemented for linux |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: give --soak-minutes <n> to run a long recording |
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
| `desktop.recording_banner` | SKIP | Linux: during a recording the app shows its banner window and no floating HUD or screen-sized topmost window that would be in the video: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland win... |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | FAIL | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: no screenshot appeared within 30 s of the forwarded display capture request |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | FAIL | xrandr and the product list the same monitors: xrandr failed: Can't open display |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | PASS | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: needs --installer <setup.exe> |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: needs --installer <setup.exe> |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) |
| `regress.encoder_unavailable_wording` | PASS | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle |
| `zz.crash_events` | PASS | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | FAIL | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `mcp.errors`, `desktop.settings_applied` |
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
| CAP-1 | FAIL | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | FAIL | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | PASS | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | FAIL | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| LINUX-REC-5 | not run | `desktop.recording_banner` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | FAIL | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | FAIL | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | FAIL | `rec.screen_cursor` |
| REC-3 | FAIL | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | FAIL | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 (Linux: banner in the Studio window, not a floating HUD) | not run | `desktop.recording_banner` |
| REC-5 tray | PASS | `desktop.tray_icon`, `linux.tray_icon` |
| REC-7 | FAIL | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | not run | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
| bug: 54 % late frames on a static screen | FAIL | `rec.screen_cursor` |
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
| bug: recording stop crash 0xC0000409 | FAIL | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | FAIL | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | PASS | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | FAIL | `shot.invalid`, `export.errors` |
| make verify-release | not run | `installer.dmg` |
| named pipe single instance | not run | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

### `shot.invalid`: bad targets are refused with the shared error codes and create no capture

- unknown window: exit Some(1), expected 2

Evidence:

```text
daemon pid 1176170 answered after 282 ms
unknown display: exit 2 (0x00000002) Some("not_found")
empty region: exit 2 (0x00000002) Some("invalid_input")
region off the display: exit 2 (0x00000002) Some("invalid_input")
unknown window: exit 1 (0x00000001) Some("not_implemented")
```

### `rec.screen_cursor`: screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings

- no cursor log in the bundle; files: ["regress recording - quick.mp4", "manifest.json", "screen.mp4", "session.json"]

Evidence:

```text
daemon pid 1180909 answered after 282 ms
started rec_01a1bcd1c863_0001
export job job_2c7097e3a731eeb9: done
screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.03 s; manifest duration Some(Number(728893)) ticks
manifest 6.07 s, expected 6.03 s (recorded at the pause plus the time after the resume)
4 key frames, gaps Some((2.0, 2.0))
```

### `desktop.settings_applied`: settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long

- no screenshot appeared within 30 s of the forwarded display capture request

Evidence:

```text
forwarded capture request: exit 0 (0x00000000)
```

### `linux.displays`: xrandr and the product list the same monitors

- xrandr failed: Can't open display

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (2): shot.second_monitor, rec.external_4k
- needs --allow sound (1): rec.system_audio
- needs --installer <setup.exe> (2): linux.deb_install, linux.tarball_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (10): rec.each_monitor, installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.no_console_flash, desktop.recording_placement, desktop.tray_icon
- this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session (3): shot.window, redact.window_text, desktop.recording_banner
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffm...) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.files` (333 ms): capturemann: 5013560 bytes; cm-mcp: 9943360 bytes; cm-daemon: 97857840 bytes; cm-desktop: 150193016 bytes; cm-overlay: 10845728 bytes; licence: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/LICENSE"), third party: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../share/doc/capturemann/THIRD_PARTY_LICENSES.md"), notice: Some("/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/../../shar...
- `pkg.startup` (426 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 15 ms
- `pkg.diagnostics` (1095 ms): version 0.3.0; daemon pid 1175198 answered after 282 ms; - GPU Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620], kernel driver i915, pci id 8086:3ea0; - video encoder: hardware H.264 through VA-API (/dev/dri/renderD128) through ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann/ffmpeg; - llama-server at ~/...
- `daemon.lifecycle` (1266 ms): daemon pid 1175295 answered after 282 ms; pid 1175295, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1631 ms): daemon pid 1175360 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (664 ms): daemon pid 1175466 answered after 282 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: [INFO cm_core::logging] daemon 0.3.0 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr1175116/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (918 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [1175543]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1060 ms): daemon pid 1175574 answered after 282 ms; daemon pid 1175635 answered after 283 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (794 ms): daemon pid 1175732 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x...
- `targets.list` (1273 ms): daemon pid 1175829 answered after 282 ms; display 1 'Built-in display' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":false,"list_windows":false,"pick":true}; apps: ; log hygiene: 2 WARN line(s) in the case's logs
- `shot.display` (1951 ms): daemon pid 1175902 answered after 282 ms; display 1 1920x1080; PNG 1920x1080; capture shot_06GVSM83SVGJVD6F; log hygiene: 2 WARN line(s) in the case's logs
- `shot.region` (1339 ms): daemon pid 1176050 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 2 WARN line(s) in the case's logs
- `lib.manifest` (751 ms): daemon pid 1176292 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (663 ms): daemon pid 1176356 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (978 ms): daemon pid 1176422 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (734 ms): daemon pid 1176508 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1004 ms): daemon pid 1176561 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (1998 ms): daemon pid 1176670 answered after 282 ms; clip-h264.mp4: rec_06GVSM9A6CEY5PDE, 320x240, 3.00 s, job 141 ms; clip-h264.mov: rec_06GVSM9BJADHRYFJ, 320x240, 3.00 s, job 147 ms; clip-h264.mkv: rec_06GVSM9CWC2FM8NZ, 320x240, 3.02 s, job 349 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (950 ms): daemon pid 1177201 answered after 282 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (1198 ms): daemon pid 1177447 answered after 282 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1043 ms): daemon pid 1177682 answered after 282 ms; rec_06GVSM9TD2F5GYJX: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1527 ms): daemon pid 1177852 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The video codec Png is not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr1175116/2...; missing file: exit 2 (0x00000002)...
- `import.dry_run` (778 ms): daemon pid 1178198 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr1175116/26/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mo...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (1587 ms): daemon pid 1178279 answered after 282 ms; export --preset quick: clip-h264 - quick.mp4 by h264_vaapi, 278 ms; h264 320x240 yuv420p range tv space bt709 trc prim , 3.00 s; encoder h264_vaapi: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (10165 ms): daemon pid 1178582 answered after 282 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_vaapi, 1342 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1013013 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_vaapi, 1255 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (767304 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_vaapi, 1199 ms; square-1080: h264 1080...
- `export.audio` (1547 ms): daemon pid 1179387 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1054 ms): daemon pid 1179658 answered after 281 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (813 ms): daemon pid 1179888 answered after 282 ms; server capturemann 0.3.0, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (799 ms): daemon pid 1179963 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1815 ms): daemon pid 1180017 answered after 281 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 499 chars; get_diagnostics: ok, 5424 chars; list_models: ok, 2001 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1302 chars; list_targets: ok, 259 chars; list_devices: ok, 2420 chars; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1331 ms): daemon pid 1180141 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.policy_allow` (1295 ms): daemon pid 1180255 answered after 282 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 2 WARN line(s) in the case's logs
- `mcp.request_id` (706 ms): daemon pid 1180339 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (790 ms): daemon pid 1180393 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7840 ms): daemon pid 1180458 answered after 282 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1419 ms): daemon pid 1180684 answered after 282 ms; log hygiene: 2 WARN line(s) in the case's logs
- `mcp.http` (946 ms): daemon pid 1180843 answered after 282 ms; port 45299: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region` (5614 ms): daemon pid 1181451 answered after 281 ms; started rec_01a1bcd1feb6_0001; export job job_30d0578a203b7bfc: done; region recording 640x360 h264; log hygiene: 2 WARN line(s) in the case's logs
- `rec.region_min` (9480 ms): daemon pid 1181748 answered after 281 ms; started rec_01a1bcd214ee_0001; export job job_ca99ba2394eed174: done; region 18x18: 18x18 h264 2.97 s; started rec_01a1bcd225df_0002; export job job_4e8d9184a733eacf: done; export job job_ca99ba2394eed174: done; region 20x20: 20x20 h264 3.03 s; log hygiene: 3 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8632 ms): daemon pid 1182223 answered after 282 ms; started rec_01a1bcd239d6_0001; before the kill: state recording recorded 6.0 s; daemon pid 1182426 answered after 282 ms; recovered: 4.00 s of about 6.0 s recorded before the kill; log hygiene: 5 WARN line(s) in the case's logs
- `rec.mic` (7208 ms): daemon pid 1182491 answered after 281 ms; started rec_01a1bcd25bb7_0001; export job job_4744671df770b9dc: done; mic track 4.02 s; mic level: rms -36.7 dBFS, peak -10.5 dBFS; log hygiene: 2 WARN line(s) in the case's logs
- `rec.camera` (9663 ms): daemon pid 1182822 answered after 281 ms; started rec_01a1bcd277f1_0001; export job job_4e436f5cfdbe3904: done; camera 1280x720 h264 measured 30.0 fps, 5.03 s; log hygiene: 2 WARN line(s) in the case's logs
- `ai.no_model` (1315 ms): daemon pid 1183265 answered after 282 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech ...
- `ai.llama_version` (1260 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 1183483 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (332 ms): session wayland desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.desktop_stays_up` (10902 ms): window not checked: this is a Wayland session: GNOME offers no way to list native Wayland windows from outside (the X11 probes see XWayland windows only); run the window cases on an X11 session; the app stayed up and its daemon answers; log hygiene: 2 WARN line(s) in the case's logs
- `linux.second_launch` (6937 ms): second launch: exit 0 (0x00000000) after 15 ms; log hygiene: 2 WARN line(s) in the case's logs
- `linux.tray_icon` (6916 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-1185065-1; log hygiene: 2 WARN line(s) in the case's logs
- `linux.vaapi_encoder` (1070 ms): vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder; profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2; daemon pid 1185204 answered after 281 ms; diagnostics: - VAAPI render device /dev/dri/renderD128; log hygiene: 1 WARN line...
- `regress.encoder_unavailable_wording` (1316 ms): daemon pid 1185382 answered after 282 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 2 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
