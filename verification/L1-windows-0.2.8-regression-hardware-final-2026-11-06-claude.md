# L1 windows 0.2.8 regression report (hardware)

- **Date:** 2026-11-06T04:45:09Z to 2026-11-06T04:56:07Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit eb3cb47c239db609fa433e075d1df2130c532960, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **Final release-candidate run:** commit eb3cb47c239db609fa433e075d1df2130c532960 (package, head of track/windows); package run 42160646234 (version 0.2.8; zip SHA-256 `0385831a2664f802a07eb0eb2f3c06074b75a8e89f08761b0c1b2f82ba526b86`, setup.exe SHA-256 `5d173869b6b393eb56c0b9d064d553287922974ed4209170dd263d603457899e`); test programs run 42159767107 (commit 3a2067a2b342a3ae39832730c14875751814e816, `cm-regress` unchanged since). Verdict PARTIAL; two 1920x1080 / 3840x2160 monitors at 150 %; the maintainer's monitor content was left as it was.
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --out C:\lab\regress-out\20261106T044506Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,sound,mic,camera,second_monitor --soak-minutes 3 --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --installer C:\lab\capturemann-0.2.8-windows-x64-setup.exe --commit eb3cb47c239db609fa433e075d1df2130c532960`, tier `hardware`
- **Result:** 64 passed, 2 failed, 1 skipped of 67 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `9fd2c3b855167ea45af84bf8692500d167afcb704b848970e900faec2a0c8bd7`
  - `cm-mcp.exe` `8b3edef2119bd02f8901e2d126a809ce6cc53c6c363b2b20727aa6a49a8145ba`
  - `cm-daemon.exe` `fe5a069f7dd1ff59a7d55e2ca289351372bd5f4c521cd619d0eba0eaa95563fe`
  - `cm-desktop.exe` `06566742e157e32486ef9e8f4677a7c608e3be1c0830460dabe72d59e4bc3925`
  - `cm-overlay.exe` `1742f13a1db6f7f06b05b675399ff96a901bc75436b8080280b132249db52d5a`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`
  - `capturemann-0.2.8-windows-x64-setup.exe` `5d173869b6b393eb56c0b9d064d553287922974ed4209170dd263d603457899e`

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
| `shot.second_monitor` | PASS | every extra monitor lists and captures at its own physical size |
| `shot.invalid` | PASS | bad targets are refused with the shared error codes and create no capture |
| `lib.manifest` | PASS | the library lists a bundle; manifest.json has the schema keys; export of the image is a valid PNG |
| `lib.rename` | PASS | rename changes the title and the bundle folder, and is not an undo step |
| `lib.annotate_undo` | PASS | annotate adds a revision; undo and redo move through it; one call is one step |
| `lib.delete_needs_confirmation` | PASS | deleting a capture is refused without an on-screen confirmation and deletes nothing |
| `redact.fixture` | PASS | Auto-redact finds the e-mail address and the IPv4 address in a picture with known text |
| `redact.window_text` | PASS | Auto-redact on a screenshot of a real window that shows an e-mail address |
| `import.formats` | PASS | mp4, mov and mkv import as recordings with the right size and duration |
| `import.rotated` | PASS | a source with a rotation tag imports upright |
| `import.vfr` | PASS | a variable-frame-rate source gets a constant-rate editing copy through the hardware encoder |
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
| `rec.screen_cursor` | PASS | screen and cursor, pause and resume: H.264, constant 30 fps, 2 s key frames, right length, no warnings |
| `rec.region` | PASS | a region recording is exactly the requested size |
| `rec.region_min` | PASS | the smallest recording areas (18x18 and 20x20) record with the real encoder: exact size, frames present |
| `rec.kill9_recovery` | PASS | the daemon killed mid-recording: the next start recovers the bundle and loses under 3 s |
| `rec.mic` | PASS | a microphone track is recorded for the length of the recording |
| `rec.system_audio` | PASS | system audio contains a tone played by another process |
| `rec.camera` | PASS | a camera track keeps at least 24 fps in a lit room |
| `rec.external_4k` | FAIL | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: display 11142891: the video is 3840x2160, the display is 5760x3240 physical px |
| `rec.each_monitor` | FAIL | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: display 27526735: the video is 1920x1080, the display is 2880x1620 physical px; display 11142891: the video is 3840x2160, the display is 5760x3240 physi... |
| `rec.soak` | PASS | a long recording (--soak-minutes) keeps constant 30 fps and 2 s key frames |
| `ai.no_model` | PASS | without a model, transcribe and the text model say what to do (exit 2) instead of failing |
| `ai.llama_version` | PASS | the bundled llama-server runs --version and is not started by the daemon |
| `ai.transcribe` | PASS | a transcription with a real model runs to the end without a crash |
| `ai.speakers` | SKIP | speaker labels: two voices give two speakers with the speaker models (--speech, --speaker-models): no --speaker-models <dir> given (the two ONNX files; this package bundles none) |
| `installer.silent` | PASS | silent install into a temp folder, the programs run, silent uninstall leaves nothing |
| `desktop.stays_up` | PASS | cm-desktop starts, stays up for 10 s and leaves no crash event |
| `desktop.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `desktop.hotkey_overlay` | PASS | Ctrl+Alt+Shift+1 opens one overlay window per monitor on its exact rectangle; Esc ends it |
| `desktop.hud_click_through` | PASS | during a recording the HUD answers clicks and the area window is click-through |
| `desktop.no_console_flash` | PASS | no console window appears during import, export and a screenshot |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | PASS | countdown, area window and HUD of a recording of the external monitor appear on that monitor |
| `desktop.tray_icon` | PASS | the tray icon appears in the notification area while the app runs (UI Automation probe) |
| `regress.ffmpeg_missing_wording` | PASS | with no FFmpeg anywhere the message says so in the platform's own words (no Homebrew on Windows) |
| `regress.encoder_unavailable_wording` | PASS | a recording the hardware encoder refuses ends with neutral wording and leaves no bundle |
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
| AI-3 | PASS | `ai.transcribe` |
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
| CAP-1 | PASS | `targets.list`, `shot.invalid` |
| CAP-1 overlay | PASS | `desktop.hotkey_overlay` |
| CAP-1.AC1 | PASS | `shot.display`, `shot.second_monitor` |
| CAP-1.AC2 | PASS | `shot.region`, `rec.region`, `rec.region_min` |
| CAP-1.AC4 | PASS | `shot.window` |
| CAP-3 | PASS | `targets.list`, `desktop.hotkey_overlay` |
| CAP-4: export_image | PASS | `lib.manifest` |
| DIAG-1 | PASS | `pkg.diagnostics` |
| DPI-WIN | PASS | `desktop.hotkey_overlay`, `desktop.recording_placement` |
| DPI-WIN layout A | PASS | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | FAIL | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick`, `export.errors` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| IMP-1 | PASS | `import.formats`, `import.audio` |
| IMP-2 | PASS | `import.formats`, `import.rotated`, `import.vfr` |
| IMP-3 | PASS | `import.errors` |
| IMP-4 | PASS | `import.dry_run` |
| IMP-4: second launch opens Import | PASS | `desktop.second_launch` |
| IMP-WIN error paths | PASS | `import.errors` |
| LIB-1 | PASS | `daemon.library_created`, `lib.manifest`, `lib.rename` |
| MCP protocol | PASS | `mcp.handshake` |
| NFR-2 | FAIL | `rec.screen_cursor`, `rec.external_4k`, `rec.soak` |
| NFR-licence | PASS | `pkg.files` |
| O7 installer | PASS | `installer.silent` |
| OD-24 | PASS | `settings.app_only_refused`, `mcp.policy_allow` |
| PKG-contents | PASS | `pkg.files` |
| R2-8 | PASS | `daemon.lifecycle` |
| R5-3 | PASS | `regress.ffmpeg_missing_wording` |
| REC-1 | PASS | `rec.screen_cursor` |
| REC-1.AC | PASS | `rec.region`, `rec.region_min` |
| REC-2 | PASS | `rec.screen_cursor` |
| REC-3 | FAIL | `rec.screen_cursor`, `rec.external_4k` |
| REC-4 | PASS | `rec.screen_cursor` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-5 tray | PASS | `desktop.tray_icon` |
| REC-7 | FAIL | `rec.screen_cursor`, `rec.each_monitor` |
| REC-8 | PASS | `rec.kill9_recovery` |
| REC-WIN gate | PASS | `rec.soak` |
| REC-WIN section 5 | PASS | `regress.encoder_unavailable_wording` |
| RED-1 | PASS | `redact.window_text` |
| VOC-2 | PASS | `import.audio`, `export.audio` |
| Windows.Media.Ocr recogniser | PASS | `redact.fixture` |
| bug class: camera 10 fps in low light | PASS | `rec.camera` |
| bug class: illegal instruction | PASS | `desktop.stays_up` |
| bug class: llama-server missing in the Windows package | PASS | `ai.llama_version` |
| bug class: process loopback | PASS | `rec.system_audio` |
| bug: 0xC0000409 at start | PASS | `pkg.startup` |
| bug: 16 px area fails on Intel UHD 620 | PASS | `rec.region_min` |
| bug: 4K recording of the external lost frames on the UHD 620 | FAIL | `rec.external_4k` |
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
| bug: countdown and toasts on the other monitor | PASS | `desktop.recording_placement` |
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
| named pipe single instance | PASS | `desktop.second_launch` |
| recording events | PASS | `mcp.events` |
| recording.lock | PASS | `rec.kill9_recovery` |
| specs/02-agent-interface.md tool table | PASS | `mcp.tools_list` |
| specs/README.md shared error codes | PASS | `mcp.errors` |
| tier: Confirm | PASS | `mcp.confirm_tier` |
| windows-package.yml silent install | PASS | `installer.silent` |

