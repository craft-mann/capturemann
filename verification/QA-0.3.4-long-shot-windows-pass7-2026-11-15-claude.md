# QA 0.3.4 Long Shot on Windows, pass 7 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. **Build:** the final build from main `011b5000` (rebuilt on the laptop, installed silently; `capturemann 0.3.4`; `cm-desktop.exe` SHA-256 `46211DC2723E2B5E0C05B8BE132C24285DDC31D0BF5F82E52241A9C4635187EE`).
- **Method:** as in pass 6: the installed app with a private library, a scheduled task in the logged-in session, UI Automation for the confirmation (`Allow`) and the HUD, `capturemann capture screenshot --window <title> --long --out <png> --json` from the CLI. Targets: `long-page.html` in Edge (`?screens=60`), `sidebar.html` (fixed left bar 180 px and right bar 140 px, 260 rows, classic scrollbar; `#plain` hides the bars), a classic console (conhost, `powershell` in a normal window with the console-host delegation keys set), Windows Terminal (`wt.exe`, 400 lines). The registry keys `HKCU\Console\%%Startup` were set for the session and removed at the end.

| # | Case | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Page with fixed left and right sidebars plus a classic scrollbar | **FAIL (defect 1)** | three runs: `nothing_scrolled` (1 frame), `no_overlap` (1 frame), `no_overlap` (1 frame), 1082x791 each. The control (`#plain`, same rows, no bars) passes: `limit_reached`, 32 frames, 1082x16384 |
| 2 | Classic console host, 300 lines, scrolled to the top by hand | **FAIL (defect 2)** | five runs: `nothing_scrolled` 4 times (window verified at the top by a screenshot, `LINE 001` visible); once `no_overlap` after 13 frames (1165x6223). My own wheel events scroll the same window |
| 3 | Windows Terminal, 400 lines | PASS (1 of 2) | `reached_end`, 13 frames, 1182x5791, header 78, footer 40. The first attempt ended `nothing_scrolled` with the terminal already at the bottom (my harness did not reach the top that time), so it is not counted as a product failure |
| 4 | Second Long Shot request while one runs | PASS | the second CLI call answers at once `busy` ("A Long Shot is already running. Stop it first."), exit code 6, **no confirmation dialog** (UI Automation looked for `Allow` 2 s later: none); the first run finished `limit_reached`, 32 frames |
| 5 | Window closed mid-run | PASS | the CLI answer is `not_found`: `The window "Long Shot test page - Personal - Microsoft Edge" is no longer available` (the title is named) |
| 6 | HUD `Stop` | PASS | HUD 66x66 px `Stop` found 12 to 330 ms after `Allow`; clicking it: `stopped_by_user`, 6 to 7 frames |
| 7 | Esc | PASS | `stopped_by_user`, 7 frames |
| 8 | Window moved mid-run | NOTE | one run `window_changed` ("The window was moved or resized", pass 6 behaviour); in the repeat run the move came 2 s after the start and the run ended `limit_reached` with 30 frames at 1082x16384 (the run had already scrolled most of the way; the move did not stop it). Not a clear defect |
| 9 | Window resized mid-run | PASS | `window_changed`, 5 frames, note "The window was moved or resized" |
| 10 | Edge, 60 screens, Auto (control) | PASS | `limit_reached`, 32 frames, 1082x16384, header 120 |

## Defects

1. **Fixed left and right sidebars still fail (medium).** The pass-6 fix (join across a scrollbar, both side bars) did not change the result on this page: one frame and no overlap. The page lies in `C:\lab\ls\sidebar.html` of the lab laptop (source: pass 6 report). The screenshot of the one frame shows both bars and the scrollbar normally (nothing covers the page).
2. **Classic console host (conhost) still not scrolled (medium).** `nothing_scrolled` in four of five runs even with the whole-notch wheel, the message and key fallbacks of the fix. One run got 13 frames and then `no_overlap`, so the fallbacks sometimes move the content but the stitch fails. The wheel events of my script scroll the same window by 3 lines per notch.
3. **Dialog wording and the toast (carried over, low).** The start toast for a CLI run still says `Started by agent via MCP`.

## UX proposals

- Name the cause when a run ends on one frame: "the page has fixed side areas" or "this window type cannot be scrolled; use Hand" (same as pass 6).
- When a console window ends `nothing_scrolled`, offer Hand mode in the message.

## Not run

Hand scroll (refused for agents, overlay not drivable), covered target with a non-topmost cover, KDE/second monitor. Windows Terminal was run once successfully.

## Verdict

PARTIAL
