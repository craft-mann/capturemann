# QA 0.3.4 Windows summary, pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. No cm-regress. No reboot. No rebuild.
- **Build:** the installed main `0bcac6a3`, `capturemann 0.3.4`, `cm-desktop.exe` SHA-256 `979E002710ACF2DECBF6725CC7F99B4C747F6B31BCED8C97C8700A62960DD014` (the same after the uninstall and reinstall from `C:\wt\out\capturemann-0.3.4-windows-x64-setup.exe`). `C:\wt` kept. App not running at the end; the `HKCU\Console\%%Startup` keys, scheduled tasks, private library and config, Edge profile and temporary scripts removed.

| Area | Result | Report |
| --- | --- | --- |
| Click Guide: bar sizes, real clicks, pause, note, undo, double click, 0/1/100 steps, window guide, follow, close, kill, editor, reorder, exports, clipboard | PASS (27 cases, details there) | `QA-0.3.4-click-guide-windows-pass6-...` |
| Click Guide under a click burst, agent/CLI start | FAIL (defects 1 and 2) | same |
| Click Guide: retake, Tall picture chip, viewer, Esc, Edit image, countdown look, red Stop | NOT RUN or not verifiable | same |
| Long Shot: Edge Auto, limit, HUD Stop, Esc, moved, resized, passive HUD | PASS | `QA-0.3.4-long-shot-windows-pass6-...` |
| Long Shot: fixed left and right sidebars, classic conhost, second start | FAIL (defects 3, 4, 5) | same |
| Long Shot: Hand scroll | NOT RUN (refused for agents; overlay not drivable) | same |
| Library: header buttons, sidebar, chips, published chips with a fake record, hover-select, board and export sheets | PASS | `QA-0.3.4-library-misc-windows-pass6-...` |
| Uninstall, reinstall, Open With, HUD drag (B2) | PASS | same |
| Bubble drag (B3) | NOT RUN (no camera) | same |
| Long library path, Vietnamese new surfaces | PARTIAL | same |

## Defects (top first)

| # | Severity | Defect | Steps and evidence |
| --- | --- | --- | --- |
| 1 | medium to high | Click Guide queues frames without a limit: a burst of clicks grows memory to GB and makes Stop wait minutes | 100 clicks at 100 ms: `cm-desktop.exe` 109 -> 828 MB; 520 clicks: 3,578 MB, 97 steps written by then, `guide stop` returned after about 11 minutes with 500 steps (the cap) and memory back at 121 MB. At 0.9 s per click: 100 steps, 120 to 270 MB, no growth. 31.7 GB machine; a smaller Mac or PC could swap |
| 2 | medium (known CG-2) | A guide started from the CLI or an agent has no bar and no Stop; it records clicks behind a toast that says "Started by agent via MCP" (it was the CLI) | UI Automation of every process during `state: recording`: only the toast window. A recording or a Long Shot started the same way shows a HUD with Stop |
| 3 | medium | Long Shot ends `no_overlap` after one frame on a page with fixed left and right side bars | 2 of 2; the same rows without the bars: 33 frames. Page `sidebar.html` written for this pass (left 180 px, right 140 px, 260 rows) |
| 4 | medium | Long Shot of a classic console host ends `nothing_scrolled` | conhost window on the screen, 300 lines, scrolls with the wheel by hand; Windows Terminal passed in pass 5 |
| 5 | low to medium | A second Long Shot or guide request while one runs shows its confirmation first, over the captured page, and breaks the first run | first run `no_overlap` after 1 frame; the guide variant answers `busy` only after Allow |
| 6 | low | A click on CaptureMann's own confirmation dialog during a guide becomes a step | the Allow click of a second `guide start` was step 1 of the running guide |
| 7 | low | A click on a window lying over the followed window's rectangle becomes a step | cover test in the Click Guide report |
| 8 | low | Covered Long Shot target says only "the next part did not line up" | topmost cover over the page |
| 9 | low (accessibility) | UI Automation Invoke on `New Click Guide` answers "invoked" and starts nothing | seen once; a real click works |
| 10 | low | Export overwrites an existing file or folder without a warning; the Markdown heading keeps a title's backticks unescaped | CLI export twice to the same destination; title with backticks |
| 11 | low | Vietnamese: the agent confirmation body and its `Start guide` button are English; the `Board title` field name is English | UI Automation names |

## UX proposals

| # | Proposal | Evidence |
| --- | --- | --- |
| 1 | Show the Click Guide bar (or a tray Stop) for agent-started guides, with an "Agent" chip and the right source (CLI, not MCP); the same for Long Shot (already has a HUD) | defect 2, CG-2 |
| 2 | While the guide writer is behind, show "n steps waiting" on the bar and drop or merge clicks beyond a small queue | defect 1 |
| 3 | Refuse a second Long Shot or guide at once with `busy` and no dialog; never show a confirmation over a window being captured | defect 5 |
| 4 | Name the cause when Long Shot cannot continue: "another window is on top", "this window cannot be scrolled by CaptureMann (use Hand)", "the page has fixed side areas" | defects 3, 4, 8 |
| 5 | Short button names get the draft title `Click here` (Beta, Gamma on plain buttons); try the accessibility name of the clicked control (UI Automation) before OCR | Click Guide case 4 |
| 6 | Say "Published" in a card's accessible name and add a status line to the name | library case 4 |
| 7 | One Vietnamese name for Click Guide across tile, page, card and button | library case 15 |
| 8 | Name the window in "Long Shot failed. Window 6882546 not found" and make the toast dismissible by keyboard | Long Shot case 14 |
| 9 | When the library path is too long, say so in the app and name the fallback folder (the banner text was not readable by UI Automation) | library case 13 |
| 10 | The agent confirmation for a scrolling capture should say that the window will be scrolled, not "Take screenshots ... You set this tool to Ask first" | Long Shot defect list |

## Lab state and notes

- Harness lessons (not product defects): a file called `con.ps1` is a reserved device name; `LS` is a PowerShell alias; a topmost leftover test window silently swallowed clicks meant for the app twice, and an open modal sheet made the Vietnamese tile test void; `SendKeys` goes to the foreground window, so Esc cases on the app need the app in front; `conhost.exe powershell ...` from a hidden task creates no window, `Start-Process powershell -WindowStyle Normal` does; the confirmation for a CLI Long Shot or guide must be answered through UI Automation.
- Pass-5 leftovers killed: three `powershell -NoExit term1.ps1` consoles.

## Verdict

PARTIAL
