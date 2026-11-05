# L1 Windows 0.2.8, round 3: picker, HUD clicks, placement, delay pill, overlay deadline, parity, import colour

- **Date:** 2026-11-05 about 13:35 to 14:40 (UTC+7), the maintainer being away for the desktop part.
- **Who:** Claude (AI agent), from a Mac over ssh and the logged-in session (`C:\lab\win-run-in-session.ps1`). Windows were read with
  `EnumWindows` / `GetWindowRect` / `GetWindowLongPtr` / `WindowFromPoint` / `WM_NCHITTEST` and logged as text; input was `SetCursorPos`,
  `mouse_event` and `keybd_event`. No screenshot was taken or stored. No product code was changed.
- **Machine:** ThinkPad T490, Intel UHD 620 (driver 31.0.101.2137), Windows 11 Pro 25H2 build 26200.9711. Panel 1920x1080 at 150 %, primary, origin
  (0,0); external Samsung `S27B80P` 3840x2160 at 150 %, origin (1920,0). Display settings were not touched.
- **Build under test:** package run 41972558656, commit `7d108896` (installed per user; `capturemann diagnostics` still prints "CaptureMann 0.2.7"). It does
  NOT contain `631b2874` (click-through area window, explicit stacking), which is on `public/track/windows` afterwards.
- **Instance:** the maintainer's `cm-desktop` was stopped and a second instance ran on its own data (`CAPTUREMANN_CONFIG_DIR=C:\lab\r3\cfg`,
  `CAPTUREMANN_LIBRARY=C:\lab\r3\lib`, `CAPTUREMANN_SOCKET=\\.\pipe\cm-r3`, `start_recording` set to "allow" so no on-screen confirmation was needed).
  Display ids in this build are 4327037 (panel) and 7931565 (external) in `capturemann targets` (not 65608 / 131073 of the round-2 report).
- **FFmpeg:** gyan.dev release essentials 9.0.2, zip SHA-256 `60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba` = the published `.sha256`;
  on `PATH` of the test processes only; deleted at the end.
- **Test programs:** `C:\lab\tests` on the laptop was the OLD build (`BUILD.txt`: commit `d2713822`, run 41962900083), not run 41971550916 as assumed.
  Item 6 was therefore run from the real artifact of run 41971550916 (zip SHA-256 `d48a0747...9b8f9454`, `BUILD.txt` commit `7d108896`), extracted to
  `C:\lab\r3\tn` and deleted afterwards.
- **Status:** PARTIAL. Observed by one agent on one laptop and one layout.

## Results

