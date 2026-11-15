# QA 0.3.4 Long Shot on the merged fix, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd`, isolated library/config/socket under `/tmp/cmq2`; the maintainer's app was not running.
- **Method:** `capturemann capture screenshot --long --window <id>` (Auto; the Confirm dialog answered with Tab and Return). **Hand scroll is refused for agents (`--hand`), and the overlay and Long Shot bar are hidden from capture, so Hand was NOT run.** My own windows: a separate Chrome profile with a 120-paragraph page and a sticky header, a new Terminal window with 250 lines. The maintainer's Chrome and Terminal windows were not touched.

| Item | Result | Evidence |
| --- | --- | --- |
| Auto in Chrome, sticky header, run 1 | FAIL | 3 frames, `end: no_overlap`, 2400 x 3336 px, "Stopped: the next part did not line up"; Para 001 to 029 of 120 captured; header taken once (`header_px: 293`) |
| Auto in Chrome, run 2 | PASS | 9 frames, `reached_end`, 2400 x 10005 px; read by eye at reduced size: Para 028 to 120 in order, no repeat, no gap, sticky header once on top (page had been scrolled by run 1). Remarks: small window-corner notches and scrollbar-thumb pieces show at the seams (`macos-pass2-long-shot-chrome-run1.png` shows the notches at Para 013/014 and 018/019) |
| Auto in Chrome at the end of the page | PASS | 1 frame, `nothing_scrolled`, warning "Nothing scrolled: check that the window can scroll" (honest now, no success wording) |
| The limit | PASS | `--max-px 3000`: height exactly 3000, 3 frames, `limit_reached`, "Reached the longest image CaptureMann can edit" |
| Auto in Terminal at the live bottom | PASS | 1 frame, `nothing_scrolled` (nothing below to scroll) |
| Auto in Terminal after scrolling up by hand-made wheel events | FAIL (medium) | window showed Rows 102 to 151 of 250; result 1720 x 2318 px, 2 frames, `end: reached_end`, `header_px 84, footer_px 25`; the picture is continuous Row 102 to 175 (by eye) but then ends, while the terminal had scrolled on to Row 166-215 and the real end is Row 250. So "reached_end" was reported with about 75 rows left |
| Esc during a run | INCONCLUSIVE | pressed Esc 2.5 s after confirming a CLI run; the run finished all 11129 px. Either Esc is not wired for the CLI-driven run or it came too early |
| Hand scroll, overlay, bar | NOT RUN | see method |
| Safari/Preview, Spotlight in Terminal, horizontal | NOT RUN | |

## Verdict

PARTIAL. Auto works in Chrome once of twice (stopped early the first time) and reports "nothing scrolled" honestly; Terminal ended early once.
