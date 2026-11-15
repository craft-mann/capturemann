# QA 0.3.4 final build, Windows (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** the lab ThinkPad T490, Windows 11 Pro build 26200, Intel UHD 620, one internal 1920x1080 display at 150 % scaling.
- **Source:** `git archive HEAD` of the Mac repo at commit 84bc00cd, unpacked to `C:\wt\main`, built with `packaging\windows\build-windows.ps1` (`rc2_build.ps1`: bundles in `C:\wt\bundles`, `CM_NO_SPEAKER_MODELS=1`, release profile with LTO off).
- **Package (built here, unsigned):**
  - `capturemann-0.3.4-windows-x64-setup.exe` 51,292,816 bytes, SHA-256 `33e914a4921b8a2dc38b06b1ed429ce48c830d48b508d4fde0899b56c90a250a`
  - `capturemann-0.3.4-windows-x64.zip` 72,596,369 bytes, SHA-256 `82d42ab6c1b6e7d74aa9617054e8eba6d5449b8dc8af291e095889fb1718474c`
- **Installed:** the setup was run silently (`/S`) over the previous copy in `C:\Users\trung\AppData\Local\Programs\CaptureMann`; `capturemann --version` prints 0.3.4; the installed `cm-desktop.exe` has SHA-256 `88595e3e...bc08686` (the same as in the regression reports). The app was not running at the end.

## Regression runs on the installed build

`scripts/testlab/regress.sh windows --installed ... --commit 84bc00cd`, reports in this folder:

| Tier | Result | Report |
| --- | --- | --- |
| full (`--allow screen`) | 66 passed, 0 failed, 14 skipped of 80 | `L1-windows-0.3.4-84bc00cd-final-regression-full-2026-11-15-claude.md` |
| hardware (`--allow screen,sound,camera`) | 68 passed, 0 failed, 20 skipped of 88 (`rec.camera` passed at the first try, no dark-room frame-rate failure) | `L1-windows-0.3.4-84bc00cd-final-regression-hardware-2026-11-15-claude.md` |
| 5 minute soak (`rec.soak`) | **not run**: the case needs `--allow mic`, and this run was told not to record the microphone; the runner answered `SKIP rec.soak: needs --allow mic` (that skip report was not kept) | none |

Skipped in both: `ai.transcribe`, `ai.speakers` (no models given), the second-monitor case (`desktop.recording_placement`), `desktop.recording_banner`, the macOS and Linux cases. The installer case (`installer.silent`) ran with the setup above. The `--zip` check of the suite was not run (`regress.sh` has no option for it).

## What was checked for the changes since the last Windows build

Click Guide bar (commit 7058e3f0: `BAR_W` 580, `BAR_H` 58, Stop as the red square icon button, buttons `flex_none`):

1. Started `cm-desktop` from the installed folder with a private library, config and pipe (`C:\lab\gbar.ps1`, deleted afterwards).
2. Pressed "New Click Guide" through Windows UI Automation (an invoke, not a screenshot), waited 7 s.
3. Read the bar window and its named controls from the UI Automation tree (positions in physical px):
   - bar window: x 514, y 13, 892 x 98 (the toolbar inside it is 870 x 87, which is 580 x 58 points at 150 %)
   - Stop 538..604 (66 x 66, an icon button, no label); Pause 827..940 (113 x 60); Note step 950..1094 (144 x 60); Undo step 1103..1252 (149 x 60)
   - all three right-hand buttons lie inside the toolbar (right edge 1395) and the window (right edge 1406); about 143 px are free to the right of Undo step.
4. `capturemann guide stop` afterwards ("No step was taken, so no guide was kept"); all `cm-*` processes stopped; the temp folder removed.

Result: the bar is wide enough at 150 % on a 1920 x 1080 display with the English UI; Pause, Note step and Undo step are present and inside the bar. No screenshot was taken.

Also seen on the way: `capturemann guide start` from the CLI answers a Confirm dialog (802 x 493 px) and, once confirmed, shows only a toast window, not the bar; the bar belongs to the app's own button. That is the designed split (the app starts the guide), noted so nobody reads the missing bar as a bug.

## Not run

- No manual look at the installed app by a person; no screenshots; nothing about colours, icon drawing, spacing or the red square Stop looks was judged, only sizes and positions.
- Not run for the other changes since the last Windows build (Vietnamese strings, hover-select, Retype auto-widen): no UI case exists in the suite for them and they were not exercised by hand. Only the code path of the full and hardware suites touched the same binaries.
- The bar was checked in English only, not in Vietnamese or the pseudo-locale, at 150 % only (not 100 %, 125 % or a second monitor), and not while recording steps (countdown to recording state was not compared; the bar was read once in its state after the 3 s countdown).
- No click on the bar buttons themselves, no drag, no real click recording with the Click Guide.
- Not run: microphone cases, the 5 minute soak, Windows 10, SmartScreen, mixed-DPI monitors, NVIDIA/AMD encoders, a second monitor.

The verdict of every report here is PARTIAL.