| # | Check | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Window picker, hotkey Ctrl+Alt+Shift+2, both monitors | PASS with findings | Hotkey with the pointer on the external: list picker 1522x1121 at (3079,484) (centred on the external), outline window beside it; pointer on the panel: 1522x1019 at (199,-5) on the panel. Chosen two ways, each on both monitors: (a) type "notepad" + Enter, (b) Tab ("Pick on screen") then a click on the Notepad. Notepads were 800x500 frames placed at (207,200) on the panel and (2207,200) on the external; `targets` listed each as 786x493; saved `.cmshot` sizes 786x493 (all four captures); overlay.log "overlay ends: the user chose a target", "capture made". Esc in the overlay: overlay.log "cancelled by the user", exit 3 (`user_declined`), desktop.log "capture ended: cancelled", 0 `cm-overlay` processes afterwards. Findings F5, F6 below |
| 2 | HUD cannot be clicked during a recording | FAIL (reproduced; cause shown) | Recording of the external (display 7931565, frame (1920,0) 3840x2160), z-order top first: toast 682x218 at (1225,796) z=12; **area window** hwnd 0xA056C rect (1909,0) 3862x2171, ex-style 0x200188 = TOPMOST\|TOOLWINDOW (no TRANSPARENT, no LAYERED), z=13; **HUD** hwnd 0x3702B6 rect (3286,1888) 1108x152, same flags, z=14 (below the area window). `WindowFromPoint` on 18 points over the HUD rect and on its centre (3840,1996) and Pause estimate (3616,1996) = 0xA056C, `WM_NCHITTEST` = 1 (HTCLIENT). Points inside the recorded area away from the HUD ((2500,500), (4000,1000), (5000,300)) = 0xA056C as well: clicks do not reach the windows behind. `mouse_event` click on Pause (3616,1996): `record status` before 00:50 recording, after 00:52 "recording (paused 00:00, 0 pauses)". Afterwards the clicked area window was raised to z=12 above the toast. Panel (display 4327037, frame (0,0) 1920x1080): toast 0x110048 (1225,796) 682x218 z=12, area 0x430632 (-11,0) 1942x1091 z=13, HUD 0x100646 (406,808) 1108x152 z=14; HUD grid points = area window except the right third (x 1225..1514) = the toast; centre (960,916) and Pause estimate (736,916) = area window; click: 00:55, not paused. Bundles removed |
| 3 | Placement on the recorded monitor | FAIL (countdown), PASS (area, HUD) | External recorded: **countdown disc** 286x320 at (817,403) = centre (960,563) = the **panel's** centre (polled every 100 ms from 13:36:08.417 to 13:36:11.6, did not move). Area (1909,0) 3862x2171: on the external; the extra 22x11 px is the window frame (style 0x14C00000 has WS_CAPTION), client = display. HUD centre x = 3286+554 = 3840 = display centre; bottom 2040 inside the display; the work area ends at 2088 (taskbar at y 2088), so the margin is 48 px. Panel recorded: **countdown** 286x320 at (3697,943) = centre (3840,1103) = the **external's** centre; area (-11,0) 1942x1091 on the panel; HUD (406,808) 1108x152: centre x 960, bottom 960, work area ends 1008. So in both runs the countdown opened on the other monitor. The pointer was not logged: in run 2 it had been left on the external by the click of item 2, in run 1 probably on the panel, which fits "countdown opens where the pointer is", not tested |
| 4 | Delay pill and display ids | PASS | `capture.delay_s` 3 (settings file; see F7 for the CLI). Hotkey Ctrl+Alt+Shift+3 with the pointer on the external at (4000,1000): pill window 106x95 at (5641,19) (top right of the external), saved `shot_06GTMW23NEG4R5AG` **3840x2160**, pill closed 5.3 s after opening; pointer on the panel at (900,500): pill (1801,19), saved `shot_06GTMW2ZKNVHPQP5` **1920x1080**, 3.3 s. The `display_id` is not in the info log or in the manifest, so the choice is shown by the image size only |
| 5 | Overlay deadline and parent-pid watch | PASS | `cm-overlay.exe region --socket \\.\pipe\cm-r3` with `CM_OVERLAY_DEADLINE_S=10`: exited after 10,116 ms; stderr "overlay started: ... deadline 10 s" and "[ERROR cm_overlay] overlay: the overlay was still running after 10 s and ends itself"; JSON error exit_code 1. With `CAPTUREMANN_PARENT_PID` = a `cmd /c ping -n 6` (about 5 s): overlay ended 5,121 ms after start, "the app that started the overlay (process 13144) is gone; the overlay ends itself". 0 `cm-overlay` left. These standalone runs write to stderr only (nothing was appended to overlay.log) |
| 6 | Parity and `cm-media` with the full FFmpeg | PASS (new programs); FAIL with the stale ones | Programs of run 41971550916 (commit `7d108896`), full FFmpeg 9.0.2 on `PATH`: `cm-desktop__parity` prints "GPU adapter: Intel(R) UHD Graphics 620", 3 passed, 0 failed (294.6 s wall with the other suite running); control (no edit) frames 31/159/286: shift 0, MAE 0.00 / 0.64 / 0.57 (limit 3); the other cases were not copied from the output. `cm-media`: quick_export 43 passed (3 ignored, 216.6 s), vp9_export 1, edit_view_export 5, import 8, decode 4 (1 ignored), caption_export 4, audio_export 5, lib 147; 0 failed. The old programs that were in `C:\lab\tests` (commit `d2713822`, no NV12 fix) with the same FFmpeg: parity 1 passed, 2 FAILED, control MAE 7.24 / 7.26, trim 7.23, bubble 7.25 to 7.47, everything 5.96 to 6.15 (limit 3), PSNR about 25 dB, shift 0 (one -1): the same numbers as round 2, which confirms `2d7145c0` is what fixed it, and that `C:\lab\tests` must be refreshed |
| 7 | Import colour (`prepare.rs`, NV12 to `h264_mf`) | PASS for a BT.709-tagged HD source | Source: smptebars 1280x720, H.264 yuv420p, tagged bt709/bt709/bt709 tv, VFR (so the import prepares an editing copy; `--dry-run`: "prepared for editing (variable_frame_rate, container_mkv)"). `import --wait`: copy made, `screen.edit.mp4` H.264 Constrained Baseline 1280x720 yuv420p, tags bt709/bt709/bt709 tv kept, 120 frames at 30. Frame 10 of each decoded to PNG by ffmpeg, 12 sample points (7 bars, 5 lower-strip patches): per-point RGB identical, max difference 0 (for example yellow 195,179,0; red 207,17,0; blue 0,11,199). Decoding the copy with BT.601 forced gives visibly different values (yellow 192,190,0), so the check can see a matrix error. **Not tested:** untagged, BT.601, full-range or 10-bit sources, HEVC sources, other GPUs |
| 8 | Windows Terminal name | PASS | Windows Terminal 1.24.12741.0 installed. `capturemann targets` lists "Windows Terminal - Windows PowerShell 1734x927 at (188,190)" for a real window I opened (class `CASCADIA_HOSTING_WINDOW_CLASS`) and closed with WM_CLOSE. The terminal that hosts the lab's session scripts is listed too, as "Windows Terminal - C:\WINDOWS\...\powershell.exe 219x39 at (-31991,-32000) (minimized)" (id changes on every call) |
| 9 | Install sanity | PASS | `capturemann diagnostics` lines below; `llama-server.exe --version`: `version: 0.7.0-dev (build 0, commit unknown)`, `built with GNU 16.2.0 for Windows AMD64` |

