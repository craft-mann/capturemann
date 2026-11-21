# QA 0.3.6, Windows 11, the final CI files of commit `1778431b`, installed (PARTIAL)

- **Date:** 2026-11-21 evening to 2026-11-22 (UTC+7). **Who:** Claude (recheck agent), driven from the Mac. **Machine:** ThinkPad T490, Intel UHD 620, Windows 11, one display 1920x1080, user `trung` on the console.
- **Files under test:** GitHub "Windows package" run 44914326579 for `1778431b` (unsigned). `capturemann-0.3.6-windows-x64-setup.exe` SHA-256 `60a4c5bfd9f5bc913bc90c284218a32a5ebc919b7a023b0e391e562b71e92ddb`, `capturemann-0.3.6-windows-x64.zip` SHA-256 `e02dd69d94adaa6231a0db16f76e7a2fb78715cb889360724d82cbe3ac11a7c5`. Both matched on the Mac and on the laptop after the copy. The zip was only checksummed. Installed program sums are in the regression reports (`capturemann.exe` a445d5b6..., `cm-desktop.exe` 945a153b..., `cm-daemon.exe` 98cba5a4...).
- **Before:** nothing CaptureMann installed (no uninstall entry, no folder, no process, no lock). Silent install (`/S`) to the default per-user folder; `capturemann --version` = `capturemann 0.3.6`.
- **Isolation:** private `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, pipe `\\.\pipe\q36h`, `CAPTUREMANN_SECRETS=file` under `C:\lab\q36h\env`; app started from a scheduled task in the console session; keys and clicks by `keybd_event`, `SetCursorPos`, `mouse_event`; UI Automation for names and rectangles; `GetForegroundWindow` for the foreground. Before/after listing of the laptop user's default library and `%APPDATA%\CaptureMann` (176 lines, with the SHA-256 of `settings.json`): **0 differing lines**.

## F-1 recheck (bar opened with Ctrl+Alt+Shift+C, then 2, 3, 1, Esc, with no click on the bar)

Evidence per try: the bar window (cm-desktop process, about 1323x161) existed, `GetForegroundWindow` equalled the bar handle 1.5 s after the chord, `quick_capture.json` kind went screenshot > video > audio > screenshot, and Esc closed the bar.

| Situation | Foreground before the chord | Tries | Bar was foreground and keys worked |
| --- | --- | --- | --- |
| (a) desktop in front (all windows minimised, click on an empty desktop spot) | `Program Manager` (confirmed in every try) | 10 | 10 of 10 |
| (b) Edge window in front (`msedge about:blank`, own profile folder, clicked on its title bar) | the Edge window (title confirmed) | 10 | 10 of 10 |
| (c) Studio window in front (clicked) | `CaptureMann` | 3 | 3 of 3 |
| (d) Esc closes the bar, opened again at once (nothing clicked between) | a console window of the test helper (the foreground the harness had; not the desktop) | 5 | 5 of 5 |

F-1 did not reproduce in 28 of 28 tries. Notes: a first (a) run alternated desktop and the helper console because Win+D toggles; it was redone with MinimizeAll and every try started on Program Manager. A first (b) run did not click Edge (title match failed); redone, Edge confirmed in front. (d) did not start from the desktop; (a) covers that.

## Earlier bar results (one run, same installed files)

| Item | Result |
| --- | --- |
| Drag by the background (gap between Long Shot and Editor), -100,+90 | OK: window moved exactly -100,+90 (twice, in two runs), bar stayed foreground |
| Drag starting on the Display button, +120,+90 | OK: no move, target unchanged (Region) |
| Clicks Video and Screenshot | OK, kind changed, no move |
| P opens the picker, "Save current as preset..." opens the name screen (Preset name, Cancel, Save preset, 431 px), Esc returns to the picker rows (window stayed open) | OK |
| Saving a name (Enter) shrinks the window back to 161 px, Preset button shows the name | OK |
| Ctrl+1 picks the first preset ("Preset - Bug report", "Capture - Bug report") | OK |
| Ctrl+1 does not raise the library | OK: bar z 0, Studio z 14 before and after; bar still foreground |
| O collapses (1323 to 717 wide), `collapsed: true` in `quick_capture.json`, remembered on reopen; O again expands | OK |

## Regression (installed files, `--allow screen` only, `--commit 1778431b`)

| Tier | Passed | Failed | Skipped | Total |
| --- | --- | --- | --- | --- |
| standard | 57 | 0 | 7 | 64 |
| full | 68 | 0 | 12 | 80 |

Reports: `L1-windows-0.3.6-ci-1778431b-installed-regression-{standard,full}-2026-11-22-claude.{md,json}`. (The files carry the run's UTC date 2026-11-21 inside; they were renamed to the requested names.) `cm-regress.exe` on the laptop is the 96b85253 build (no cm-regress code changed). `installer.silent` passed in the full tier; `ai.transcribe`, `ai.speakers`, `desktop.hotkey_overlay` passed. **No failures.** Skips: 8 `linux.*` plus `linux.deb_install`/`linux.tarball_install` (not implemented for Windows), `installer.dmg`, `desktop.recording_banner`, `desktop.recording_placement` (second monitor), and in the standard tier the full-tier-only cases. Hardware tier not run (as briefed).

## Not run

Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA/AMD encoders, camera, microphone, system sound, the soak, hardware tier, the zip installed, the Tab focus ring, bar pictures (the bar is excluded from capture).

## Incidents

A pipe through `cut` broke on a non-ASCII UI Automation name and one remote run was interrupted; I waited for it to end, reset the bar state and reran cleanly. My helper pushed `lab.sh` to `C:\lab` by mistake; removed.

## Cleanup

Silent uninstall (`uninstall.exe /S _?=`); the install folder (only `uninstall.exe` was left), the uninstall entry, Start menu folder are gone. Removed `C:\lab\q36h`, the setup and zip copies, my scheduled task, `lab.lock`; stopped app, daemon and Edge. Kept `C:\wt` and `C:\lab\tests`. Laptop not rebooted, still on Windows at the desktop.

## Verdict

PARTIAL. F-1 (keyboard focus when the bar opens with another program in front) is fixed in this build on the lab laptop: 28 of 28 tries; earlier bar behaviour unchanged; regression 57/0 and 68/0.