## Problems found

### `rec.external_4k`: 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps

- display 11142891: the video is 3840x2160, the display is 5760x3240 physical px

Evidence:

```text
daemon pid 5444 answered after 321 ms
started rec_01a1af2fa9a6_0001
export job job_baee5e1e948e0a34: done
display 11142891: video 3840x2160, display 3840x2160 at scale 1.5 = 5760x3240 physical
display 11142891: frame at 2 s vs its screenshot 0.035 (limit 0.15); vs others (frame, screenshots) [(0.725, 0.690)]; told apart from 1, not telling for 0 (screenshots too alike, under 0.05)
display 11142891 3840x2160: track 48000..4058989 ticks, file_end NaN, effective_fps NaN, late_pct None, dropped Some(Number(0)), rate_changes None
file 33.83 s
encoder_overloaded warning logged: false
file: ? frames, average 30.00 fps; manifest screen track: {"bitrate_bps":60000000,"bytes":87695974,"coalesced":0,"codec":"h264","cursor":true,"downscaled_from":null,"dropped":0,"effective_fps":30.0,"end":4058989,"file":"screen.mp4","file_end":4060000,"file_start":48000,"fps":30,"fragment_secs":[2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,2.0,1.4333333333333331],"fragments":17,"idle...
export job job_baee5e1e948e0a34: done
```

