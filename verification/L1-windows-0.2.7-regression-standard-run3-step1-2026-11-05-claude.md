# L1 windows 0.2.7 regression report (standard)

- **Date:** 2026-11-05T10:35:38Z to 2026-11-05T10:37:12Z
- **Who:** cm-regress via regress.sh
- **Machine:** Microsoft Windows 11 Pro 10.0.26200 build 26200; GPU: Intel(R) UHD Graphics 620 (driver 31.0.101.2137)
- **Build:** commit unknown, package folder `C:\Users\trung\AppData\Local\Programs\CaptureMann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier standard --out C:\lab\regress-out\20261105T103532Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen,second_monitor --only rec.external_4k,desktop.settings_applied,shot.second_monitor,desktop.hud_click_through`, tier `standard`
- **Result:** 1 passed, 3 failed, 0 skipped of 4 cases
- **SHA-256 of the files under test:**
  - `capturemann.exe` `54de4cef1252c28df3f049a30a5c0a9626b7368dc39fd8f273363cc4588ab4a8`
  - `cm-mcp.exe` `22f296cffe660a2ef8f39cb1b256bd603cf6f91a1a96265e8fd3a0dc73f88bd3`
  - `cm-daemon.exe` `c869a197f3994b75e80f9cf3a4464d4f80e9cb897f6a6a8fdf8eab958ebb5b1b`
  - `cm-desktop.exe` `288606b6e3e0aebeef1842de7efafff19b4f1d05e2250b2f31a5cd22874ef767`
  - `cm-overlay.exe` `01983fc20f23ed6a6a1d5f2c0312858513e3a7b3ae8064baef613b5c652c064d`
  - `ffmpeg.exe` `2c1a80e031805cd110bc5cad0c4847dcbccc277ba924a47ff0f1503d92066f91`
  - `ffprobe.exe` `5fec37a87e26a8c3b3da8528045471612003010e22e12f3d25ead8baf0b594fa`
  - `llama-server.exe` `be54819796793a564b088083e8d46febe2b7d3507745d12d886a52465080b115`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `shot.second_monitor` | PASS | every extra monitor lists and captures at its own physical size |
| `rec.external_4k` | FAIL | 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps: the rate fell to NaN fps with no encoder_overloaded warning |
| `desktop.hud_click_through` | FAIL | during a recording the HUD answers clicks and the area window is click-through: 4 ERROR or panic line(s) in the logs, first: desktop.log: 2026-11-05T10:37:05.7738039Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578) |
| `desktop.settings_applied` | FAIL | settings set capture.delay_s reaches the running app: a forwarded display capture waits that long: `capturemann settings set capture.delay_s 4` failed (exit 2 (0x00000002)): {"error":{"code":"invalid_input","data":null,"exit_code":2,"message":"capture.delay_s:... |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| AGT-1 | FAIL | `desktop.settings_applied` |
| CAP-1.AC1 | PASS | `shot.second_monitor` |
| DPI-WIN layout A | PASS | `shot.second_monitor` |
| NFR-2 | FAIL | `rec.external_4k` |
| REC-3 | FAIL | `rec.external_4k` |
| REC-5 | FAIL | `desktop.hud_click_through` |
| bug: 4K recording of the external lost frames on the UHD 620 | FAIL | `rec.external_4k` |
| bug: area window swallowed clicks meant for the HUD | FAIL | `desktop.hud_click_through` |
| bug: settings set not applied to the running app | FAIL | `desktop.settings_applied` |

## Problems found

### `rec.external_4k`: 30 s of a 4K external monitor: a full-length file (with an encoder_overloaded warning) or full 30 fps

- the rate fell to NaN fps with no encoder_overloaded warning

Evidence:

```text
daemon pid 1280 answered after 321 ms
started rec_01a1ab4768d4_0001
export job job_f37732cf135683a0: done
display 7997105 3840x2160: track 48000..3651975 ticks, file_end NaN, effective_fps NaN, late_pct None, dropped Some(Number(0)), rate_changes None
file 30.43 s
encoder_overloaded warning logged: false
```

### `desktop.hud_click_through`: during a recording the HUD answers clicks and the area window is click-through

- 4 ERROR or panic line(s) in the logs, first: desktop.log: 2026-11-05T10:37:05.7738039Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)

Evidence:

```text
hwnd 0x78069c pid 3464 class 'Zed::Window' title '' rect [406, 808, 1514, 960] ex 0x200188
hwnd 0xc01c4 pid 3464 class 'Zed::Window' title '' rect [-11, 0, 1931, 1091] ex 0x82801a8
hwnd 0x4b0126 pid 3464 class 'Zed::Window' title '' rect [1225, 589, 1907, 807] ex 0x200188
hwnd 0x19048e pid 3464 class 'Zed::Window' title 'CaptureMann' rect [64, 8, 1856, 1000] ex 0x240100
WindowFromPoint(1566,698) is the HUD
export job job_d7917bfbeb39360c: done
log hygiene: 12 WARN line(s) in the case's logs
log: desktop.log: 2026-11-05T10:37:05.7738039Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)
log: desktop.log: 2026-11-05T10:37:05.7738756Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)
log: desktop.log: 2026-11-05T10:37:05.7805987Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)
log: desktop.log: 2026-11-05T10:37:05.7806597Z desktop ERROR gpui-pre-windows-0.3.9/src/platform.rs:493: Invalid window handle. (0x80070578)
```

### `desktop.settings_applied`: settings set capture.delay_s reaches the running app: a forwarded display capture waits that long

- `capturemann settings set capture.delay_s 4` failed (exit 2 (0x00000002)): {"error":{"code":"invalid_input","data":null,"exit_code":2,"message":"capture.delay_s: must be 0, 3, 5 or 10"}}

## Not run

- Every selected case ran.

## Evidence of passing cases

- `shot.second_monitor` (4426 ms): daemon pid 8752 answered after 320 ms; display 11601761 'Display 1' at (0,0) listed 1920x1080, saved 1920x1080; display 7997105 'S27B80P' at (-3840,-1861) listed 3840x2160, saved 3840x2160; log hygiene: 1 WARN line(s) in the case's logs

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
