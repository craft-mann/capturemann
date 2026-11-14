# QA-0.3.4 Long Shot (CAP-6) report, macOS (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent)
- **Machine:** Apple M4 Pro, macOS 26.6.2, 2x display
- **Build:** installed dev bundle of main `26a05100`. No Long Shot fix commit had reached main when I ran (the fix is on branch `worktree-agent-a4ba64ee727bf805a`), so this is the pre-fix state.
- **Fixture:** TextEdit with a 180-line text file ("Line 001: ..."), window 1312x844 physical px. Auto scroll through `capturemann capture screenshot --long --window <id>`; the Confirm dialog answered blind with Tab and Return.

| Item | Result | Evidence |
| --- | --- | --- |
| Auto scroll, TextEdit, run 2 | PASS | 8 frames, `end: reached_end`, 1286x4756 px; OCR of seven 700 px slices lists Line 001 to Line 180 continuously, no duplicate, no gap; `header_px: 64` (title bar taken as a fixed header, nothing repeated) |
| Auto scroll, TextEdit, run 1 | FAIL (flaky) | the same command first returned 1 frame, `end: no_overlap`, note "Stopped: the next part did not line up", the window had not scrolled at all (checked with a window capture), and the call still returned an image as a success. Not reproduced on run 2; I suspect the wheel went elsewhere while another agent held focus, as in D1 of `QA-0.3.4-longshot-settings-2026-11-14-claude.md` |
| Auto scroll in Chrome with a sticky header, Terminal scrollback, Preview/Safari | not run | another agent was using Chrome and Terminal on the same screen for the Long Shot fix; TextEdit was the only window I could use without disturbing it |
| Hand scroll (pixel and line events), Esc/Stop, the 16384 px limit, HUD | not run | Hand scroll is being fixed elsewhere; the HUD cannot be photographed |

## Verdict

PARTIAL. Auto scroll works on TextEdit in one of two tries; the failed try is a sign that "nothing scrolled" is reported as a finished capture.
