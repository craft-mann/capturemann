# L1 linux 0.2.6 regression report (hardware)

- **Date:** 2026-11-06T14:20:00Z to 2026-11-06T14:25:23Z
- **Who:** claude-orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: n/a
- **Build:** commit unknown, package folder `/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out /home/trung/lab/regress-out/20261106T141959Z --who claude-orchestrator --verdict PARTIAL --allow screen,sound,mic,camera`, tier `hardware`
- **Result:** 26 passed, 26 failed, 25 skipped of 77 cases
- **SHA-256 of the files under test:**
  - `capturemann` `0b686eba9fb5a8055ffd9326f40aef8330e3e4d0f9fdab0b9fe3e9d312c46b75`
  - `cm-mcp` `bf4ce173a625f22e526827ef3dbb0d82582ca7bacb0230a73f7a7e98933a588c`
  - `cm-daemon` `07f613d3303af7e7a97d9575bb060729cf655f8ffdf163afed4b94a86a3ca3bd`
  - `cm-desktop` `eafc57097cdca0b8339b9b3f21d94183e143288a190a1c8f24a1f29cf3de7bc1`
  - `cm-overlay` `ff6cc670214d44dfc3e1e226497cd6a6971872b5ab4f6ec54012485a1ad3fb7c`
  - `ffmpeg` `a50fdb123b89d9122b62da6c5030b6bb5e09eb9d8df7f07bd8cff3e8f91ef846`
  - `ffprobe` `c5bc5542f0cb02501a2431a6f474d5672a569401ec654c4a44c660919d5ca188`
  - `llama-server` `eb1ea7337742b604ecf1702486f21683fb2f065ad86cfd054e517170f31a1c62`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: no LICENSE file; no THIRD_PARTY_LICENSES file |
