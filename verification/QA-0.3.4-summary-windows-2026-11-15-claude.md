# QA 0.3.4 Windows summary (PARTIAL)

- **Date:** 2026-11-15 (the run began on Nov 14 at about 22:00 and ended after 01:00 on the laptop's clock)
- **Who:** Claude (agent), overnight on the dedicated ThinkPad T490 (Intel UHD 620, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %); nobody watched
- **Build:** main `26a05100` (first build), then the branch `worktree-agent-a6d5c5d69408afd7c` with `e8f8e122` and `ac730b17` (final build, used for the regression tiers: zip SHA-256 `2a3717f6b1c6b2955130874b1fc651128de39f71388f7f2edff760ae98aa8f13`, installer SHA-256 `2a8d475d6731a4d50e423ff5cc682aa9519dfb1e86f3ccdd47a13749644d6498`). Both built on the laptop with `packaging\windows\build-windows.ps1` (`--no-default-features`, LTO off, no speaker engine, FFmpeg and llama-server taken from the installed 0.3.3).
- **Rule:** every report here is PARTIAL (nothing was looked at by a person, floating windows are excluded from capture and were not seen).

## Per feature

| Feature | Report | Result |
| --- | --- | --- |
| CAP-6 Long Shot | `QA-0.3.4-cap-6-long-shot-windows-2026-11-15-claude.md` | Auto PASS (Edge, Chrome; bands, jump, limits 2,000 and 16,384); Hand wheel notches and Down key PASS (Edge, Chrome); **horizontal with a whole-window target FAIL**; horizontal with a region PASS (Edge; Chrome not read by OCR); **Hand with continuous small wheel steps FAIL (open)**; Windows Terminal long output PARTIAL (stops at line 86 of 200); Esc/Stop and the overlay bar not run |
| GDE-1 to GDE-4 Click Guide | `QA-0.3.4-gde-click-guide-windows-2026-11-15-claude.md` | record, titles, double click, pause, keystrokes, outside clicks, clicks on the bar, reorder, delete, undo, retake, exports (Markdown, HTML, PDF, rich text on the clipboard) PASS; `library list` `nullxnull` FAIL on the first build, fixed (`e8f8e122`) and PASS on the final build; editor window actions, redaction proposals, MCP client not run |
| ANN-9 Retype | `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md` | AC1 PASS (0 pixels changed outside the rect), AC4 PASS; AC2, AC3, AC5, AC7 not run |
| ANN-10 Move area | `QA-0.3.4-ann-10-move-area-windows-2026-11-15-claude.md` | AC1 (text level), AC2, AC4 PASS; pixel check and overlap not run |
| ANN-11 Blueprint | `QA-0.3.4-ann-11-blueprint-windows-2026-11-15-claude.md` | op, bulk blueprint, order check PASS; 20-line case and bar geometry not run |
| ANN-8 Zoom Lens | `QA-0.3.4-ann-8-zoom-lens-windows-2026-11-15-claude.md` | op, partly outside, every AC5 rejection, blur plus lens at zoom 2 PASS; pixel comparison and editor not run |
| BRD-1 Boards | `QA-0.3.4-brd-1-boards-windows-2026-11-15-claude.md` | pair, strip, grid, free, deleting a source (AC3, identical hash) PASS; sheet and blur case not run |
| BLK-1 Bulk export | `QA-0.3.4-blk-1-bulk-export-windows-2026-11-15-claude.md` | size, JPEG, watermark flag, name pattern, no overwrite, blueprint plus WebP PASS; 50 files, Cancel, sheet not run |
| Canvas text selection | `QA-0.3.4-canvas-text-selection-windows-2026-11-15-claude.md` | NOT RUN (editor UI, no automation) |
| IMP-6 CMYK JPEG | `QA-0.3.4-imp-6-cmyk-jpeg-windows-2026-11-15-claude.md` | PASS (one synthetic file) |
| Open With, NSIS installer | `QA-0.3.4-open-with-installer-windows-2026-11-15-claude.md` | silent install, handlers (Open with for 10 import types, none for `.cmrec/.cmshot/.cmguide`), run, uninstall, restore PASS; `cm-regress` `installer.silent` PASS |
| B1 delete with a paused upload | `QA-0.3.4-b1-delete-paused-upload-windows-2026-11-15-claude.md` | NOT RUN (needs a Drive account) |
| B2, B3 HUD and bubble drag | `QA-0.3.4-b2-b3-hud-bubble-drag-windows-2026-11-15-claude.md` | bubble PASS (three exact drags), HUD PASS on the regression case and on presses on the bar background; some presses hit buttons or transparent parts |
| Hygiene, version label, library chips, Agents & CLI icon | `QA-0.3.4-hygiene-version-chips-agents-icon-windows-2026-11-15-claude.md` | version `v0.3.4`, chips (All, Audio), icon blue when MCP is ready and grey with `MCP offline` / `Another app owns the socket` when a second process owns the socket PASS; other hygiene items not Windows lab work |
| cm-regress full and hardware, final build | `L1-windows-0.3.4-ac730b17-regression-full-2026-11-15-claude.md` and `...-hardware-...` | full tier 66 passed, 0 failed, 14 skipped (`installer.silent` and `desktop.hud_drag` pass); hardware tier 68 passed, **1 failed** (`rec.camera`: 10.1 fps in an unlit room at night, an environment failure, not repeated), 19 skipped (the 5-minute soak was not run this time) |

## Defects

| # | Defect | State |
| --- | --- | --- |
| 1 | `capturemann library list` printed `nullxnull` for guides (and for audio) | fixed, commit `e8f8e122`, unit test, clippy `-D warnings` clean; PASS on the final build |
| 2 | Horizontal Long Shot of a whole browser window ends after one frame (`no_overlap`); a region works | open; reproduced offline (`long_shot_stitch` with `LS_HORIZONTAL=1`); probable cause is the static toolbar rows that run across the scroll axis |
| 3 | Hand scroll with continuous small wheel steps captures one frame | open, known since the first report |
| 4 | Windows Terminal with 200 near-identical lines stops after 86 | open (PARTIAL) |
| 5 | One first-run Auto Long Shot ended after one frame | intermittent, not reproduced in 6 later runs |
| 6 | The probe could not capture an area; `--region` added, `ac730b17` | tooling |

## Not run (whole list)

Esc and Stop during a Long Shot and the overlay bar, HUD and preview (floating windows), editor window actions of every feature, canvas text selection, B1, the "new" dots, PLUGIN-1, an MCP client for the new tools, Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA and AMD encoders, Excel, Slack and a PDF viewer.

## Lab state

Chrome could not be installed (no administrator rights): Chrome for Testing 157.0.8103.78 was used from a folder. The laptop's own 0.3.3 install, its registry entries and its Start menu folder were saved before the installer test and restored. All test libraries, captures, browser profiles and build outputs of this run were deleted at the end.

## Verdict

PARTIAL
