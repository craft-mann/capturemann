# QA 0.3.4 GDE-1 to GDE-4 Click Guide on Windows (PARTIAL)

- **Date:** 2026-11-15 (run on the laptop's clock Nov 14, 23:10 to 23:30)
- **Who:** Claude (agent); nobody watched
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %, Edge InPrivate on a local test page (`buttons.html`: three buttons "Open settings", "Save profile", "Send invoice")
- **Build:** main `26a05100` (release package built on the laptop, see `QA-0.3.4-cap-6-long-shot-windows-2026-11-15-claude.md` for hashes); `capturemann library list` of this build still prints `nullxnull` (the fix is `e8f8e122`, in the final build below)
- **Data:** isolated library, config, pipe, secrets file; deleted at the end. No picture of a browser window was stored.
- **Method:** the CLI against `cm-desktop` hosting the daemon; real `SendInput` clicks on the page; the agent confirmation answered on screen with Tab and Enter; the bar started once through the library header button (a click on the app window); exports read back from disk; the clipboard read with a STA PowerShell. The Click Guide bar is a floating window excluded from capture: its rectangle was read with `EnumWindows` and it was not seen.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `guide start --window "QA buttons" --countdown 2` (Confirm tier), then 3 real clicks on the 3 buttons (GDE-1.AC1) | PASS | 3 steps in order titled `Click "Open settings"`, `Click "Save profile"`, `Click "Send invoice"`; click points 278,282 / 278,396 / 278,510 in physical px of the step image (the buttons' own positions) |
| 3 key presses while recording (NFR keystrokes) | PASS | step count stayed 3 |
| A click outside the target window (GDE-1.AC2) | PASS | stayed 3 |
| Double click | PASS | one step `Double-click "Send invoice"`, `count` 2 |
| `guide set --pause`, a click, `--resume`, a click (GDE-1.AC3) | PASS | steps while paused 4 (unchanged), after resume 5 |
| `guide set --note-step "Type your password" --note ...`, `guide stop` | PASS | step 6 `type: note`, no image; `guide.json` schema 1, 5 `steps/step_N.cmshot` folders, `thumbs/poster.jpg`, `manifest.json` |
| Started from the library header button (the app's own entry), bar at 597,13 727x107 (physical px) | PASS | a click on the desktop while recording became a step (positive control) |
| Clicks on the bar are not steps (GDE-1.AC2), 22 clicks on the bar's top strip (y 19) and its lower padding (y 104), x from 3 % to 97 % | PASS | 0 steps; positive controls before and after each gave steps. (A first sweep also clicked 6 px above the window's lower edge, below the 470x64-point bar the exclusion covers: 10 steps. That point is outside the drawn bar, so it is not counted as a defect; the window rect from `GetWindowRect` is 22x11 px larger than the bar at 150 %.) |
| Reorder: `guide set --steps @file` moving step_4 to position 2, then `undo` (GDE-2.AC1) | PASS | order `1,4,2,3,5,6`, titles follow, `undo` restores `1,2,3,4,5,6` in one step |
| Delete step_4 (GDE-2.AC2) | PASS | `steps/` lost `step_4.cmshot` at once, the folder waits in `.undo/step_4.cmshot`; `undo` brings both back; delete again leaves `.undo/step_4.cmshot` (history can still restore it) |
| Insert a note step with `--steps` (no id) | PASS | new `step_7` placed after the listed ones |
| Focus crop on, numbering `upper`, title, intro, step title and note | PASS | `focus_crop: true` for step_1; markdown headings `A.` `B.` ...; title and intro in `guide.md` |
| Retake with the followed window alive | PASS | new image folder `step_1-retake.cmshot` (48,475 bytes against 48,150), the old one in `.undo/`, `guide.json` points at the new one. With the window closed: `not_found: Window <id> not found` (no crash) |
| Export Markdown (GDE-3.AC1) | PASS | `guide.md` 480 bytes, `images/step-01.png` to `step-05.png` (72 to 73 KB), relative links only (0 absolute paths), the note step has a heading and no image |
| Export HTML | PASS | one file 488,725 bytes |
| Export PDF | PASS | one file 284,223 bytes |
| Copy as rich text with the app hosting the daemon | PASS | `copied_to_clipboard: true`; clipboard formats `UnicodeText, Text, HTML Format, Locale, OEMText`; `HTML Format` 486,962 chars with 5 `<img>`; plain text starts `QA guide renamed / This is the intro. / A. Click "Open settings"` |
| `library list --kind guide` and `library list` | FAIL on the first build, PASS on the final build | first build: `guide_... guide nullxnull ...`; commit `e8f8e122` prints `-` (unit test `a_row_without_a_size_prints_a_dash_not_nullxnull`); final build (zip SHA-256 `2A3717F6...AA8F13`): `guide_06GXNQ0N0GNZA5ZT  guide  -  ...` and audio rows `-` too |
| Auto-redact proposals on a step with an e-mail address (GDE-1.AC4) | not run | the test page has no e-mail address |
| Editor window (drag reorder, delete button, Retake and focus-crop buttons, notes fields) | not run | UI driving was not built; the same actions through `guide set` were run |
| Export sheet options and the "Review step N" offer | not run | |
| MCP tools `start_guide`, `stop_guide`, `get_guide`, `set_guide`, `export_guide` (GDE-4) | PARTIAL | the CLI commands call the same daemon actions and ran above; the MCP server itself was not driven for guides in this run (`cm-regress` `mcp.*` cases passed in the full tier, see the regression reports) |
| Click ring and badge look | not run | carried from `L1-windows-0.3.4-main-a1bf57dc-feature-cases-...` (PASS, looked at); the exported PNGs of this run were not re-inspected |

## Acceptance criteria

GDE-1: AC1 PASS, AC2 PASS, AC3 PASS, AC4 not run, AC5 macOS only, AC6 Wayland only. GDE-2: AC1 PASS, AC2 PASS. GDE-3: AC1 PASS (relative links, 5 images in order), AC2 not run (no blur step exported). GDE-4: PARTIAL (CLI; MCP not driven).

## Problems found

1. `capturemann library list` printed `nullxnull` for guides: fixed (`e8f8e122`).
2. Nothing else. The note on the bar's window size above is an observation.

## Not run

The editor window actions, Auto-redact proposals, export sheet, the MCP tools driven through an MCP client, Wayland, Input Monitoring on macOS, Windows 10.

## Verdict

PARTIAL