| `pkg.startup` | PASS | every program starts without a crash (illegal instruction, fast fail) |
| `pkg.diagnostics` | FAIL | diagnostics reports app, system, models and engines, FFmpeg and the helper: the llama-server line does not say `bundled with this build` |
| `daemon.lifecycle` | PASS | daemon starts on a private endpoint, answers, stops, and the endpoint is gone |
| `daemon.library_created` | PASS | a fresh library is created and lists no captures; no helper program runs while idle |
| `daemon.second_instance` | PASS | a second daemon on a busy endpoint exits cleanly and leaves the first one working |
| `daemon.autostart` | PASS | the CLI starts a daemon when none runs |
| `settings.roundtrip` | PASS | settings set and get round trip, and survive a daemon restart |
| `settings.app_only_refused` | PASS | app-only settings keys are refused to agents and change nothing |
| `targets.list` | PASS | targets lists displays and windows with readable names and real sizes |
| `shot.display` | PASS | a display screenshot is a valid PNG of the display's physical size |
| `shot.region` | PASS | a region screenshot has the requested size |
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for linux |
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for linux |
| `import.formats` | FAIL | mp4, mov and mkv import as recordings with the right size and duration: import of /tmp/cmr5408/19/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [O... |
| `import.rotated` | FAIL | a source with a rotation tag imports upright: import of /tmp/cmr5408/20/clips/clip-rotated.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For m... |
| `import.vfr` | FAIL | a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder: import of /tmp/cmr5408/21/clips/clip-vfr.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' U... |
| `import.audio` | FAIL | a WAV file imports as an audio capture of the right length: import of /tmp/cmr5408/22/tone.wav printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For m... |
| `import.errors` | FAIL | not a media file, a missing file, a file held open and a Unicode name end cleanly: import of /tmp/cmr5408/23/clips/Chào bạn – bản ghi màn hình đầy đủ ✓.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand ... |
| `import.dry_run` | FAIL | import --dry-run reports and creates nothing: import --dry-run failed: error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'. |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: import of /tmp/cmr5408/25/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: captur... |
| `export.presets` | FAIL | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length: import of /tmp/cmr5408/26/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export'... |
| `export.audio` | FAIL | audio export to WAV and M4A has the right codec and length: import of /tmp/cmr5408/27/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COM... |
| `export.errors` | FAIL | an unknown preset or capture is refused with the shared error codes: import of /tmp/cmr5408/28/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTI... |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | FAIL | tools/list matches the tool table of the spec (no drift either way): tools the spec lists as built but the server lacks: get_cursor_track, normalize_transcript, get_normalized_transcript, import_media, get_platform_rules |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | FAIL | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP): with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | FAIL | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes: import_media: JSON-RPC error -32602: unknown tool `import_media` |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `rec.screen_cursor` | FAIL | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings: the recording is 15.58 s in the manifest but about 6.03 s were recorded outside the 2 s pause |
| `rec.region` | FAIL | a region recording is exactly the requested size: record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time. |
| `rec.region_min` | FAIL | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present: record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the deskt... |
| `rec.kill9_recovery` | FAIL | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s: recovery lost 4.03 s (limit 3 s) |
| `rec.mic` | FAIL | a microphone track is recorded for the length of the recording: record start failed (killed by a signal): |
| `rec.system_audio` | SKIP | system audio contains a tone played by another process: no tone player for Linux yet |
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: record start failed (exit 5 (0x00000005)): platform_rejected The desktop portal cannot share the screen. The desktop portal did not answer within 3 s. |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: needs --allow second_monitor |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: not implemented for linux |
| `rec.soak` | SKIP | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames: give --soak-minutes <n> to run a long recording |
| `ai.no_model` | FAIL | without a model, transcribe and the text model say what to do (exit 2) instead of failing: import of /tmp/cmr5408/47/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Us... |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | SKIP | a transcription with a real model runs to the end without a crash: needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) |
| `installer.dmg` | SKIP | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing: not implemented for linux |
| `installer.silent` | SKIP | silent install into a temp folder, the programs run, silent uninstall leaves nothing: not implemented for linux |
| `desktop.stays_up` | SKIP | cm-desktop starts, stays up for 10 s and leaves no crash event: not implemented for linux |
| `desktop.second_launch` | SKIP | a second launch with a file forwards it to the running app and exits: not implemented for linux |
| `desktop.hotkey_overlay` | SKIP | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it: not implemented for linux |
| `desktop.hud_click_through` | SKIP | during a recording the HUD answers clicks and the area window is click-through: not implemented for linux |
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for linux |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | FAIL | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: left programs running: cm-desktop (9179) |
| `linux.second_launch` | FAIL | a second launch with a file forwards it to the running app and exits: left programs running: cm-desktop (9306) |
| `linux.tray_icon` | FAIL | the tray icon registers as a StatusNotifierItem while the app runs: left programs running: cm-desktop (9394) |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) |
| `linux.vaapi_encoder` | FAIL | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding: vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder |
| `linux.deb_install` | SKIP | the .deb installs, its programs find their libraries and run, remove and purge leave nothing: needs --installer <setup.exe> |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: needs --installer <setup.exe> |
| `regress.ffmpeg_missing_wording` | SKIP | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows): this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg) |
| `regress.encoder_unavailable_wording` | SKIP | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle: the request was refused earlier (not_implemented), not by the encoder: could not provoke it |
| `zz.crash_events` | SKIP | no crash of a CaptureMann program was logged (Windows event log, macOS DiagnosticReports) during the whole run: no crash log is read on Linux yet |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT idempotency | PASS | `mcp.request_id` |
| AGT tiers: Confirm | PASS | `lib.delete_needs_confirmation` |
| AGT-1 | PASS | `daemon.lifecycle`, `daemon.second_instance`, `daemon.autostart`, `settings.roundtrip`, `mcp.errors`, `desktop.settings_applied` |
| AGT-1: every tool has the same name as its CLI subcommand | PASS | `mcp.cli_parity` |
| AGT-2 | FAIL | `mcp.handshake`, `mcp.tools_list`, `mcp.read_only_calls` |
| AGT-2 events | FAIL | `mcp.events` |
| AGT-2 transport | PASS | `mcp.http` |
| AGT-3 | PASS | `lib.annotate_undo` |
| AGT-POLICY | FAIL | `settings.app_only_refused`, `lib.delete_needs_confirmation`, `mcp.confirm_tier`, `mcp.policy_allow` |
| AI-1 | FAIL | `ai.no_model`, `ai.transcribe` |
| AI-3 | not run | `ai.transcribe` |
| AI-7 | FAIL | `ai.no_model`, `ai.llama_version` |
| AI-7: the text engine starts only when asked | PASS | `daemon.library_created` |
| AI-8 | not run | `ai.speakers` |
| AI-9 / RED-1: Auto-redact | PASS | `redact.fixture` |
| ANN-2 | PASS | `redact.fixture`, `redact.window_text` |
| ANN-3 | PASS | `lib.annotate_undo` |
| AUD-1 | FAIL | `rec.mic`, `rec.system_audio` |
| AUD-2 | FAIL | `rec.mic` |
| AUD-4 | FAIL | `export.audio` |
| CAM-1 | FAIL | `rec.camera` |
| CAM-2 | FAIL | `rec.camera` |
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | not run | `desktop.hotkey_overlay`, `linux.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | FAIL | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | not run | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay`, `linux.displays` |
| CAP-4: export_image | PASS | `lib.manifest` |
| D-R2 software VP9 fallback | FAIL | `linux.vaapi_encoder` |
| DIAG-1 | FAIL | `pkg.diagnostics`, `linux.vaapi_encoder` |
| DPI-WIN | not run | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | FAIL | `export.quick`, `export.errors` |
| EXP-2 | FAIL | `export.presets` |
| EXP-3 | FAIL | `export.presets` |
| IMP-1 | FAIL | `import.formats`, `import.audio` |
| IMP-2 | FAIL | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | FAIL | `import.errors` |
| IMP-4 | FAIL | `import.dry_run` |
| IMP-4: second launch opens Import | FAIL | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | FAIL | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| LINUX-1 | FAIL | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | FAIL | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-licence | FAIL | `pkg.files` |
| O7 installer | not run | `installer.silent` |
| OD-24 | FAIL | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | FAIL | `pkg.files` |
| PKG-deb | not run | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | not run | `regress.ffmpeg_missing_wording` |
| REC-1 | FAIL | `rec.screen_cursor`, `linux.vaapi_encoder` |
| REC-1.AC | FAIL | `rec.region`, `rec.region_min` |
| REC-2 | FAIL | `rec.screen_cursor` |
| REC-3 | FAIL | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | FAIL | `rec.screen_cursor` |
| REC-5 | not run | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 tray | FAIL | `desktop.tray_icon`, `linux.tray_icon` |
| REC-7 | FAIL | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | FAIL | `rec.kill9_recovery` |
| REC-WIN gate | not run | `rec.soak` |
| REC-WIN section 5 | not run | `regress.encoder_unavailable_wording` |
| RED-1 | not run | `redact.window_text` |
| VOC-2 | FAIL | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | FAIL | `rec.camera` |
| bug class: crash at start | FAIL | `linux.desktop_stays_up` |
| bug class: illegal instruction | not run | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | not run | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | FAIL | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
| bug: 54 % late frames on a static screen | FAIL | `rec.screen_cursor` |
| bug: FFmpeg-missing text named Homebrew on Windows | not run | `regress.ffmpeg_missing_wording` |
| bug: Import copy not constant 30 fps | FAIL | `import.vfr` |
| bug: Import editing copy not upright | FAIL | `import.rotated` |
| bug: Transcribe illegal instruction (AVX-512 in the build) | PASS | `pkg.startup` |
| bug: Transcribe illegal instruction (AVX-512) | not run | `ai.transcribe` |
| bug: Windows Terminal Host name | PASS | `targets.list` |
| bug: area window swallowed clicks meant for the HUD | not run | `desktop.hud_click_through` |
| bug: cm-desktop panic at start (tokio pipe outside a runtime) | not run | `desktop.stays_up` |
| bug: console window flashes (child programs without CREATE_NO_WINDOW) | not run | `desktop.no_console_flash` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | FAIL | `export.quick` |
| bug: hardware_encoder_unavailable text named a Mac | not run | `regress.encoder_unavailable_wording` |
| bug: illegal instruction | not run | `zz.crash_events` |
| bug: recording stop crash 0xC0000409 | FAIL | `rec.screen_cursor`, `zz.crash_events` |
| bug: settings set not applied to the running app | not run | `desktop.settings_applied` |
| bug: window name garbled | PASS | `targets.list` |
| bug: window shot saved at 960x485 while listed 1426x721 (headless daemon not DPI aware) | not run | `shot.window` |
| bug: yuv420p refused by Quick Sync (NV12 needed) | FAIL | `import.vfr` |
| data model: bundles are the source of truth | PASS | `lib.manifest` |
| errors: invalid_input, not_found | FAIL | `shot.invalid`, `export.errors` |
| make verify-release | not run | `installer.dmg` |
| named pipe single instance | not run | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | PASS | `linux.session` |
| recording events | FAIL | `mcp.events` |
| recording.lock | FAIL | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | FAIL | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- no LICENSE file; no THIRD_PARTY_LICENSES file

