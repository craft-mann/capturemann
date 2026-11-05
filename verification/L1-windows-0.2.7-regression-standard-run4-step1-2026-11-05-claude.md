# L1 windows 0.2.7 regression report (standard)

- **Date:** 2026-11-05T11:12:33Z to 2026-11-05T11:13:20Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out C:\lab\regress-out\20261105T111227Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,second_monitor --only rec.external_4k,desktop.settings_applied,desktop.recording_placement,rec.each_monitor,shot.second_monitor,desktop.hud_click_through,export.quick,export.presets`, tier `standard`
- **Result:** 4 passed, 0 failed, 4 skipped of 8 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `6de115d30da861aaece5b914eedad1ee44641fe6aaa1d3bb15e1a32d0100b943`
  - `cm-mcp.exe` `05b31fe08cdfaf21716c331d2102452e6ae660c2d4a4c4ec6436d9a17023d911`
  - `cm-daemon.exe` `5b53e1d9494b0c909c44def735591be81c359d536b74b090d4b77d311944bf0b`
  - `cm-desktop.exe` `1e8aff037be0bddc916c79242b9c6b2b7bfe443391a2164c0dc158e889595604`
  - `cm-overlay.exe` `f1ad57a6808e160b334c3bceb668cf0b0b4600046227c3f95bc67719c24ce978`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `shot.second_monitor` | SKIP | every extra monitor lists and captures at its own physical size: only one display is connected |
| `export.quick` | PASS | the quick export is H.264 MP4 of the right length, tagged BT.709 limited range |
| `export.presets` | PASS | youtube-1080p, vertical, square, webm and gif exports have the right codec, shape and length |
| `rec.external_4k` | SKIP | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: no non-primary display of 3000 px or more is connected |
| `rec.each_monitor` | SKIP | 10 s of each monitor: the cursor header names that monitor's origin and size; friendly names, sprite sizes: only one display is connected |
| `desktop.hud_click_through` | PASS | during a recording the HUD answers clicks and the area window is click-through |
| `desktop.settings_applied` | PASS | settings set capture.delay_s (0, 3, 5 or 10) reaches the running app: a forwarded display capture waits that long |
| `desktop.recording_placement` | SKIP | countdown, area window and HUD of a recording of the external monitor appear on that monitor: only one display is connected |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | PASS | `desktop.settings_applied` |
| CAP-1.AC1 | not run | `shot.second_monitor` |
| DPI-WIN | not run | `desktop.recording_placement` |
| DPI-WIN layout A | not run | `shot.second_monitor` |
| DPI-WIN steps 7 to 7c | not run | `rec.each_monitor` |
| EXP-1 | PASS | `export.quick` |
| EXP-2 | PASS | `export.presets` |
| EXP-3 | PASS | `export.presets` |
| NFR-2 | not run | `rec.external_4k` |
| REC-3 | not run | `rec.external_4k` |
| REC-5 | PASS | `desktop.hud_click_through`, `desktop.recording_placement` |
| REC-7 | not run | `rec.each_monitor` |
| bug: 4K recording of the external lost frames on the UHD 620 | not run | `rec.external_4k` |
| bug: area window swallowed clicks meant for the HUD | PASS | `desktop.hud_click_through` |
| bug: countdown and toasts on the other monitor | not run | `desktop.recording_placement` |
| bug: export colour tags (NV12 encoders converted with BT.601, stream untagged) | PASS | `export.quick` |
| bug: settings set not applied to the running app | PASS | `desktop.settings_applied` |

## Problems found

- None in the cases that ran.
## Not run

- no non-primary display of 3000 px or more is connected (1): rec.external_4k
- only one display is connected (3): shot.second_monitor, rec.each_monitor, desktop.recording_placement

## Evidence of passing cases

- `export.quick` (2308 ms): daemon pid 14952 answered after 321 ms; export --preset quick: clip-h264 - quick.mp4 by h264_mf, 291 ms; h264 320x240 yuv420p range tv space bt709 trc bt709 prim bt709, 3.00 s; encoder h264_mf: colour tags not asserted (only the Media Foundation path had the bug); log hygiene: 1 WARN line(s) in the case's logs
- `export.presets` (13052 ms): daemon pid 14832 answered after 321 ms; export --preset youtube-1080p: clip-h264 - youtube-1080p.mp4 by h264_mf, 1489 ms; youtube-1080p: h264 1920x1080 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (2266281 bytes); export --preset vertical-1080: clip-h264 - vertical-1080.mp4 by h264_mf, 1474 ms; vertical-1080: h264 1080x1920 3.00 s mov,mp4,m4a,3gp,3g2,mj2 fps Some(30.0) (1667874 bytes); export --preset square-1080: clip-h264 - square-1080.mp4 by h264_mf, 1368 ms; square-1080: h264 1080x1080 3.00...
- `desktop.hud_click_through` (9777 ms): hwnd 0xb03a0 pid 14556 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188; hwnd 0xf02fa pid 14556 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8; hwnd 0xf0240 pid 14556 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188; hwnd 0x100314 pid 14556 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100; WindowFromPoint(1566,698) is the HUD; export job job_c1cbecd58ca70aa6: done; log hygiene: 12 WARN line(s) in the case's l...
- `desktop.settings_applied` (9764 ms): forwarded capture request: exit 0 (0x00000000); the screenshot appeared after 5.4 s with capture.delay_s = 5; log hygiene: 12 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
