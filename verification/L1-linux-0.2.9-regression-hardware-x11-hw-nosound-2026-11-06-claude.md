# L1 linux 0.2.9 regression report (hardware)

- **Date:** 2026-11-06T17:32:20Z to 2026-11-06T17:39:01Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: n/a
- **Build:** commit unknown, package folder `/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out /home/trung/lab/regress-out/20261106T173219Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,mic,camera`, tier `hardware`
- **Result:** 35 passed, 17 failed, 25 skipped of 77 cases
- **SHA-256 of the files under test:**
  - `capturemann` `cf059d0f373a147816b10fac2599fe79a7ae1d36474bf179a709841ed731b0c9`
  - `cm-mcp` `fdaf0f0b9ae5e835d70857cb499a5512ecb3666652dfd60b310006250b582ead`
  - `cm-daemon` `0df2ab34585d65eb62e1813de5ac926a8be8641a9fd1cea78d6b207f3254fafb`
  - `cm-desktop` `8b95538f254b532d2c58a2bf5d505e6d0318582748b84ee763952741def78e84`
  - `cm-overlay` `ffe785f5cee771d7b0c5838fbe268ea4154bcd82f554feba0fa5dd03eb640435`
  - `ffmpeg` `1b928d7b7170b240a6efbafae9b1416812b85a921d54604a7a4bd5f06a91c400`
  - `ffprobe` `45dbb102c7babc4d45eef0b4365fbd2b08fabf55f448f6c9278a65d07780eae7`
  - `llama-server` `d693fb41e91a3329006276137757a600c8861e7a501468898dfa6fcb2ec10132`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `pkg.files` | FAIL | the package holds the programs, FFmpeg, the helper and the licences: no LICENSE file; no THIRD_PARTY_LICENSES file |
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
| `shot.window` | SKIP | a window screenshot is saved at the size the window is listed with: not implemented for linux |
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: needs --allow second_monitor |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | SKIP | Auto-redact on a screenshot of a real window that shows an e-mail address: not implemented for linux |
| `import.formats` | FAIL | mp4, mov and mkv import as recordings with the right size and duration: import of /tmp/cmr27210/19/clips/clip-h264.mkv printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for editing (no hardware... |
| `import.rotated` | FAIL | a source with a rotation tag imports upright: import of /tmp/cmr27210/20/clips/clip-rotated.mp4 printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for editing (no hardware video encoder).","data... |
| `import.vfr` | FAIL | a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder: import of /tmp/cmr27210/21/clips/clip-vfr.mp4 printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for ... |
| `import.audio` | PASS | a WAV file imports as an audio capture of the right length |
| `import.errors` | PASS | not a media file, a missing file, a file held open and a Unicode name end cleanly |
| `import.dry_run` | PASS | import --dry-run reports and creates nothing |
| `export.quick` | FAIL | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range: the quick export is vp9 not h264 |
| `export.presets` | FAIL | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length: youtube-1080p: vp9 1920x1080 |
| `export.audio` | PASS | audio export to WAV and M4A has the right codec and length |
| `export.errors` | PASS | an unknown preset or capture is refused with the shared error codes |
| `mcp.handshake` | PASS | initialize answers with the server name, the product version and tool support |
| `mcp.tools_list` | PASS | tools/list matches the tool table of the spec (no drift either way) |
| `mcp.read_only_calls` | PASS | the read-only tools answer without error |
| `mcp.confirm_tier` | PASS | start_recording and delete_capture do not run without an on-screen confirmation |
| `mcp.policy_allow` | FAIL | agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP): with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked) |
| `mcp.request_id` | PASS | a repeated request_id returns the first result and acts once |
| `mcp.errors` | PASS | errors map to structuredContent.code, JSON-RPC codes and CLI exit codes |
| `mcp.events` | PASS | daemon events arrive as notifications (capture.revised, job.*) for changes any client makes |
| `mcp.cli_parity` | PASS | CLI and MCP return the same fields for library list, settings and targets |
| `mcp.http` | PASS | cm-mcp --http needs the bearer token and answers initialize |
| `rec.screen_cursor` | FAIL | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings: record start failed (killed by a signal): |
| `rec.region` | FAIL | a region recording is exactly the requested size: record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time. |
| `rec.region_min` | FAIL | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present: record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the deskt... |
| `rec.kill9_recovery` | FAIL | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s: record start failed (exit 5 (0x00000005)): platform_rejected The desktop portal refused screen capture (create session): Portal request didn't succeed with no information |
| `rec.mic` | FAIL | a microphone track is recorded for the length of the recording: record start failed (killed by a signal): |
| `rec.system_audio` | SKIP | system audio contains a tone played by another process: needs --allow sound |
| `rec.camera` | FAIL | a camera track keeps at least 24 fps in a lit room: record start failed (exit 1 (0x00000001)): not_implemented Camera recording is not available on Linux yet. |
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
| `desktop.no_console_flash` | SKIP | no console window appears during import, export and a screenshot: not implemented for linux |
| `desktop.settings_applied` | SKIP | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long: not implemented for linux |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: not implemented for linux |
| `desktop.tray_icon` | SKIP | the tray icon appears in the notification area while the app runs (UI Automation probe): not implemented for linux |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | FAIL | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own: left programs running: cm-desktop (32944) |
| `linux.second_launch` | FAIL | a second launch with a file forwards it to the running app and exits: left programs running: cm-desktop (33059) |
| `linux.tray_icon` | FAIL | the tray icon registers as a StatusNotifierItem while the app runs: left programs running: cm-desktop (33150) |
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
| AUD-1 | FAIL | `rec.mic`, `rec.system_audio` |
| AUD-2 | FAIL | `rec.mic` |
| AUD-4 | PASS | `export.audio` |
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
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | FAIL | `desktop.second_launch`, `linux.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
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
| VOC-2 | PASS | `import.audio`, `export.audio` |
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
| errors: invalid_input, not_found | PASS | `shot.invalid`, `export.errors` |
| make verify-release | not run | `installer.dmg` |
| named pipe single instance | not run | `desktop.second_launch` |
| packaging/linux/build-deb.sh | not run | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| packaging/macos | not run | `installer.dmg` |
| platform-acceptance | PASS | `linux.session` |
| recording events | PASS | `mcp.events` |
| recording.lock | FAIL | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | not run | `installer.silent` |

## Problems found

### `pkg.files`: the package holds the programs, FFmpeg, the helper and the licences

- no LICENSE file; no THIRD_PARTY_LICENSES file

Evidence:

```text
capturemann: 4476440 bytes
cm-mcp: 9411520 bytes
cm-daemon: 96017008 bytes
cm-desktop: 147930952 bytes
cm-overlay: 10305552 bytes
licence: None, third party: None, notice: None
note: no README in the package folder
```

### `import.formats`: mp4, mov and mkv import as recordings with the right size and duration

- import of /tmp/cmr27210/19/clips/clip-h264.mkv printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for editing (no hardware video encoder).","data":{"job_id":"job_b6867c0aa940e5cc"},"exit_code":6}}

Evidence:

```text
daemon pid 28492 answered after 282 ms
clip-h264.mp4: rec_06GV3TVAF7TXJ3RK, 320x240, 3.00 s, job 143 ms
clip-h264.mov: rec_06GV3TVBS8FMR5CS, 320x240, 3.00 s, job 159 ms
```

### `import.rotated`: a source with a rotation tag imports upright

- import of /tmp/cmr27210/20/clips/clip-rotated.mp4 printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for editing (no hardware video encoder).","data":{"job_id":"job_838c6b418834b7dc"},"exit_code":6}}

Evidence:

```text
daemon pid 28805 answered after 282 ms
```

### `import.vfr`: a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder

- import of /tmp/cmr27210/21/clips/clip-vfr.mp4 printed no job (exit 6 (0x00000006)): {"error":{"code":"job_failed","message":"This computer can't prepare this file for editing (no hardware video encoder).","data":{"job_id":"job_8f137253579207b9"},"exit_code":6}}

Evidence:

```text
daemon pid 28870 answered after 282 ms
```

### `export.quick`: the quick export is H.264 MP4 of the right length, tagged BT.709 limited range

- the quick export is vp9 not h264

Evidence:

```text
daemon pid 29477 answered after 282 ms
export --preset quick: clip-h264 - quick.mp4 by libvpx-vp9, 809 ms
vp9 320x240 yuv420p range tv space bt709 trc  prim , 3.00 s
```

### `export.presets`: youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length

- youtube-1080p: vp9 1920x1080

Evidence:

```text
daemon pid 29839 answered after 282 ms
export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by libvpx-vp9, 4870 ms
youtube-1080p: vp9 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1811623 bytes)
export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by libvpx-vp9, 4190 ms
vertical-1080: vp9 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2244159 bytes)
export --preset square-1080: clip-h264 - square-1080.mp4 by libvpx-vp9, 3606 ms
square-1080: vp9 1080x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1723081 bytes)
export --preset webm: clip-h264 - webm.webm by libvpx-vp9, 761 ms
webm: vp9 320x240 3.01 s matroska,webm fps Some(30.0) (121447 bytes)
export --preset gif: clip-h264 - gif.gif by gif, 3076 ms
gif: gif 800x600 3.00 s gif fps Some(16.666666666666668) (2084503 bytes)
```

### `mcp.policy_allow`: agents.tool_policy allow for start_recording is honoured by the daemon (CLI and MCP)

- with the policy set to allow, the CLI start of a bogus display gave None (expected not_found, i.e. no confirmation asked)

Evidence:

```text
daemon pid 31573 answered after 282 ms
CLI with policy allow: killed by a signal None
```

### `rec.screen_cursor`: screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings

- record start failed (killed by a signal):

Evidence:

```text
daemon pid 32125 answered after 281 ms
```

### `rec.region`: a region recording is exactly the requested size

- record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.

Evidence:

```text
daemon pid 32234 answered after 282 ms
```

### `rec.region_min`: the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present

- record start failed (exit 1 (0x00000001)): not_implemented Recording a window or a region is not available on Linux yet. Record the whole screen; the desktop asks which one the first time.

Evidence:

```text
daemon pid 32296 answered after 282 ms
```

### `rec.kill9_recovery`: the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s

- record start failed (exit 5 (0x00000005)): platform_rejected The desktop portal refused screen capture (create session): Portal request didn't succeed with no information

Evidence:

```text
daemon pid 32359 answered after 281 ms
```

### `rec.mic`: a microphone track is recorded for the length of the recording

- record start failed (killed by a signal):

Evidence:

```text
daemon pid 32445 answered after 282 ms
```

### `rec.camera`: a camera track keeps at least 24 fps in a lit room

- record start failed (exit 1 (0x00000001)): not_implemented Camera recording is not available on Linux yet.

Evidence:

```text
daemon pid 32537 answered after 282 ms
```

### `linux.desktop_stays_up`: cm-desktop starts, stays up for 10 s and (X11) shows a window of its own

- left programs running: cm-desktop (32944)

Evidence:

```text
1 visible window(s) of the app: hwnd 0x2000001 pid 32944 class 'dev.capturemann.app' title 'CaptureMann' rect [66, 69, 1246, 829] ex 0x0
```

### `linux.second_launch`: a second launch with a file forwards it to the running app and exits

- left programs running: cm-desktop (33059)

Evidence:

```text
second launch: exit 0 (0x00000000) after 31 ms
```

### `linux.tray_icon`: the tray icon registers as a StatusNotifierItem while the app runs

- left programs running: cm-desktop (33150)

Evidence:

```text
tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-33150-1
```

### `linux.vaapi_encoder`: VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding

- vainfo lists H.264 encode but diagnostics says recording will use the software VP9 encoder

Evidence:

```text
vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder
profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2
daemon pid 33238 answered after 282 ms
diagnostics: - VAAPI render device /dev/dri/renderD128
```

## Not run

- give --soak-minutes <n> to run a long recording (1): rec.soak
- needs --allow second_monitor (2): shot.second_monitor, rec.external_4k
- needs --allow sound (1): rec.system_audio
- needs --installer <setup.exe> (2): linux.deb_install, linux.tarball_install
- needs --whisper-model <ggml file> (and --speaker-models <dir> for the speaker case) (2): ai.transcribe, ai.speakers
- no crash log is read on Linux yet (1): zz.crash_events
- no global-shortcut path on GNOME 46 Wayland and the suite never injects keys into the lab laptop's session; the overlay is checked by hand (tests/manual/CAP-1-overlay.md) (1): linux.hotkey_overlay
- not implemented for linux (13): shot.window, redact.window_text, rec.each_monitor, installer.dmg, installer.silent, desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.hud_click_through, desktop.no_console_flash, desktop.settings_applied, desktop.recording_placement, desktop.tray_icon
- the request was refused earlier (not_implemented), not by the encoder: could not provoke it (1): regress.encoder_unavailable_wording
- this system has an FFmpeg in a standard folder the suite cannot hide (- video encoder: software VP9 through ffmpeg at /usr/bin/ffmpeg) (1): regress.ffmpeg_missing_wording

## Evidence of passing cases

- `pkg.startup` (425 ms): capturemann --version: exit 0 (0x00000000) in 15 ms; cm-mcp --bogus: exit 2 (0x00000002) in 15 ms; cm-daemon --bogus: exit 2 (0x00000002) in 15 ms; cm-overlay --help: exit 0 (0x00000000) in 15 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 15 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 15 ms; llama-server --version: exit 0 (0x00000000) in 16 ms
- `pkg.diagnostics` (920 ms): version 0.2.9; daemon pid 27291 answered after 282 ms; - video encoder: software VP9 through ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at /opt/capturemann/lib/capturemann/ffmpeg; - llama-server at /opt/capturemann/lib/capturemann/llama-server (bundled with this build, llama.cpp v0.7.0); - text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4); - speaker engine built in: true; - spe...
- `daemon.lifecycle` (1263 ms): daemon pid 27354 answered after 282 ms; pid 27354, 86 actions; stop: clean exit on SIGTERM; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (1597 ms): daemon pid 27419 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (690 ms): daemon pid 27495 answered after 282 ms; second daemon: exit 0 (0x00000000) in 31 ms; output: [INFO cm_core::logging] daemon 0.2.9 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving /tmp/cmr27210/6/s6.sock; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (919 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [27572]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1042 ms): daemon pid 27603 answered after 282 ms; daemon pid 27662 answered after 283 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (795 ms): daemon pid 27759 answered after 282 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (731 ms): daemon pid 27856 answered after 282 ms; display 65 'eDP-1' 1920x1080 at (0,0) scale 1 primary; 0 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: ; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1285 ms): daemon pid 27909 answered after 282 ms; display 65 1920x1080; PNG 1920x1080; capture shot_06GV3TTET7CSAQCA; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (782 ms): daemon pid 27984 answered after 282 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (859 ms): daemon pid 28048 answered after 282 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (750 ms): daemon pid 28156 answered after 282 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (664 ms): daemon pid 28220 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (846 ms): daemon pid 28284 answered after 282 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (734 ms): daemon pid 28370 answered after 282 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1018 ms): daemon pid 28423 answered after 282 ms; recogniser Some(String("tesseract")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (926 ms): daemon pid 28934 answered after 282 ms; rec_06GV3TVRSVE4ERJ3: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (1415 ms): daemon pid 29052 answered after 282 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The file can't be read. It may be damaged, cut short or not a media file.","data":{"reason":"unreadable_file","files...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","message":"The video codec Png is not supported yet.","data":{"reason":"unsupported_format","files":[{"path":"/tmp/cmr27210/23/...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (812 ms): daemon pid 29396 answered after 282 ms; dry run: exit 0 (0x00000000) { "files": [ { "path": "/tmp/cmr27210/24/clips/clip-h264.mp4", "name": "clip-h264.mp4", "size_bytes": 86456, "title": "clip-h264", "kind": "video", "status": "ready", "reasons": [], "container": "mov"...; log hygiene: 1 WARN line(s) in the case's logs
- `export.audio` (1517 ms): daemon pid 30875 answered after 282 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1070 ms): daemon pid 31116 answered after 282 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (806 ms): daemon pid 31294 answered after 282 ms; server capturemann 0.2.9, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (696 ms): daemon pid 31369 answered after 282 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1274 ms): daemon pid 31423 answered after 282 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 497 chars; get_diagnostics: ok, 4424 chars; list_models: ok, 1923 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1300 chars; list_targets: ok, 248 chars; list_devices: ok, 1304 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (806 ms): daemon pid 31502 answered after 282 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (712 ms): daemon pid 31647 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (681 ms): daemon pid 31701 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (7836 ms): daemon pid 31781 answered after 281 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (862 ms): daemon pid 31974 answered after 281 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (934 ms): daemon pid 32072 answered after 282 ms; port 34051: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (1305 ms): daemon pid 32607 answered after 282 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1265 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 13.3.0 for Linux x86_64; daemon pid 32825 answered after 282 ms; log hygiene: 1 WARN line(s) in the case's logs
- `linux.session` (339 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (917 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 32891 answered after 282 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
