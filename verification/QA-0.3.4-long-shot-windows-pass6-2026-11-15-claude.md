# QA 0.3.4 Long Shot on Windows, pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. **Build:** installed main `0bcac6a3`, `capturemann 0.3.4`. Method as in the Click Guide report of this pass (scheduled task in the logged-in session, UI Automation, real input). Runs started with `capturemann capture screenshot --window <title> --long --out <png> --json` from the CLI; the confirmation ("Capturemann Cli is asking to run "Take screenshots". You set this tool to Ask first in Settings Agents & CLI.") was answered by clicking `Allow`. Target pages: the repository's `long-page.html` in Edge (`?screens=12`, `?screens=60`), a classic console window, and a page with fixed side bars written for this pass.
- Result summary is by `long_shot.end`.

| # | Case | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Edge, 12 screens, Auto | PASS | `reached_end`, 22 frames, 1082x11744 px, header 120 px (the browser chrome), footer 8, 10 s |
| 2 | Edge, 60 screens, Auto, the 16384 limit | PASS | `limit_reached`, 31 to 33 frames, 1082x16384 px, note "Reached the longest image CaptureMann can edit" |
| 3 | Control page with no fixed bars (same content) | PASS | `limit_reached`, 33 frames |
| 4 | **Page with a fixed left bar and a fixed right bar** | **FAIL (defect 1)** | `no_overlap`, **1 frame** (1082x791), "Stopped: the next part did not line up"; twice out of two; the control page (case 3) with the same rows passes |
| 5 | **Classic console host (`ConsoleWindowClass`, conhost, window fully on the screen, 300 lines, scrolled to the top by hand with the wheel)** | **FAIL (defect 2)** | `nothing_scrolled`, 1 frame (1182x751), "Nothing scrolled: the area may be covered by another window or the page is not scrollable". The same wheel events done by my script scroll the console. A console window larger than the screen (1728x1008 at 190,190) gave the same result |
| 6 | HUD `Stop` | PASS | the passive HUD of a CLI-started run: a toolbar window 531x95 px = **354x63 logical** with `Stop` 66x66 px and the status `Started by agent capturemann-cli`, plus a toast 682x155 px `Started by agent via MCP` with `Dismiss notification`. Clicking `Stop` 1.5 s into the run: `stopped_by_user`, 1 frame |
| 7 | Esc | PASS | Esc sent with `SendKeys` while Edge was in front: `stopped_by_user` |
| 8 | Window moved mid-run | PASS | `window_changed`, note "The window was moved or resized" (in the CLI answer). A toast with that text was **not** seen in the UI Automation tree (only the start toast was there 1 s later) |
| 9 | Window resized mid-run | PASS | `window_changed`, same note |
| 10 | Covered target (a topmost window over the middle of the page) | NOTE (defect 4) | `no_overlap`, 1 frame, "Stopped: the next part did not line up" (the cover stayed on top of Edge, so Auto could not raise the page above it). A normal, non-topmost cover was not tested |
| 11 | Second start while one runs | **FAIL (defect 3)** | the second CLI call shows a confirmation dialog at once, over the page being captured (the dialog at 559,225 802x559 px lies over the Edge window); the first run ended `no_overlap` after 1 frame; the second call was still waiting for the answer 8 s later |
| 12 | Hand scroll (wheel notches, Down key, small steps) | NOT RUN | `--hand` is refused for agents; the sidebar tile `Long Shot` armed a flow whose overlay (`CaptureMann overlay`, 1920x1080) exposes no elements and is excluded from capture, so it could not be driven blind |
| 13 | Sidebar page of the library at several widths | NOT RUN beyond the library pages in the library report | |
| 14 | Sidebar tile state | PARTIAL | the tile `Long Shot` is a button, enabled; after clicking it the tile looks highlighted and no window appears. A toast "Long Shot failed. Window 6882546 not found" stayed on the screen after an earlier tile click and survived a click on its `Dismiss notification` in my script (not reproduced from a clean start) |
| 15 | CLI Long Shot results in the library | PASS | each run saved a `Screenshot ... .cmshot`; a 5894 px tall one is listed as `1082 by 5894 pixels` |

## Defects and proposals

1. **Long Shot cannot stitch a page with fixed side bars (medium).** Left bar 180 px, right bar 140 px, a scrolling middle column of numbered rows: `no_overlap` on the first step. The release text and plan list fixed side content as handled (LS-1); on this build it is not. Reproduced twice; control passes.
2. **Classic console host not scrolled (medium).** Auto ends `nothing_scrolled` on a conhost window that scrolls by hand. Windows Terminal passes (pass 5). Proposal: send the wheel to the console window the way the console accepts it, or say "this window type cannot be scrolled by CaptureMann: use Hand".
3. **A second agent request while a run is active covers the page and breaks the first run (low to medium).** Proposal: refuse with `busy` before showing a dialog.
4. **The message for a covered target is the generic "did not line up" (low).** Proposal: name the cause ("another window is on top of the page").
5. **The confirmation says "You set this tool to Ask first in Settings" for a request the user never configured (low).** For `--long` the wording should say that scrolling a window needs a confirmation.
6. **The move or resize toast was not visible to UI Automation (low, unverified):** the CLI answer carries the text; the toast needs a check by eye.

## Verdict

PARTIAL