## Findings with evidence (for the people who fix)

- **F1 HUD cannot be clicked (item 2).** On `7d108896` the full-monitor area window (`recording indicator`) is created after the HUD, sits above it in the TOPMOST band
  (z 13 against 14) and is not click-through on Windows (`Tweaks::click_through` is not applied, `applied_after_exclusion` returns false; ex-style has no
  WS_EX_TRANSPARENT/LAYERED). It answers `WindowFromPoint` and `WM_NCHITTEST` (HTCLIENT) everywhere on the recorded monitor, over the HUD and over every other
  window. `631b2874` addresses both (click-through plus `raise_order`); it needs this same probe on a package that contains it: area window must have
  TRANSPARENT|LAYERED|NOACTIVATE, `WindowFromPoint` at the Pause button must return the HUD hwnd, the click must pause.
- **F2 Toast placement.** Toast windows ("toast window opened: 440 x 154 at (824, 518), display known") are always at the lower right of the **panel**
  ((1225,796) 682x218), also while the external is being recorded, and they are TOPMOST above the area window and the HUD. With the panel recorded the
  toast covers x 1225..1514 of the HUD (the right-hand buttons) until it closes. The 440x154 toast is re-created every 0.5 to 2 s while it shows
  (54 "toast window opened" for 6 "toast queued" in 11 minutes in `desktop.log`), and the log has `gpui window.rs:2017 window not found`,
  `platform.rs:493 Invalid window handle (0x80070578)` and, when the HUD opens, `window.rs:1968 RefCell already borrowed` (06:36:11.686 and 06:37:51.340).
- **F3 Countdown on the wrong monitor (item 3).** See the item. The code of `6ba2215c` is in this package for the HUD and the area window (both are right) but the
  countdown (`countdown window ... opened`, no rect logged) opens on the other monitor in both runs. Probe on the next package: put the pointer on the
  recorded monitor, on the other monitor, and check the disc centre against the recorded display's centre each time.
- **F4 4K recording of the external on this laptop.** `rec_06GTMV0GS8HVYM44` (73.5 s wall, 3840x2160, 60 Mbps): `screen.mp4` holds 981 frames = 32.7 s
  (`file_end` 3,960,000 ticks = 33.0 s against `end` 8,823,282 = 73.5 s), 17 fragments; counters `delivered 2364, written 981, dropped 8931, late 9027,
  late_pct 920` (dropped and late are more than delivered); `record status` said "late frames: 617.4 %" at 1 s and "85.6 % of screen frames were late over
  the last 30 s". `record start` answered `output.codec: "hevc"` while the manifest has `codec: "h264"`. The same panel recording at 1080p30: 0.1 % late. The
  file silently covers less than half of the recording; the only signal was a warning in the status. The UHD 620 cannot keep up with 4K30 and the stats are not
  consistent (a rate above 100 %).
- **F5 Keyboard selection in the window picker (item 1).** After the hotkey the picker is the foreground window; six Down presses (0.7 s apart) never changed the
  outline window (same hwnd and rect each time), while typing a filter did (outline moved to the Notepad: (2187,141) 826x567 around the 786x493 window). The outline
  stays on the first entry (Windows Explorer at the start) whatever Down does: either Down is not delivered to the list or the outline is not updated on Windows.
  Also the outline is 22x11 larger than the ring computation (frame, as for the area window), harmless.
- **F6 Tab held for 150 ms bounces back.** In the list, Tab ("Pick on screen") with `keybd_event` down and up 150 ms apart started `cm-overlay` (mode Window)
  which ended 47 ms after its start, "overlay event loop ended (ok)", "overlay finished after 179 ms / 302 ms: ok" (twice, panel and external), and the list
  reopened. With 25 ms between down and up the overlay stayed and the click captured. A human key press is closer to 100 ms: the overlay probably sees the key release
  (or a repeat) of the Tab that started it. Not seen by eye.
- **F7 Settings written by the CLI are not applied by the running app.** `capturemann settings set capture.delay_s 3` is stored (the file and `settings get` show
  it), but the app kept 0: the hotkey opened the display overlay without a pill; later `settings set capture.delay_s 0` against an app that had loaded 3 still made a
  3 s pill. After a restart the stored value is used. (`capture.delay_s` is agent-writable.)
