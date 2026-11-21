# QA 0.3.6, Windows 11, the final CI files of commit `96b85253`, installed (PARTIAL)

- **Date:** 2026-11-21, about 20:38 to 21:35 local (UTC+7). **Who:** Claude (QA agent), driven from the Mac.
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one internal display 1920x1080 at 150 %, user `trung` logged in on the console, on AC (battery 100 %, High performance plan, sleep never). Laptop address 192.168.1.215.
- **Files under test:** the GitHub "Windows package" run 44898477660 for `96b85253` (unsigned, with the speaker engine), downloaded earlier to the Mac and copied unchanged.
  - `capturemann-0.3.6-windows-x64-setup.exe` SHA-256 `d476e741f441cdf4d023e146e23f9517283a32981ee34e8148913108cacb100e`
  - `capturemann-0.3.6-windows-x64.zip` SHA-256 `9295a059f9cd572d080fbc9426cf13fa9645582d8c63c5cd200bd107053b66dc`
  - Both sums matched on the Mac and again on the laptop after the copy (`Get-FileHash`). The zip was only copied and checked, not unpacked (the setup was the installed form).
  - Installed programs (first 12 hex of SHA-256, full values are in the regression reports): `capturemann.exe` f3e4631a11b4, `cm-daemon.exe` 10ab8564fa13, `cm-desktop.exe` c84f5bcde9ed, `cm-mcp.exe` 498b9bce7c0b, `cm-overlay.exe` e7dcc179c6a6, `ffmpeg.exe` 75933d80ff59, `ffprobe.exe` 37daffbdc199, `llama-server.exe` 323afb63b587.
- **What was installed before:** nothing. No CaptureMann uninstall entry (`HKCU\...\Uninstall`), no `Get-Package` hit, no `%LOCALAPPDATA%\Programs\CaptureMann` folder, no CaptureMann process. So the silent install went to the default per-user folder as the 0.3.5 run did (no maintainer install to protect, so no portable zip).
- **Install:** `setup.exe /S`: exit 0, `capturemann --version` = `capturemann 0.3.6`, uninstall entry "CaptureMann 0.3.6" with the InstallLocation, `Resources\Models\speaker` holds the two `.onnx` files.
- **Isolation:** every app, CLI and `cm-regress` run used private folders. The Quick Capture checks: `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR` under `C:\lab\q36g\env`, pipe `\\.\pipe\q36g`, `CAPTUREMANN_SECRETS=file`, the app started from a scheduled task in the console session (`win-run-in-session.ps1`). The regression has its own folder per case. **Before and after listing** of the laptop user's default library (`C:\Users\trung\Videos\CaptureMann`) and the app data folders (`%APPDATA%\CaptureMann` with `settings.json`, the other possible folders were absent): 133 lines (path, size, last write) and the SHA-256 of `settings.json` (`C4B6E1D7...6241`); **the two listings are identical (0 differing lines)**, taken before the install and after the last tier (before the uninstall).
- **Method (bar checks):** keys and clicks were injected with `keybd_event` and `SendInput` (absolute moves, down, 12 steps, up for a drag) and read back with UI Automation (names with state, rectangles, keyboard focus), the window list (rectangle, z-order, foreground window). The bar is excluded from screen capture, so there are no pictures; the evidence is the rectangles and names in this table.
- **`cm-regress`:** built on the laptop from `git archive 96b85253` (`cargo build --release --locked -p cm-regress`, 47 s, cache `C:\wt\target`), put at `C:\lab\tests\cm-regress.exe` (the Nov 18 one is kept beside it as `cm-regress.exe.pre-q36g`), and run through `scripts/testlab/regress.sh windows --installed --tier <t> --allow screen --commit 96b85253 --whisper-model C:\lab\r6\ggml-small.bin --speech C:\lab\r6\speech2.wav --speaker-models <installed>\Resources\Models\speaker --installer <the setup>` (`WANT_DISPLAYS=1`). No camera, no microphone, no sound.

## Quick Capture bar on the installed files

