# QA 0.3.4 Click Guide on Windows, pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 % (logical = physical / 1.5), 31.7 GB RAM. **Build:** the installed main `0bcac6a3` (`capturemann 0.3.4`; `cm-desktop.exe` SHA-256 `979E002710ACF2DECBF6725CC7F99B4C747F6B31BCED8C97C8700A62960DD014`, the same file before and after the reinstall of this pass). No cm-regress, no reboot, no rebuild.
- **Method:** the installed app with a private library and config (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`), driven from a scheduled task in the logged-in session: UI Automation (names, rectangles) plus real `mouse_event` clicks and drags. Target for clicks: a small WinForms window with three buttons (`Alpha`, `Beta`, `Gamma`). Console and desktop were not private. The bar and HUDs are excluded from capture, so a screenshot shows what lies behind them: the bar was measured by UI Automation only, and "Stop is a red square" was **not** checked visually.
- **Harness:** `HKCU\Console\%%Startup` `DelegationConsole` and `DelegationTerminal` set to the console host GUID for the session and removed at the end (checked: key absent). A file named `con.ps1` cannot exist on Windows (reserved device name): a lost 20 minutes, not a product matter.

| # | Case | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Start from the sidebar tile `Click Guide` (real click) | PASS | state `countdown` for about 3 s, then `recording`; bar window 901x98 px, toolbar `Click Guide` 879x87 px = **586x58 logical** (<= min(720, 1280-48)) |
| 2 | Start from the library header `New Click Guide` (real click) | PASS | same bar. **A UI Automation Invoke on that button answered "invoked" and started nothing** (seen once; a real click works): see defect 9 |
| 3 | Bar buttons | PASS | `Stop` 66x66 px (44x44 logical, a square), `Pause` 126x66, `Note step` 159x66, `Undo step` 162x66, all in the tree. "Stop is a red square" and the countdown look ("Starting... 3", disabled controls) **not verified**: the bar is excluded from capture and UI Automation shows all four buttons enabled during the countdown |
| 4 | Real clicks become steps | PASS | Alpha, Beta, Gamma: 3 clicks, 3 steps, click points in physical px (432,820 / 642,820 / 852,820), `scale` 1.5. Draft titles `Click "Alpha"`, `Click here` (Beta), `Click "Gamma"`: OCR missed the short label Beta and later Gamma on a plain WinForms button |
| 5 | Pause and Resume | PASS | button reads `Resume` while paused, a click while paused added no step (3 -> 3), a click after Resume did (4) |
| 6 | Note step, Undo step | PASS | 4 -> 5 -> 4 |
| 7 | Double click | PASS | one step, title `Double-click "Gamma"`, `count` 2 |
| 8 | Spam: 20 clicks 120 ms apart on one button | PASS (by design) | 10 steps, each `Double-click`, count 2: clicks inside the OS double-click time pair up. Alternating buttons every 100 ms gives one step per click |
| 9 | Double start | PASS with a note | a second `guide start` from the CLI first shows the on-screen confirmation, and only after Allow answers `busy: a Click Guide is already recording`. The click on Allow was itself recorded as a step of the running guide (defect 6) |
| 10 | 0 steps | PASS | `guide stop` with no click: `discarded: true`, no folder |
| 11 | 1 step | PASS | a guide with one step is kept |
| 12 | 100 steps at a human pace (one click per 0.9 s) | PASS | 100 clicks, 100 steps, `cm-desktop.exe` working set 121 -> 127 -> 266 -> 252 -> 139 -> 250 -> 122 MB (no growth) |
| 13 | 100 clicks 100 ms apart, then 520 clicks | **FAIL (defect 1)** | working set 109 MB -> 828 MB after 100 clicks (22 steps yet), 3,578 MB after 520 clicks (97 steps yet); steps kept arriving at about 5 per 5 s for minutes; `guide stop` blocked about 11 minutes; working set fell back to 121 MB afterwards |
| 14 | The 500-step cap | PASS (message not seen) | the saved guide holds exactly 500 steps. The cap message was not captured: the run finished while the backlog drained, and I was not looking at the screen |
| 15 | Titles `<script>` and backticks | PASS with a note | folder `One _script_alert(1)__script_ `tick` & q.cmguide`; HTML escapes (`&lt;script&gt;`, no `<script` tag); Markdown escapes the angle brackets but leaves the backticks, so the heading renders `tick` as code (defect 10) |
| 16 | Window guide: `Click Guide of a window...` | PASS | header button is an icon (60x59 px) beside `New Click Guide`; picker window named `Capture a window` (the purpose copy "Follow a window" is not exposed by UI Automation, so **not verified**), lists `QA Follow, Windows PowerShell 5.1, 878 by 439 pixels, on Display 1`, `Start guide` enabled after choosing the tile; bar 983x98 px, toolbar 961x87 px = **640x58 logical** |
| 17 | Follow: click outside, move, resize | PASS | a click in another window: no step; after moving the window by (+150,+80) and widening it, clicks still became steps (2, 3) |
| 18 | Follow: a window on top of the followed one | NOTE (defect 7) | a click on a second window that lay over the followed window's rectangle became a step |
| 19 | Follow: the window closes | PASS | within 3 s `state: paused, window_closed: true`; bar swaps to `Stop` and `Pick a window...`; toast "QA Follow closed. The Click Guide is paused. Your 4 steps are kept." with `Pick another window` and `Stop and open guide`; `guide set --resume` answers `invalid_input` ("the followed window closed: stop the guide ... and start another"); Stop keeps 4 steps |
| 20 | Kill the app mid-guide | PASS | 2 steps recorded, `cm-desktop.exe` killed, restarted: the guide is in the library (`Click Guides` count +1) with its steps; no toast about the recovery |
| 21 | Agent/CLI-started guide | **FAIL (known CG-2, defect 2)** | confirmation "Allow this action? Start a Click Guide: CaptureMann will take a screenshot of each mouse click (clicks only; keystrokes are never recorded)" with Cancel / Start guide; then **no bar window at all** (checked the UI Automation tree of every process) and only a toast `Started by agent via MCP` (it was the CLI) with `Dismiss notification`, no Stop. Clicks were recorded (`state: recording`) |
| 22 | Editor opens, drag-reorder | PASS | the card opens `Click Guide ... 4 steps`; toolbar `Back to the library`, `Undo`, `Redo`, `Copy as rich text`, `Export...`; fields `Guide title`, `Intro`, `Step title`, `Step note`; `Move up` / `Move down`; real drag of step 1 below step 3 reordered `guide.json` (`step_2, step_3, step_1, step_4`) |
| 23 | Editor: picture fit, `Tall picture` chip, viewer, Esc, retake, Edit image | NOT RUN | the picture button (`Show the picture at full size`, 1032x372 px) exists; the chip, the viewer and Esc were not seen (the viewer adds nothing to the UI Automation tree, and `SendKeys` goes to the foreground window, which was not the app); retake and Edit image were not driven |
| 24 | Export sheet | PASS | `Export guide`: `Markdown`, `HTML`, `PDF`, `Choose...`, `1. Title`, `Step 1 - Title`, `Title only`, `1x`, `2x`, switches `Intro`, `Step notes`, `Note steps without an image`, `Cancel`, `Export Markdown` |
| 25 | Export by CLI: Markdown, HTML, PDF, overwrite | PASS with notes | Markdown: `guide.md` + `images/step-01.png`; HTML 226 KB self-contained; PDF header `%PDF-1.7`, 174 KB. A second export to the same destination **overwrites without a word** (`warnings: []`). No redaction UI anywhere (sheet, editor, bar) |
| 26 | Rich text on the clipboard | PASS | formats `Text`, `UnicodeText`, `HTML Format` (CF_HTML 224,970 chars), `Locale`, `OEMText`; one `<img>` with a `data:image/png;base64` source (no file reference); the plain text carries the title and `1. Click "Alpha"` |
| 27 | No redaction-proposal UI | PASS | none in the editor, the sheet or the bar |

## Defects and proposals from this topic

1. **Memory and backlog under a click burst (medium to high).** Clicks are queued as frames without a limit: 100 clicks in 10 s took the working set from 109 to 828 MB, 520 clicks to 3.5 GB; the guide went on adding steps for minutes after the clicking stopped and `guide stop` waited about 11 minutes. A human click rate is fine. Proposal: drop or coalesce clicks while the writer is behind, show "n steps waiting", and let Stop return after a bounded wait.
2. **Agent/CLI-started guide has no bar (medium, known CG-2).** Silent click recording with a toast that says MCP for a CLI start and offers no Stop (a recording or a Long Shot started the same way shows a HUD with Stop and a status line).
3. See the summary for the rest.
6. **A click on CaptureMann's own confirmation dialog became a step** (low): approving the second `guide start` while a guide ran recorded the Allow click as step 1 (`Click "Tall picture click to see full size"`, the text under the dialog). Proposal: ignore clicks on CaptureMann's own windows, as the spec does for the bar.
7. **A window on top of the followed window** (low): a click on another window inside the followed rectangle is a step. Proposal: ignore a click whose top-level window is not the followed one.
9. **Invoke on `New Click Guide` does nothing** (low, accessibility; once).
10. **Exports overwrite silently; Markdown title keeps backticks** (low).

## Verdict

PARTIAL
