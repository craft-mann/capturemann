# QA 0.3.4 Windows summary, pass 5 (PARTIAL, stopped early)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. No cm-regress. No reboot.
- **Build:** main `0bcac6a3`, installed silently at `C:\Users\trung\AppData\Local\Programs\CaptureMann` (`capturemann 0.3.4`; `cm-desktop.exe` `979E002710ACF2DECBF6725CC7F99B4C747F6B31BCED8C97C8700A62960DD014`). Zip `capturemann-0.3.4-windows-x64.zip` SHA-256 `BC0F5ED083685F41A6C5FD27225FB338D9B4E597F7FDA6C5CA3710395F3534B6` (73,191,696 bytes); setup `capturemann-0.3.4-windows-x64-setup.exe` SHA-256 `9C1C8825EADC07147839AC16CD0CE776B1A8AB1293C3058AA87D067870D738E1` (51,329,544 bytes), in `C:\wt\out` (kept, with `C:\wt\main`). App not running; temp scripts, libraries, tasks removed.

| Area | Result | Report |
| --- | --- | --- |
| Install | PASS | `QA-0.3.4-install-windows-pass5-...` |
| Text default colour Auto (light, dark, busy) | PASS | `QA-0.3.4-library-editor-boards-windows-pass5-...` |
| Retype, Move area, Zoom Lens, boards (4 layouts), bulk export, CMYK | PASS, with defects 2 and 3 | same |
| Blueprint | FAIL (defect 1) | same |
| Long Shot Windows Terminal, 400 lines | PASS | `QA-0.3.4-long-shot-windows-pass5-...` |
| Long Shot: conhost, covered target, HUD Stop, Esc, window moved/closed, second start, Hand, passive HUD, sidebar widths | NOT RUN | |
| Click Guide (all of it), library page checks, published chips, hover-select, Open With, uninstall, B2/B3 drag, long library path, Vietnamese UI | NOT RUN | scripts were written and not run: my harness cost the time (see the long-shot report) |

## Defects

| # | Severity | Defect | Steps and evidence |
| --- | --- | --- | --- |
| 1 | medium (privacy) | Blueprint leaves the tops of text visible and a letter sticking out: Windows OCR line boxes are shorter than the text, and bars or boxes only cover the box | 800x500 image, 28 px Segoe UI lines; `annotate --suggest-blueprint`, then a `blueprint` op with those boxes (bar and box styles); "Plan: Team" and "Amount: 99.00" show above the bar |
| 2 | low | Retype in a box about as high as the font clips Vietnamese dot-below marks and descenders flat, and leaves the old line's lower marks as specks | 36 px box, 30 px text `Ngọc Ấn Đặng Thị Hường`; a 56 px box is fine |
| 3 | low | `annotate --ops file` rejects a UTF-8 file with a BOM with a generic "expected value at line 1 column 1" | file written by Windows PowerShell `Set-Content -Encoding UTF8` |

## UX proposals

| # | Proposal | Evidence |
| --- | --- | --- |
| 1 | Pad the Blueprint boxes to the line height (or measure the glyph extent) before drawing, and say how many lines were found | defect 1 |
| 2 | Retype and Move area: draw the new text unclipped by the box, or warn when the box is shorter than the font's line | defect 2 |
| 3 | Boards: give each cell a hairline border (white screenshots on a white board and dark ones on a dark board vanish); scale caption size with the board width | grid and strip PNGs |
| 4 | Strip the BOM in `--ops` and `--steps @file` | defect 3 |
| 5 | The long destination error repeats the whole 300-character path; shorten it in the middle | bulk export |

## Lab state and notes

Windows Terminal as the default terminal breaks console programs run from the task session (`0xC0000142`); registry fix used and removed (details in the long-shot report). No stale `wt`, `WindowsTerminal`, `cm-*` processes remain.

## Verdict

PARTIAL