| Item | Result | Evidence |
| --- | --- | --- |
| Shortcut Ctrl+Alt+Shift+C opens the bar | OK | In every run (8 opens): the bar window appeared within 2.5 s, 1323 x 161 px at 150 %, near the pointer; pressing the chord again closed it. |
| (a) Drag by the background moves the bar (AC20) | OK | `SendInput` press on the gap between the Long Shot and Editor buttons, drag -220,-160: the window went 239,495 to 19,335 (delta exactly -220,-160); a second drag from the gap between the kind and target groups, +200,+150: delta exactly +200,+150; the bar stayed open and stayed the foreground window; a key pressed right after the drag (2) switched the kind. In two other runs the same drag was clamped at the display's top and left edges (delta -220,-53), which is the "fully on the display" rule, not a defect. |
| (a) A drag starting on a button does not move the bar | OK | Press on the Display button, drag +120,+90: window rectangle unchanged (delta 0,0) in 2 of 2 runs; the target stayed Region (the press released elsewhere did not click). |
| (a) Clicks still work | OK | Click on Video changed the kind to Video, click on Screenshot back; the window did not move (delta 0,0). |
| (b) Ctrl+1 picks the first preset | OK | With the four starter presets in the private config: Ctrl+1 changed the Preset button to "Preset - Bug report - key P" and the action to "Capture - Bug report - key Enter"; Ctrl+2 gave "Docs shot". Repeated in 2 runs. |
| (b) Ctrl+1 does not raise the library window | OK | Library (Studio) window and bar z-order before and after Ctrl+1: library z 15 and bar z 9 both times; the foreground window stayed the bar (same handle); the Studio page did not change. |
| (c) "Save current as preset..." then Esc returns to the picker | OK | P opened the picker (window 1323 x 643 at 239,18: it opens above the bar because there is no room below on this display; rows Docs shot, Demo video, Meeting, Save current as preset..., Edit presets...). Clicking "Save current as preset..." showed the name screen (controls "Preset name", Cancel, Save preset; window 1323 x 431). Esc: the picker rows were back (the same 643 px window), the bar still open. |
| (c) After that Esc: Esc, Down and Enter, keys 1/2/3 | OK, with a note | In the picker, Down then Enter chose the next preset (Bug report) and closed the picker, window back to 161 px. After the picker closed, keys 2, 3, 1 gave Video, Audio, Screenshot. While the picker is open, plain 1/2/3 and Tab change nothing; the same is true in a picker opened with P without the name screen, so it is not a regression of the Esc path. If the brief meant that plain 1/2/3 must work with the picker open, that is NOT the case (see finding F-2). First Esc closed the picker only, second Esc closed the bar (as specified). |
| (c) Tab | NOT VERIFIED | UI Automation keeps reporting the focus on the window "Quick Capture" before and after Tab (the focus ring is drawn by the app, not exposed). Not a pass or a failure. The 0.3.6 pass of Nov 20 read the ring from the tooltips; not repeated. |
| (c) After a save the window shrinks back (no dead zone) | OK | P, Save current as preset..., typed `qgx`, Enter: 0.5 s later the window was 1323 x 161 (rest height 161) and stayed so; the Preset button read "Preset - qgx - key P"; `presets.json` of the private config holds `qgx`. The first Esc after the save closed the bar (nothing else was open). (On `8a746f6b` the window kept 431 px, F-2 of the last pass.) |
| O collapses and is remembered | OK | O: width 1323 to 717, same left edge (239), toggle name "Expand - key O"; `quick_capture.json` has `"collapsed": true`; Esc, chord again: the bar opened at 717 px; O again: 1323. `settings.json` of the private config was not written by the bar. |

### Findings from the bar work

