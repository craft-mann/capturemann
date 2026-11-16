# QA 0.3.4 Windows summary, pass 7 (PARTIAL)

- **Date:** 2026-11-15 (overnight). **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. No reboot.
- **Build:** local main `011b5000` (`git archive` to `C:\wt\main`, `packaging\windows\build-windows.ps1` through a scheduled task, `CM_NO_SPEAKER_MODELS=1`, release profile with LTO off; package built 22:01 to 22:28; `cm-regress` built after). Installed silently over the previous copy (setup exit 0); `capturemann --version` prints `capturemann 0.3.4`. The package printed the known warning (Windows licence list not generated, the checked-in macOS list ships). No speaker engine. Files are unsigned.
- **Checksums (SHA-256):**
  - `capturemann-0.3.4-windows-x64.zip`, 73,443,132 bytes: `D6479ED2FEA29176F0370FFEF89184987DE68246381569E50F5774762FD76BFC`
  - `capturemann-0.3.4-windows-x64-setup.exe`, 51,429,580 bytes: `CCF49DA6FB05C73A79DD212C91E68E251D46C55C210E514FC2A78733F086DA5B`
  - installed `cm-desktop.exe`: `46211DC2723E2B5E0C05B8BE132C24285DDC31D0BF5F82E52241A9C4635187EE`
  - `cm-regress.exe`: `F03575AE2031C9E8D62A38B08624272A4E94AB6902B86A23E8C5F97E80A4F923`
- **Lab state at the end:** the new build is installed, the app is not running, `C:\wt` is kept (`main`, `bundles`, `target`, `out`). Removed: the `HKCU\Console\%%Startup` keys (checked), scheduled tasks, private libraries and configs, the Edge profile, scripts, `C:\lab\ls`, `C:\lab\p6`, the source archive.

## Results

| Area | Result | Report |
| --- | --- | --- |
| Long Shot: fixed left and right sidebars plus a classic scrollbar | **FAIL** (still 1 frame, 3 of 3) | `QA-0.3.4-long-shot-windows-pass7-...` |
| Long Shot: classic console host (conhost) | **FAIL** (`nothing_scrolled` 4 of 5; one run 13 frames then `no_overlap`) | same |
| Long Shot: Windows Terminal 400 lines | PASS (1 of 1 valid run) | same |
| Long Shot: second request while one runs | PASS (`busy`, no dialog) | same |
| Long Shot: window closed mid-run | PASS (names the window title) | same |
| Long Shot: HUD Stop, Esc, resized | PASS | same |
| Long Shot: moved | NOTE (one `window_changed`, one run was not stopped) | same |
| Click Guide: 100 clicks at 100 ms, 520 clicks | PASS for memory (max 325 MB, was 828 MB and 3.5 GB) and Stop (7 to 10 s, was minutes); most clicks of a fast burst are dropped by design | `QA-0.3.4-click-guide-windows-pass7-...` |
| Click Guide: the 500-step cap | PASS (toasts at 450 and 500; saved guide has exactly 500 steps) | same |
| Click Guide: same crop in every step | NOT VERIFIABLE on 1080p at 150 % (the crop is off when it covers more than 85 % of the frame) | same |
| Click Guide: Vietnamese name | PASS (sidebar, tab, cards, buttons) | same |
| Blueprint (bar and box) on a Windows-recognizer image | PASS (looked at the PNGs: no letter tops) | `QA-0.3.4-editor-ops-windows-pass7-...` |
| Retype Vietnamese dot-below in a 36 px box | PASS (looked at the PNG) | same |
| `annotate --ops` file with a BOM | PASS | same |
| Uninstall, reinstall | not run again (the installer regression case ran: see below) | |

## Regression on the installed final build (`--installed --commit 011b5000`, `--installer` the setup above)

| Tier | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| full (`--allow screen`) | 66 | 0 | 14 | `L1-windows-0.3.4-011b5000-final3-regression-full-2026-11-15-claude.md` (+ `.json`) |
| hardware (`--allow screen,sound,camera`, no soak) | 67 | **1** | 20 | `L1-windows-0.3.4-011b5000-final3-regression-hardware-2026-11-15-claude.md` (+ `.json`) |
| hardware, `rec.camera` only, repeated | 0 | 1 | 0 | `L1-windows-0.3.4-011b5000-final3-regression-hardware-camera-rerun-2026-11-15-claude.md` (+ `.json`) |

Failure: `rec.camera` - "the camera ran at 10.3 fps (a dark room makes some cameras drop to 10 fps; light the room and rerun)"; the repeat gave 10.1 fps. The room is dark at night (the maintainer is asleep); it was **not** lit, so this is the known low-light behaviour of the camera, not a verified product defect. Skipped cases are the same classes as before: second monitor, microphone (`rec.mic`, soak), Whisper models, speakers, the Linux and macOS-only cases, `guide.real_clicks` (needs the desktop confirmation UI). The soak was not run (needs the microphone).

## Defects (top first)

| # | Severity | Defect | Evidence |
| --- | --- | --- | --- |
| 1 | medium | Long Shot of a page with fixed left and right sidebars plus a classic scrollbar still stops after one frame (`no_overlap` or `nothing_scrolled`); the code fix of pass 6 did not take effect on this page | 3 of 3 runs; the same rows without the bars give 32 frames |
| 2 | medium | Long Shot of a classic console host (conhost) is still not scrolled: `nothing_scrolled` in 4 of 5 runs although the window is at the top and a script wheel scrolls it; one run produced 13 frames and then `no_overlap` | Long Shot report case 2 |
| 3 | low to medium | A fast burst of clicks (10 per second) in a Click Guide keeps only about 10 of 100 clicks (toast "A click was not saved: ... still saving the earlier clicks"); about 0.8 steps per second are written | Click Guide report cases 1 and 2 |
| 4 | low | `rec.camera` runs at about 10 fps in the dark room (not repeated with light) | regression reports |
| 5 | low | Cap toast has a doubled full stop: "was saved.. Start a new guide" | Click Guide case 3 |
| 6 | low | `annotate` right after `import` answers `not_found` while the import job is still `queued` | editor report |
| 7 | low (carried) | The start toast of a CLI-started Long Shot or guide says "Started by agent via MCP" | pass 6 |

Fixed and checked on this build: guide memory growth and slow Stop (pass 6 defect 1), `busy` without a dialog (pass 6 defect 5), window title in the "no longer available" error, Blueprint and Retype coverage and the BOM (pass 5 defects 1 to 3), the Vietnamese name.

## UX proposals

| # | Proposal | Evidence |
| --- | --- | --- |
| 1 | Make the Click Guide frame writer cheaper (or write in parallel) so 3 to 5 clicks per second are kept; show "n steps waiting" on the bar | defect 3 |
| 2 | When Long Shot ends after one frame, name the cause and offer Hand mode: "the page has fixed side areas" or "this window cannot be scrolled automatically" | defects 1, 2 |
| 3 | Make `import` wait until the capture can be annotated, or say `queued` in the id answer | defect 6 |
| 4 | Say "CLI" instead of "MCP" in the agent start toast | defect 7 |
| 5 | Report the guide crop (or `none`) in `guide get` so the framing can be checked by a script | Click Guide case 4 |

## Not run

Hand scroll (refused for agents), bubble drag (no camera light), the 5-minute soak (microphone), second monitor, Windows 10, SmartScreen, mixed DPI, NVIDIA/AMD encoders, agent confirmation in Vietnamese, the guide bar of agent-started guides (CG-2), Intel Mac. No "verified" is claimed.

## Verdict

PARTIAL