Evidence:

```text
capturemann: 4249672 bytes
cm-mcp: 9240448 bytes
cm-daemon: 93259440 bytes
cm-desktop: 144953024 bytes
cm-overlay: 10249136 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

### `pkg.diagnostics`: diagnostics reports app, system, models and engines, FFmpeg and the helper

- the llama-server line does not say `bundled with this build`

Evidence:

```text
version 0.2.6
daemon pid 5489 answered after 282 ms
- video encoder: software VP9 through ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg
- speech model speaker-segmentation (6 MB)
- speech model speaker-embedding (29 MB)
- ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg
- llama-server at /opt/capturemann/lib/capturemann/llama-server
- text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4)
- speaker engine built in: true
```

### `import.formats`: mp4, mov and mkv import as recordings with the right size and duration

- import of /tmp/cmr5408/19/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 6691 answered after 282 ms
```

### `import.rotated`: a source with a rotation tag imports upright

- import of /tmp/cmr5408/20/clips/clip-rotated.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 6742 answered after 282 ms
```

### `import.vfr`: a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder

- import of /tmp/cmr5408/21/clips/clip-vfr.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 6793 answered after 282 ms
```

### `import.audio`: a WAV file imports as an audio capture of the right length

- import of /tmp/cmr5408/22/tone.wav printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 6844 answered after 282 ms
```

### `import.errors`: not a media file, a missing file, a file held open and a Unicode name end cleanly

- import of /tmp/cmr5408/23/clips/Chào bạn – bản ghi màn hình đầy đủ ✓.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 6895 answered after 282 ms
not a media file: exit 2 (0x00000002) state None error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.
a PNG picture: exit 2 (0x00000002) state None error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.
missing file: exit 2 (0x00000002) None
file open elsewhere: exit 2 (0x00000002) state None error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.
```