| ID | Sev | Finding | Seen |
| --- | --- | --- | --- |
| F-1 | medium, intermittent | **The bar opened while the desktop (Program Manager) is the foreground window does not take the keyboard.** After Esc closes the bar the foreground goes to the desktop, not to the Studio window; the next chord opens the bar without activating it. Then keys (1/2/3, P) do not reach it until a button on the bar is clicked (a click on a button activates it; a click on the drag background did not always: in 3 of about 10 tries the foreground was `Program Manager` after a background click, and in the first diagnostic the keys then did nothing). Opened from the Studio window (the normal state in the runs above) the bar is always the foreground window and the keys work at once. I did not isolate this further (Windows' foreground lock for a window opened from a global hotkey while another process is in front is the likely cause); it may also be what a user sees when the bar is opened while a browser has the focus. Worth one test on the final build with a browser in front. | 1 diag run, 3 of 10 background clicks |
| F-2 | note | While the picker is open, plain 1/2/3 and Tab do nothing (Esc, Up/Down, Enter, Ctrl+1..4 work). The spec text (QC-1 Keys) does not say they should; it only says Ctrl+1 to Ctrl+4 work "with the picker open or not". Same in the plain picker and after the name screen. | 2 of 2 |
| F-3 | polish | With the bar near the bottom edge the picker opens above it and the window moves to the top of the display (1323 x 643 at y 18); the bar itself keeps its place, but the window rectangle jumps (UI Automation names stay stable). Not a defect for a user; noted because the rectangle moved 477 px. | 3 of 3 |

## Package and install checks

| Item | Result |
| --- | --- |
| SHA-256 of the setup and the zip on the laptop | match |
| Silent install, version, uninstall entry, programs, speaker models | ok (see above) |
| `installer.silent` in the full and hardware tiers (replaces and restores the uninstall entry) | PASS in both |
| Silent uninstall at the end (`uninstall.exe /S _?=<dir>`) | exit 0; install folder, uninstall entry, Start menu folder and the user PATH entry are gone (checked) |

## Regression on the installed files (reports `L1-windows-0.3.6-ci-96b85253-installed-regression-*`)

| Tier | Passed | Failed | Skipped | Total | Time |
| --- | --- | --- | --- | --- | --- |
| standard (`--allow screen`) | 57 | 0 | 7 | 64 | 14:03 to about 14:06 UTC |
| full (`--allow screen`) | 68 | 0 | 12 | 80 | 14:06 to about 14:15 UTC |
| hardware (`--allow screen` only) | 68 | 0 | 20 | 88 | 14:16 to 14:25 UTC |

**Failures: none.** `ai.transcribe` (ggml-small, speech2.wav, 600 s limit) passed in the full and hardware tiers on the first run, so the rerun the brief asked for in case of a time-out was not needed. `ai.speakers` passed (the speaker models of the installed package), `desktop.hotkey_overlay` passed in all three tiers (no leftover app held the shortcuts: I stopped every app, daemon and overlay I started before the first tier and again before each of them).

**Skipped (all with a stated reason in the reports):** `linux.*` (8 cases, not implemented for Windows), `installer.dmg`, `desktop.recording_banner` (not implemented for Windows); `desktop.recording_placement`, `shot.second_monitor`, `rec.external_4k`, `rec.each_monitor` (need a second monitor); `rec.mic`, `rec.soak` (need `--allow mic`, not given); `rec.system_audio` (needs `--allow sound`, not given); `rec.camera` (needs `--allow camera`, not given); `guide.real_clicks` (headless daemon has no confirmation UI; covered by hand in the 0.3.5 pass). The count differences between tiers are these gated cases.

## Incidents and lessons

1. The app is started over ssh only through a scheduled task in the console session (`win-run-in-session.ps1`); a process started with `Start-Process` from the ssh session dies when the ssh command ends. My first long runs were cut by a local `cut` in the pipe (encoding), which left the remote script running; I killed those leftovers (the app and three `powershell` helpers of my own) and unregistered the `cm-lab-run-*` tasks.
2. Two of my first bar runs were wrong because of the focus effect in F-1 (the bar not being the foreground window after my own close and reopen); I re-ran with the Studio window clicked before each open. The runs above are the corrected ones.
3. `cm-regress.exe` in `C:\lab\tests` was from Nov 18; I replaced it with a build of `96b85253` (the old one is `cm-regress.exe.pre-q36g`).
4. `C:\lab\lab.lock` was taken by me for the setup and removed before `regress.sh` took its own; none is left.

## Not run, or limits

- Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA/AMD encoders, the camera, the microphone, system sound (quiet hours), the 5-minute soak, a bar opened with a browser in front (F-1), the Tab focus ring by eye, the bar pictures (the bar is excluded from capture; no `CM_DEMO_CAPTURABLE` instance was started this time).
- The zip was checksummed only, not installed or tested.
- The package is unsigned; the NSIS setup was run silently only.

## Cleanup

Uninstalled the lab install (silent). Removed `C:\lab\q36g` (private library, config, logs, listings), `C:\wt\q36g` (the source), my `q36g-*.ps1` scripts, `q36g-src.tgz`, the setup and zip copies in `C:\lab`, the regression output folders of this run, my scheduled tasks and `C:\lab\lab.lock`. Stopped every process I started. Kept: `C:\wt\target` (build cache), `C:\lab\tests\cm-regress.exe` (96b85253) and `cm-regress.exe.pre-q36g`. The laptop was not rebooted and was not switched; it is on Windows at the desktop.

## Verdict

PARTIAL. The three Quick Capture fixes work on the installed final files (drag by the background, Ctrl+1 without raising the library, Save current as preset with Esc back to the picker and the window shrinking after the save); the regression has no failure (57/0, 68/0, 68/0). Open: F-1 (keyboard focus when the bar is opened while another process has the foreground; intermittent, not isolated) and the not-run list.