- **F9 `cm-desktop` crashes at start on the maintainer's real data (found while restoring the app; not one of the nine items).** Windows Application log, event 1000:
  `cm-desktop.exe` exception **0xc000001d (illegal instruction)**, fault offset `0x0000000003d4d5c5`, in `cm-desktop.exe` itself (timestamp 0x6aec123b), at 13:58:03 (the
  app restarted by me at 13:55:32, i.e. about 2.5 min after the start, with no log line between) and at 14:36:21 (2 s after the start). Reproduced 5 of 5 times with
  everything at its default (`%APPDATA%\CaptureMann` config with `ggml-medium.bin`, `~\Videos\CaptureMann` library with 9 captures, default pipe, keyring secrets): process
  exit 0xC000001D after 5.7 to 8.2 s; the last `desktop.log` line is always "activating (studio opened): App". Not reproduced with: an empty library and the default config
  (alive 20 s); each of the 9 captures alone, the whole library with and without its `.capturemann` folder, fresh config (all alive 12 to 15 s); and, in the last round, a
  copy of the library with the real config dir, with `CAPTUREMANN_SECRETS=file` and with keyring (alive 15 s each). One earlier copy-based trial (real config dir + copy of the
  library WITH `.capturemann`) did crash, so it is data-dependent but not understood. The CPU is an i7-8665U (AVX2, no AVX-512); the offset has not been symbolised.
  Candidates to check: a CPU-feature-dependent routine in the in-process code that runs after the library window opens (poster/thumbnail decode of
  `.capturemann\posters\rec_06GTMT0FVC78B6QS.jpg`, the auto-export job `job_9c62b78f8dcc0ee5` of that recording, preset `quick`, `"auto":true`, which had reached 100 % and has an
  exported `quick.mp4`), or the library index. **The maintainer's app could therefore not be left running**; his data was not modified by me (the trials only read it, except
  the files the app itself writes in `.capturemann` and `logs` on each start).
- **F8 Version string.** The package prints "CaptureMann 0.2.7" in `diagnostics` and the logs.

## Not run

Windows 10, mixed-DPI layouts, a monitor left of / above the panel, the 150 s deadline through the app's own watchdog, the countdown with the pointer placed on purpose,
the delay pill with the tray menu, hovering the picker cards by mouse (the card positions are unknown to the script), the cursor sprite, re-test of F1 with `631b2874`,
sound (no sound was played; the volume was never changed).

## `capturemann diagnostics` (instance with its own library; paths of the lab)

```
## App
- CaptureMann 0.2.7
- started from a plain binary
## System
- windows x86_64
- threads available 8
- Windows 11 Pro 25H2, build 26200.9711
- GPU Intel(R) UHD Graphics 620, driver 31.0.101.2137, 134 MB dedicated memory, vendor id 0x8086
- GPU Microsoft Basic Render Driver (software renderer: no hardware GPU is in use), driver 10.0.26100.9711, vendor id 0x1414
- secrets in a file (owner-only, not encrypted)
## Permissions
- microphone Authorized / screen recording Authorized / camera Authorized
## Models and engines
- no speech model installed
- text model: provider "none", model "", engine stopped
- ffmpeg at ~\AppData\Local\Programs\CaptureMann\ffmpeg.exe
- llama-server at ~\AppData\Local\Programs\CaptureMann\llama-server.exe (bundled with this build, llama.cpp v0.7.0)
- text recognizer: windows_ocr (available; reads en-US ...; OCR languages installed: en-US)
- speaker engine built in: true
## Library
- 8 captures, 228 MB / 0 unreadable / free space on the library volume 176 GB
## Last recording
- rec_06GTMVCMY10WHWDQ recorded 2026-11-05: codec "h264", size [1920,1080], fps 30, bitrate_bps 16000000, quality "high", late_pct 0.1048767697954903, stalls 0
## Warnings and errors: gpui window.rs:2038/2017/1720/1968 "window not found" / "RefCell already borrowed", platform.rs:493 "Invalid window handle. (0x80070578)" (about 30 lines in 11 minutes)
```

## Cleanup

Removed: my recordings, screenshots, libraries and configs (`C:\lab\r3\lib`, `cfg`, `imp-lib`, `imp-cfg`), the Notepad windows I opened, my Windows Terminal window,
`C:\lab\ffmpeg-full`, the extracted test programs `C:\lab\r3\tn`. The maintainer's `cm-desktop` was stopped at about 13:35 and could NOT be left running: it crashes at start on his real data (F9); it is closed now, nothing of mine is running. No scheduled task is left. Volume: never changed (still muted at 0 as left by the maintainer). Note: standalone `cm-overlay` runs were started without
`CAPTUREMANN_CONFIG_DIR`, but they wrote nothing to the maintainer's `overlay.log`.