### `rec.each_monitor`: 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes

- display 27526735: the video is 1920x1080, the display is 2880x1620 physical px; display 11142891: the video is 3840x2160, the display is 5760x3240 physical px

Evidence:

```text
daemon pid 1964 answered after 320 ms
display 27526735 'Display 1' at (0,0) 1920x1080 scale 1.5 primary
display 11142891 'S27B80P' at (-3840,-1861) 3840x2160 scale 1.5
started rec_01a1af30e076_0001
export job job_dc8af5268860d951: done
display 27526735: video 1920x1080, display 1920x1080 at scale 1.5 = 2880x1620 physical
display 27526735: frame at 2 s vs its screenshot 0.024 (limit 0.15); vs others (frame, screenshots) [(0.667, 0.690)]; told apart from 1, not telling for 0 (screenshots too alike, under 0.05)
display 27526735: cursor header origin Some([0, 0]) size Some([1920, 1080]); 3 lines
display 27526735: cursor sprites {"images":{"c_2d00de8e3fd7":{"file":"c_2d00de8e3fd7.png","size":[48,48],"hotspot":[0,0],"scale":1.0}}}
started rec_01a1af312308_0002
export job job_8c6af776b2c110af: done
export job job_dc8af5268860d951: done
display 11142891: video 3840x2160, display 3840x2160 at scale 1.5 = 5760x3240 physical
display 11142891: frame at 2 s vs its screenshot 0.035 (limit 0.15); vs others (frame, screenshots) [(0.725, 0.690)]; told apart from 1, not telling for 0 (screenshots too alike, under 0.05)
display 11142891: cursor header origin Some([-3840, -1861]) size Some([3840, 2160]); 3 lines
display 11142891: cursor sprites {"images":{"c_2d00de8e3fd7":{"file":"c_2d00de8e3fd7.png","size":[48,48],"hotspot":[0,0],"scale":1.0}}}
```

