# CAP-6 Long Shot: manual script

Run per system on the installed build. It scrolls real windows: do it at the machine, with
nothing private on screen. Report in `verification/` (template in its README); say "verified"
only with that report.

Setup: a page 5 screens long with a numbered line on every row (`tests/fixtures/long-shot/`),
open in Chrome. Also: Excel or LibreOffice Calc with 500 numbered rows, Slack or Discord with a
long thread, a PDF viewer with a 10-page PDF.

| # | Steps | Pass |
| --- | --- | --- |
| 1 | CLI: `capturemann capture screenshot --long --window <Chrome id>` and approve the dialog (agent) or run as the user. | Every number once and in order (AC1; `capturemann text <id>` to check), result `long_shot.end` = `reached_end`. |
| 2 | Same page with `?bands=1` (fixed header and footer). | Header once at the top, footer once at the bottom (AC2). |
| 3 | Page with `?jump=1`. | Stops with "Stopped: the next part did not line up"; the part before the jump is kept (AC3). |
| 4 | `--max-px 2000` on a longer page. | Image is 2000 px, `end` = `limit_reached`, the limit note is shown (AC4). |
| 5 | Wayland: Hand scroll only (`--hand`), page scrolled by its own script. | AC1 passes (AC5). Note whether the desktop asked once or every frame (spike S6). |
| 6 | macOS without Accessibility (remove it in System Settings): Auto scroll. | `permission_missing` / "Allow Accessibility to scroll for you"; Hand scroll still works (AC6). Settings > Permissions shows the Accessibility card. |
| 7 | `--horizontal` on a wide spreadsheet. | Columns once and in order. |
| 8 | An agent calls with `scroll: "hand"`. | `invalid_input`, `data.reason` = `needs_user` (AC7). |
| 9 | Excel, Slack/Discord, PDF viewer, each with Auto scroll. | Note per app: end found, no duplicated rows, settle time. |

Record per app: the scroll step in px that one event gave, how many frames, the total time.
