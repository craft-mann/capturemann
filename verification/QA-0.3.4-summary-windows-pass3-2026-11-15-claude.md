# QA 0.3.4 Windows summary, pass 3 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent), lab ThinkPad T490 (Windows 11 Pro 10.0.26200, 1920x1080 at 150 %).
- **Build:** main `5e832011`, built on the laptop, installed silently over the previous copy at `C:\Users\trung\AppData\Local\Programs\CaptureMann`. Zip SHA-256 `38abeef6be9b82faf3ca29c92426f9952dc81467e0e4e865423e91f16dfa5151`, setup SHA-256 `9febf0322b639f3be59b2fee44bdd20d14f33dc17bcb65c5507780cf9f699dc8` (unsigned).
- **Regression:** the final `cm-regress` full and hardware runs were NOT run (the maintainer postponed them during this pass). No `L1-windows-0.3.4-*-final2-*` report exists.

| Area | Report | Result |
| --- | --- | --- |
| Long Shot | `QA-0.3.4-long-shot-windows-pass3-2026-11-15-claude.md` | Auto, bands, jump, limits, horizontal whole window (fixed), horizontal region (Edge), Hand wheel/Down key/continuous small steps (fixed), Esc from the CLI (fixed), passive HUD measured: PASS. Terminal stops at line 111 of 200 (FAIL, improved); Chrome horizontal region 1 run FAIL; conhost not run |
| Click Guide | `QA-0.3.4-click-guide-windows-pass3-2026-11-15-claude.md` | bar 58 px high, 586 px wide logical, controls, pause, note, undo, stop, window picker, window follow, window closed (paused, `Pick a window…`, toast): PASS; countdown state and narrow collapse not seen |
| Touch-up ops, boards, bulk export, CMYK | `QA-0.3.4-editor-ops-windows-pass3-2026-11-15-claude.md` | Retype (Vietnamese tight box), Move area, Blueprint, Zoom Lens, boards, bulk (3, 50 files, no overwrite), CMYK: PASS; editor UI not driven |
| Install, hygiene | `QA-0.3.4-install-hygiene-windows-pass3-2026-11-15-claude.md` | install PASS; long library path in the app PASS (banner), via CLI slow and misleading |

## Defects

| # | Severity | Defect | Steps and evidence |
| --- | --- | --- | --- |
| 1 | medium | Windows Terminal Auto Long Shot of 200 similar lines ends with `no_overlap` at line 111 (was 86) | `wt-term` in the Long Shot report: 3 frames, 1721x1711 |
| 2 | low | Auto Long Shot does not raise the target window; when another window covers the centre the wheel goes there and the run ends `nothing_scrolled` ("check that the window can scroll") | Edge covered by a console window, 1 frame |
| 3 | low | HUD Stop (passive HUD of a CLI run) stores `no_overlap` ("did not line up") in 2 of 3 runs instead of `stopped_by_user`; the image is kept | three runs, 8 frames, 5889 px each |
| 4 | low | `capturemann library list` with a 341-character library path and no app running waits 15 s and answers `daemon_unavailable`, without naming the path | the app itself shows the right banner |
| 5 | unresolved | Chrome horizontal Long Shot with a region ended after 1 frame once | possibly my region offset |

Not defects but costing runs: `import` without `--wait` returns before the capture exists; and PowerShell 5.1 reads `"$page?x"` as a variable named `page?x`.

## Not run

cm-regress full and hardware, conhost, narrow-width collapse of the Click Guide bar, countdown state, library hover-select and selection bar, editor window flows, B1, HUD and bubble drag (B2/B3), board `free` layout, bulk cancel, Open With by double click, Windows 10, SmartScreen, second monitor, mixed DPI.

## Lab state

All of `C:\lab\qa`, the helper scripts of this pass and the Chrome copy were removed at the end; `C:\wt` (source, `out`, `bundles`, `target`) kept. The new build is installed and the app is not running.

## Verdict

PARTIAL