### `import.dry_run`: import --dry-run reports and creates nothing

- import --dry-run failed: error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 7012 answered after 282 ms
dry run: exit 2 (0x00000002) 
```

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- import of /tmp/cmr5408/25/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 7063 answered after 282 ms
```

### `export.presets`: youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length

- import of /tmp/cmr5408/26/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 7114 answered after 282 ms
```

### `export.audio`: audio export to WAV and M4A has the right codec and length

- import of /tmp/cmr5408/27/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 7165 answered after 282 ms
```

### `export.errors`: an unknown preset or capture is refused with the shared error codes

- import of /tmp/cmr5408/28/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 7216 answered after 282 ms
```

### `mcp.tools_list`: tools/list matches the tool table of the spec (no drift either way)

- tools the spec lists as built but the server lacks: get_cursor_track, normalize_transcript, get_normalized_transcript, import_media, get_platform_rules

Evidence:

```text
daemon pid 7342 answered after 282 ms
74 tools
```

### `mcp.policy_allow`: agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP)

- with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked)

Evidence:

```text
daemon pid 7544 answered after 282 ms
CLI with policy allow: exit 0 (0x00000000) None
```

### `mcp.events`: daemon events arrive as notifications (capture.revised, job.*) for changes any client makes

- import_media: JSON-RPC error -32602: unknown tool `import_media`

Evidence:

```text
daemon pid 7737 answered after 282 ms
1 notification(s); capture.revised seen: true
```

### `rec.screen_cursor`: screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings

- the recording is 15.58 s in the manifest but about 6.03 s were recorded outside the 2 s pause

Evidence:

```text
daemon pid 7958 answered after 282 ms
started rec_01a1b13bd35b_0001
export job job_f770b886065e4067: done
screen: vp9 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.07 s; manifest duration Some(Number(1869823)) ticks
manifest 15.58 s, expected 6.03 s (recorded at the pause plus the time after the resume)
```

### `rec.region`: a region recording is exactly the requested size

- record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.

Evidence:

```text
daemon pid 8477 answered after 282 ms
```

### `rec.region_min`: the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present

- record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.

Evidence:

```text
daemon pid 8539 answered after 282 ms
```

### `rec.kill9_recovery`: the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s

- recovery lost 4.03 s (limit 3 s)

Evidence:

```text
daemon pid 8601 answered after 282 ms
started rec_01a1b13c4275_0001
before the kill: state recording recorded 6.0 s
daemon pid 8727 answered after 282 ms
recovered: 2.00 s of about 6.0 s recorded before the kill
```

### `rec.mic`: a microphone track is recorded for the length of the recording

- record start failed (killed by a signal):

Evidence:

```text
daemon pid 8770 answered after 282 ms
```

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- record start failed (exit 5 (0x00000005)): platform_rejected The desktop portal cannot share the screen. The desktop portal did not answer within 3 s.

Evidence:

```text
daemon pid 8867 answered after 282 ms
```

### `ai.no_model`: without a model, transcribe and the text model say what to do (exit 2) instead of failing

- import of /tmp/cmr5408/47/clips/clip-h264.mp4 printed no job (exit 2 (0x00000002)): error: unrecognized subcommand 'import' tip: a similar subcommand exists: 'export' Usage: capturemann [OPTIONS] <COMMAND> For more information, try '--help'.

Evidence:

```text
daemon pid 8954 answered after 281 ms
```

### `linux.desktop_stays_up`: cm-desktop starts, stays up for 10 s and (X11) shows a window of its own

- left programs running: cm-desktop (9179)

Evidence:

```text
1 visible window(s) of the app: hwnd 0x2000001 pid 9179 class 'dev.capturemann.app' title 'CaptureMann' rect [66, 69, 1246, 829] ex 0x0
```

### `linux.second_launch`: a second launch with a file forwards it to the running app and exits

- left programs running: cm-desktop (9306)

Evidence:

```text
second launch: exit 0 (0x00000000) after 31 ms
```

### `linux.tray_icon`: the tray icon registers as a StatusNotifierItem while the app runs

- left programs running: cm-desktop (9394)

Evidence:

```text
tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-9394-1
```

### `linux.vaapi_encoder`: VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding

- vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder

Evidence:

```text
vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder
profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2
daemon pid 9484 answered after 281 ms
diagnostics: - VAAPI render device /dev/dri/renderD128
```

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (2): shot.second_monitor, rec.external_4k
- needs --installer <setup.exe> (2): linux.deb_install, linux.tarball_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- no crash log is read on Linux yet (1): zz.crash_events
- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- no tone player for Linux yet (1): rec.system_audio
- not implemented for linux (13): shot.window, redact.window_text, rec.each_monitor, installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, desktop.tray_icon
- the request was refused earlier (not_implemented), not by the encoder: could not provoke it (1): regress.encoder_unavailable_wording
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.startup` (425 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `daemon.lifecycle` (1262 ms): daemon pid 5551 answered after 282 ms; pid 5551, 81 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1581 ms): daemon pid 5616 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (674 ms): daemon pid 5694 answered after 282 ms; second daemon: exit 0 (0x00000000) in 15 ms; output: [INFO cm_core::logging] daemon 0.2.6 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr5408/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (919 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [5771]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1057 ms): daemon pid 5802 answered after 282 ms; daemon pid 5861 answered after 282 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (797 ms): daemon pid 5958 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x000...
- `targets.list` (662 ms): daemon pid 6055 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 1 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Gnome-terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1193 ms): daemon pid 6108 answered after 282 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GV2ESST3M3EG04; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (754 ms): daemon pid 6183 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (859 ms): daemon pid 6247 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (749 ms): daemon pid 6355 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (677 ms): daemon pid 6419 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (862 ms): daemon pid 6483 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (728 ms): daemon pid 6569 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (917 ms): daemon pid 6622 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (804 ms): daemon pid 7267 answered after 282 ms; server capturemann 0.2.6, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1332 ms): daemon pid 7395 answered after 282 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 496 chars; get_diagnostics: ok, 3816 chars; list_models: ok, 1878 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1284 chars; list_targets: ok, 248 chars; list_devices: ok, 1304 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (822 ms): daemon pid 7474 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (746 ms): daemon pid 7618 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (803 ms): daemon pid 7672 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (861 ms): daemon pid 7807 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (935 ms): daemon pid 7905 answered after 282 ms; port 46237: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.llama_version` (1145 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 9035 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (632 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (826 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 9122 answered after 282 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
