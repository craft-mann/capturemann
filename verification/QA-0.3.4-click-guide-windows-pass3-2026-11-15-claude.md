# QA 0.3.4 Click Guide on Windows, pass 3 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %. **Build:** main `5e832011`, installed 0.3.4 (setup SHA-256 `9febf0322b639f3be59b2fee44bdd20d14f33dc17bcb65c5507780cf9f699dc8`).
- **Method:** the installed `cm-desktop.exe` with a private library, buttons pressed through UI Automation (invoke), real `SendInput` clicks on an Edge page `QA buttons` (three buttons), bar measured with UI Automation (physical px; logical = physical / 1.5), steps read with `capturemann guide get`.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Library page names: sidebar tiles `Long Shot`, `Click Guide`, rows `Click Guides, n`, `Agents & CLI, MCP ready`; header buttons `Click Guide of a window…`, `New Click Guide`, chips | PASS | UI Automation tree; a screenshot of the app shows the tiles, a blue dot on the Agents icon and the version chip `v0.3.4` |
| Bar height | PASS | toolbar 879x87 px = 586x58 logical |
| Bar width <= min(720, display-48) | PASS | 586 logical (display 1280 logical); a window guide with a longer title 999 px = 666 logical |
| Pause, Note step, Undo step visible and Stop present | PASS | all four buttons in the tree, Stop 66x66 px |
| Countdown state (controls disabled) | NOT SEEN | the tree read 0.9 s after Start showed the same names and `IsEnabled=True` for all, no "Starting" text; UI Automation may not expose the disabled look |
| Real clicks become steps | PASS | 3 clicks, 3 steps (title `Click "Open settings"`) |
| Pause | PASS | state `paused`, the button reads `Resume`, a click while paused gave no step, Resume returned to `recording` |
| Note step button | PASS | step count +1 |
| Undo step button | PASS | 3 steps -> 2 (on one attempt the button was not found for a moment, the retry worked) |
| Stop button | PASS | state none, one guide in the library |
| Window picker | PASS | opens a "Capture a window" window with a list of windows (items with title, app, size); a click on a tile did not choose it, Enter did (my click may have missed the tile) |
| Guide of a window follows the window | PASS | bar with a 999 px toolbar, a click in the window made step 1 |
| The window closes | PASS | within 5 s: `state paused`, `window_closed true`, bar shows Stop and `Pick a window…` (the amber "Window closed" label is not a named element), a toast window 682x280 px appeared; `guide set --resume` answered `invalid_input: the followed window closed: stop the guide (its steps are kept) ...`; Stop kept the guide (`recorded_steps 1`) |
| Collapse at a narrow width | NOT RUN | display scaling and resolution not changed |
| Editor flows (reorder, retake, export sheet) | NOT RUN | not driven in this pass |

## Problems

None new.

## Verdict

PARTIAL