## Not run

- no --speaker-models <dir> given (the two ONNX files; this package bundles none) (1): ai.speakers

## Evidence of passing cases

- `pkg.files` (719 ms): capturemann.exe: 4626432 bytes; cm-mcp.exe: 8208384 bytes; cm-daemon.exe: 56281600 bytes; cm-desktop.exe: 94331392 bytes; cm-overlay.exe: 2767872 bytes; licence: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\LICENSE"), third party: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\THIRD_PARTY_LICENSES.md"), notice: Some("C:\\Users\\trung\\AppData\\Local\\Programs\\CaptureMann\\doc\\NOTICE")
- `pkg.startup` (1027 ms): capturemann --version: exit 0 (0x00000000) in 33 ms; cm-mcp --bogus: exit 2 (0x00000002) in 36 ms; cm-daemon --bogus: exit 2 (0x00000002) in 52 ms; cm-overlay --help: exit 0 (0x00000000) in 21 ms; ffmpeg -hide_banner -version: exit 0 (0x00000000) in 33 ms; ffprobe -hide_banner -version: exit 0 (0x00000000) in 36 ms; llama-server --version: exit 0 (0x00000000) in 51 ms
- `pkg.diagnostics` (1479 ms): version 0.2.8; daemon pid 14304 answered after 321 ms; - windows x86_64; - GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086; - GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414; - speech model speaker-segmentation (6 MB); - speech model speaker-embedding (29 MB); - ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe; - llama-server at ~\AppData\Local\Programs\CaptureMann\l...
- `daemon.lifecycle` (2098 ms): daemon pid 5796 answered after 321 ms; pid 5796, 86 actions; stop: killed (Windows has no signal for a hidden process); log hygiene: 1 WARN line(s) in the case's logs
- `daemon.library_created` (2504 ms): daemon pid 5324 answered after 606 ms; log hygiene: 1 WARN line(s) in the case's logs
- `daemon.second_instance` (1734 ms): daemon pid 6896 answered after 321 ms; second daemon: exit 0 (0x00000000) in 49 ms; output: [INFO cm_core::logging] daemon 0.2.8 started [WARN cm_daemon] cm-daemon: the CaptureMann app or another daemon is already serving \\.\pipe\cm-regress-6284-6; nothing to do; log hygiene: 2 WARN line(s) in the case's logs
- `daemon.autostart` (2417 ms): piped run: exit 0 (0x00000000); new cm-daemon pids [14228]; log hygiene: 1 WARN line(s) in the case's logs
- `settings.roundtrip` (1860 ms): daemon pid 15068 answered after 320 ms; daemon pid 13960 answered after 317 ms; recording.fps=47: exit 0 (0x00000000) None; 47 was accepted: {"settings":{"recording.fps":47}}; log hygiene: 2 WARN line(s) in the case's logs
- `settings.app_only_refused` (1351 ms): daemon pid 15352 answered after 321 ms; set agents.tool_policy: exit 3 (0x00000003) Some("user_declined") `agents.tool_policy` can only be changed in the CaptureMann app (Settings); set library.path: exit 3 (0x00000003) Some("user_declined") `library.path` can only be changed in the CaptureMann app (Settings); set privacy.recording_indicator: exit 3 (0x00000003) Some("user_declined") `privacy.recording_indicator` can only be changed in the CaptureMann app (Settings); set auth.token: exit 2 (0x00...
- `targets.list` (1200 ms): daemon pid 10984 answered after 322 ms; display 27526735 'Display 1' 1920x1080 at (0,0) scale 1.5 primary; display 11142891 'S27B80P' 3840x2160 at (-3840,-1861) scale 1.5; 2 windows; capabilities {"capture_region":true,"capture_window":true,"list_windows":true,"pick":false}; apps: Settings, Windows Terminal; log hygiene: 1 WARN line(s) in the case's logs
- `shot.display` (1524 ms): daemon pid 14028 answered after 321 ms; display 27526735 1920x1080; PNG 1920x1080; capture shot_06GTYB9516EFMYHW; log hygiene: 1 WARN line(s) in the case's logs
- `shot.region` (1331 ms): daemon pid 2904 answered after 321 ms; region 100,100,400,300: PNG 400x300; log hygiene: 1 WARN line(s) in the case's logs
- `shot.window` (4709 ms): daemon pid 10360 answered after 321 ms; window 1574614: listed 1426x721, saved 1426x721; log hygiene: 1 WARN line(s) in the case's logs
- `shot.second_monitor` (4223 ms): daemon pid 4360 answered after 323 ms; display 27526735 'Display 1' at (0,0) listed 1920x1080, saved 1920x1080; display 11142891 'S27B80P' at (-3840,-1861) listed 3840x2160, saved 3840x2160; log hygiene: 1 WARN line(s) in the case's logs
- `shot.invalid` (1392 ms): daemon pid 9360 answered after 321 ms; unknown display: exit 2 (0x00000002) Some("not_found"); empty region: exit 2 (0x00000002) Some("invalid_input"); region off the display: exit 2 (0x00000002) Some("invalid_input"); unknown window: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `lib.manifest` (1260 ms): daemon pid 1192 answered after 321 ms; manifest keys ok; exported PNG 1000x260, 35254 bytes; log hygiene: 1 WARN line(s) in the case's logs
- `lib.rename` (1281 ms): daemon pid 12384 answered after 322 ms; log hygiene: 1 WARN line(s) in the case's logs
- `lib.annotate_undo` (1725 ms): daemon pid 6072 answered after 605 ms; after annotate: 1 op(s), 1 revision(s); redo with nothing to redo: exit 0 (0x00000000) None; log hygiene: 1 WARN line(s) in the case's logs
- `lib.delete_needs_confirmation` (1202 ms): daemon pid 3232 answered after 321 ms; library rm: exit 3 (0x00000003) Some("user_declined") No confirmation UI is attached to this daemon; log hygiene: 1 WARN line(s) in the case's logs
- `redact.fixture` (1339 ms): daemon pid 7272 answered after 321 ms; recogniser Some(String("windows_ocr")); 2 suggestion(s): email,ip_address; --redact-emails: exit 2 (0x00000002) None (not asserted); log hygiene: 1 WARN line(s) in the case's logs
- `redact.window_text` (4939 ms): daemon pid 15060 answered after 320 ms; window screenshot shot_06GTYBCG3QC0ENF3: suggestions ["email", "ip_address"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.formats` (4755 ms): daemon pid 3552 answered after 323 ms; clip-h264.mp4: rec_06GTYBCT3SVWS5EW, 320x240, 3.00 s, job 383 ms; clip-h264.mov: rec_06GTYBCWSH6XYF6R, 320x240, 3.00 s, job 386 ms; clip-h264.mkv: rec_06GTYBCZFMKCRW4A, 320x240, 3.02 s, job 1932 ms; log hygiene: 1 WARN line(s) in the case's logs
- `import.rotated` (3182 ms): daemon pid 5152 answered after 323 ms; library size 240x320 (stored 320x240 with a 90 degree tag); stored track screen.edit.mp4: 240x320 rotation 0 fps Some(30.0); log hygiene: 1 WARN line(s) in the case's logs
- `import.vfr` (3159 ms): daemon pid 1804 answered after 339 ms; stored track: h264 320x240 r Some(28.0) avg Some(28.0) 4.00 s; prepared ["variable_frame_rate"]; log hygiene: 1 WARN line(s) in the case's logs
- `import.audio` (1574 ms): daemon pid 8784 answered after 321 ms; rec_06GTYBEAE72K372Q: kind Some(String("audio")), 2.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `import.errors` (2233 ms): daemon pid 10304 answered after 337 ms; not a media file: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"error":{"message":"The file can't be read. It may be damaged, cut short or not a media file.","reason":"unre...; a PNG picture: exit 2 (0x00000002) state None {"error":{"code":"invalid_input","data":{"files":[{"container":"image2","duration_s":0.04,"error":{"codec":"png","message":"The video codec Png is not supported...; missing file: exit 2 (0x00000002) S...
- `import.dry_run` (1375 ms): daemon pid 9524 answered after 322 ms; dry run: exit 0 (0x00000000) { "files": [ { "audio_channels": 1, "audio_codec": "aac", "container": "mov", "duration_s": 3.0, "fps": 30.0, "hdr": false, "kind": "video", "name": "clip-h264.mp4", "other_audio_streams": 0, "path": ...; log hygiene: 1 WARN line(s) in the case's logs
- `export.quick` (2497 ms): daemon pid 9440 answered after 321 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 405 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (15981 ms): daemon pid 9220 answered after 606 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 2251 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 2363 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1763 ms; square-1080: h264 1080x1080 3.00 ...
- `export.audio` (3031 ms): daemon pid 2556 answered after 606 ms; export --audio --format wav: clip-h264 - audio.wav by , 0 ms; wav: pcm_s24le 48000 Hz 1 ch 3.00 s; export --audio --format m4a: clip-h264 - audio.m4a by , 0 ms; m4a: aac 48000 Hz 1 ch 3.00 s; log hygiene: 1 WARN line(s) in the case's logs
- `export.errors` (1915 ms): daemon pid 10064 answered after 322 ms; unknown preset: exit 2 (0x00000002) Some("invalid_input") preset: unknown preset `no-such-preset`; use one of: quick, youtube-4k, youtube-1440p, youtube-1080p, vertical-1080, square-1080, source, gi...; unknown capture: exit 2 (0x00000002) Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.handshake` (1317 ms): daemon pid 14348 answered after 321 ms; server capturemann 0.2.8, protocol "2025-06-18"; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.tools_list` (1258 ms): daemon pid 11900 answered after 320 ms; 79 tools; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.read_only_calls` (1849 ms): daemon pid 11060 answered after 606 ms; get_settings: ok, 15 chars; list_library: ok, 241 chars; get_capture: ok, 534 chars; get_diagnostics: ok, 4137 chars; list_models: ok, 2032 chars; get_text_model: ok, 253 chars; list_jobs: ok, 11 chars; list_destinations: ok, 19 chars; list_report_templates: ok, 2917 chars; list_text_models: ok, 1324 chars; list_targets: ok, 402 chars; list_devices: ok, 1369 chars; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.confirm_tier` (1293 ms): daemon pid 12096 answered after 322 ms; start_recording: error=true code=Some("user_declined") user_declined: No confirmation UI is attached to this daemon {"reason":"no_confirmer"}; get_recording_status after the refusal: state 'idle'; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.policy_allow` (1290 ms): daemon pid 4452 answered after 321 ms; CLI with policy allow: exit 2 (0x00000002) Some("not_found"); MCP with policy allow: Some("not_found"); log hygiene: 1 WARN line(s) in the case's logs
- `mcp.request_id` (1289 ms): daemon pid 2556 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.errors` (1533 ms): daemon pid 9360 answered after 605 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.events` (8322 ms): daemon pid 15220 answered after 337 ms; 1 notification(s); capture.revised seen: true; events after import: capture.revised, job.progress, job.progress, job.progress, job.progress, capture.created, job.progress, job.progress, job.done; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.cli_parity` (1291 ms): daemon pid 10760 answered after 320 ms; log hygiene: 1 WARN line(s) in the case's logs
- `mcp.http` (1379 ms): daemon pid 9980 answered after 320 ms; port 60939: no token 401, wrong token 401, right token 200; log hygiene: 1 WARN line(s) in the case's logs
- `rec.screen_cursor` (12413 ms): daemon pid 3192 answered after 320 ms; started rec_01a1af2ea80a_0001; export job job_bbed63954c63a403: done; screen: h264 1920x1080 yuv420p r 30.00 avg 30.00 frames None 6.40 s; manifest duration Some(Number(767364)) ticks; manifest 6.39 s, expected 6.04 s (recorded at the pause plus the time after the resume); 4 key frames, gaps Some((1.9999999999999996, 2.0)); cursor log: 5 lines, header {"clicks":"available","clock":"session host clock (QPC) at hook entry","move_source":"hook","permission_mis...
- `rec.region` (5584 ms): daemon pid 15348 answered after 320 ms; started rec_01a1af2ed9d5_0001; export job job_87a63ad4d921c9b0: done; region recording 640x360 h264; log hygiene: 1 WARN line(s) in the case's logs
- `rec.region_min` (9948 ms): daemon pid 5444 answered after 320 ms; started rec_01a1af2ef124_0001; export job job_1ceda01e39599927: failed; region 18x18: 18x18 h264 3.37 s; started rec_01a1af2f0290_0002; export job job_f481ebd227354e35: failed; export job job_1ceda01e39599927: failed; region 20x20: 20x20 h264 3.23 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.kill9_recovery` (8799 ms): daemon pid 12220 answered after 319 ms; started rec_01a1af2f1942_0001; before the kill: state recording recorded 6.0 s; daemon pid 9712 answered after 316 ms; recovered: 6.30 s of about 6.0 s recorded before the kill; log hygiene: 3 WARN line(s) in the case's logs
- `rec.mic` (7200 ms): daemon pid 1964 answered after 319 ms; started rec_01a1af2f3d66_0001; export job job_d5727d7e8d697256: done; mic track 4.07 s; mic level: rms -22.7 dBFS, peak -5.3 dBFS; log hygiene: 1 WARN line(s) in the case's logs
- `rec.system_audio` (9906 ms): daemon pid 9160 answered after 321 ms; started rec_01a1af2f5aa0_0001; export job job_9f605caa1804a149: done; system audio level: mean -6.3 dB, peak -0.1 dB (the tone is 1 kHz, 2 s, played by another process); log hygiene: 1 WARN line(s) in the case's logs
- `rec.camera` (9511 ms): daemon pid 5696 answered after 320 ms; started rec_01a1af2f82ce_0001; export job job_185933e59e10ca0f: done; camera 1280x720 h264 measured 29.2 fps, 5.50 s; log hygiene: 1 WARN line(s) in the case's logs
- `rec.soak` (182560 ms): daemon pid 12640 answered after 321 ms; started rec_01a1af31aa16_0001; 180 s: state recording recorded Some(Number(180217)) ms late Some(Number(0.0))%; soak: h264 1920x1080 avg Some(30.0) 180.6 s; log hygiene: 1 WARN line(s) in the case's logs
- `ai.no_model` (2060 ms): daemon pid 13436 answered after 321 ms; transcribe without a model: exit 2 (0x00000002) Some("invalid_input") No speech model is installed, so nothing can be transcribed. Open the AI page in CaptureMann, or run `capturemann ai models`, to download or import one. Nothing...; text-model test: exit 2 (0x00000002) Some("invalid_input"); MCP transcribe without a model: error true {"code":"invalid_input","data":{"hint":"capturemann ai models download small","reason":"no_model"},"message":"No speech mo...
- `ai.llama_version` (1980 ms): llama-server --version: exit 0 (0x00000000) version: 0.7.0-dev (build 0, commit unknown) built with GNU 16.2.0 for Windows AMD64; daemon pid 12892 answered after 319 ms; log hygiene: 1 WARN line(s) in the case's logs
- `ai.transcribe` (31776 ms): daemon pid 12700 answered after 320 ms; transcribe: exit 0 (0x00000000) { "attempts": 1, "created_at": "2026-11-06T04:53:38.182Z", "depends_on": [], "elapsed_ms": 29333, "error": null, "eta_ms": null, "finished_at": "2026-11-06T04:54:07.52Z", "held": false, "inputs": { "a...; library entry after transcription: {"broken":false,"created":"2026-11-06T04:53:38.0838938Z","duration":6015000,"height":null,"id":"rec_06GTYD47TW23PHHH","kind":"audio","publishes":[],"size_bytes":11241994,"source":{"importe...
- `installer.silent` (21282 ms): 1 uninstall key(s) of CaptureMann before: ["HKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\CaptureMann"]; saved before: 8 shared key(s), Start menu folder yes, user PATH yes; installer: exit 0 (0x00000000) in 14504 ms; installed: capturemann 0.2.8; daemon pid 9048 answered after 321 ms; uninstaller: exit 0 (0x00000000) in 1308 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.stays_up` (20141 ms): 1 visible window(s) of the app: 'CaptureMann' 1792x992
- `desktop.second_launch` (8242 ms): second launch: exit 0 (0x00000000) after 50 ms; visible windows of the first instance: 1 before, 1 after
- `desktop.hotkey_overlay` (6837 ms): hwnd 0x190666 pid 14216 class 'Window Class' title 'CaptureMann overlay' rect [-3840, -1861, 0, 299] ex 0x40018; hwnd 0xc06c6 pid 14216 class 'Window Class' title 'CaptureMann overlay' rect [0, 0, 1920, 1080] ex 0x40018
- `desktop.hud_click_through` (9856 ms): hwnd 0x18055e pid 15092 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188; hwnd 0x1305ce pid 15092 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0xd06c6 pid 15092 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x3b0690 pid 15092 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(1566,698) is the HUD; export job job_06143487d4f12252: done; log hygiene: 4 WARN line(s) in the case's ...
- `desktop.no_console_flash` (2966 ms): daemon pid 11600 answered after 321 ms; log hygiene: 1 WARN line(s) in the case's logs
- `desktop.settings_applied` (12196 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 7.5 s with capture.delay_s = 5
- `desktop.recording_placement` (14652 ms): display 27526735 'Display 1' (0,0) 1920x1080 scale 1.5 primary; display 11142891 'S27B80P' (-3840,-1861) 3840x2160 scale 1.5; countdown phase: hwnd 0x1605ce pid 1592 class 'Zed::Window' title '' rect [-2474, 27, -1366, 179] ex 0x200188 -> on the recorded monitor; countdown phase: hwnd 0x4a0464 pid 1592 class 'Zed::Window' title '' rect [-695, -9, -13, 209] ex 0x200188 -> on the recorded monitor; recording phase: hwnd 0x1605ce pid 1592 class 'Zed::Window' title '' rect [-2474, 27, -1366, 179] ex ...
- `desktop.tray_icon` (12140 ms): tray probe: exit 0 (0x00000000) TRAY 0; tray probe: exit 0 (0x00000000) TRAY 1 button; tray buttons named CaptureMann: 0 before the app, 1 with it
- `regress.ffmpeg_missing_wording` (1873 ms): daemon pid 10808 answered after 237 ms; diagnostics: - ffmpeg not found: looked next to ~\AppData\Local\Temp\cmr6284\65\bin-no-ffmpeg, in the settings and on PATH; this build should include it; import without FFmpeg: exit 2 (0x00000002) {"error":{"code":"not_found","data":null,"exit_code":2,"message":"FFmpeg was not found: CaptureMann includes its own copy, so reinstall CaptureMann, or set CM_FFMPEG to the path of ffmpeg.exe. (For development: winget install Gyan.FFmpeg.)"}}; log hygiene: 1 WARN...
- `regress.encoder_unavailable_wording` (1259 ms): daemon pid 8120 answered after 321 ms; 2x2 region: exit 2 (0x00000002) invalid_input: The recording area is too small: each side must be at least 18 px.; a tiny area is refused up front: invalid_input, too small; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.

## Notes added after the run (final release-candidate)

- `rec.external_4k` and `rec.each_monitor` FAIL: **suite bug, not a product bug.** The picture check expected the listed size times the scale (1920x1080 at 150 % = 2880x1620). Windows lists physical px (the screenshot of the same display is 1920x1080 / 3840x2160 and the video matches it exactly, frame rate 30.00, 0 dropped, 17 fragments of 2 s). Fixed in `cm-regress` (commit 7fab9942: the expected size comes from the display's own screenshot). Needs a new `Windows tests` build to take effect. The same-run picture measurements are unchanged and need no new threshold: frame vs own screenshot 0.024 (panel) and 0.035 (4K) against the limit 0.15; frame vs the other display's screenshot 0.667 / 0.725; the two displays' screenshots differ by 0.690. The NFR-2 and REC-3 rows above come from `rec.external_4k` only because of this size message.
- `ai.speakers` SKIP: the suite in run 42159767107 skips without `--speaker-models`. Commit 3f917350 makes it use Resources\Models\speaker and check the diagnostics line (needs a new tests build). Done by hand instead on the same install, with `CM_SPEAKER_MODELS_DIR` unset (user, machine and process): `capturemann diagnostics` says `speaker models at ~\AppData\Local\Programs\CaptureMann\Resources\Models\speaker (bundled with this build)`; Resources\Models\speaker holds pyannote-segmentation-3-0.onnx, campplus-sv-zh-en-16k-advanced.onnx, BUILD-INFO.txt and LICENSES\ (two licence files); `ai speakers --num 2` on speech2.wav (a private library) printed `Found 2 speakers ... sherpa-onnx-2.0.1+pyannote-segmentation-3.0+campplus-zh-en-advanced`, exit 0.
