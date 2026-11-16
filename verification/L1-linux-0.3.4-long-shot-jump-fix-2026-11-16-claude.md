# Long Shot jump fix (CAP-6 AC3) on Linux X11 (PARTIAL)

- **Date:** 2026-11-16, Claude (agent), ThinkPad T490, Ubuntu 24.04, X11, Firefox (snap), 1280x972 window.
- **Build:** worktree on top of `4d4dbda6` (stitch.rs change); built on the laptop in `~/wt/main54` (release, `cm-capture` example `long_shot_probe`, not the installed package).
- **Cause:** the pixel-tolerant look (up to 3 % of a line's pixels may differ) joined two frames that did not overlap. Rows 28 apart in `long-page.html` differ by 4 px of bar and a few of digits, so a one-row overlap "matched" and 28 rows were left out silently.
- **Fix:** a match that holds only with the tolerance must be exact in at least 85 % of its compared lines (`LENIENT_INEXACT_SHARE`).

| Case | Result | Evidence |
| --- | --- | --- |
| `?screens=12&jump=1`, before the fix | reproduced | `reached_end`, 14 frames, 1280x9501, seam ROW 0127 (clipped) to ROW 0156 |
| same page, after the fix | PASS | `no_overlap`, 8 frames, 5664 px, note "Stopped: the next part did not line up"; the part before the jump is kept |
| `cm-capture` tests (248), `cm-daemon long_shot`, fmt, clippy | PASS | new: `cap_6_ac3_nearly_alike_rows_two_screens_apart_do_not_join`, `cap_6_ac3_a_jump_on_nearly_alike_rows_is_not_joined`, `cap_6_nearly_alike_rows_without_a_jump_join` |
| Horizontal `?wide=1&screens=3`, whole window | not fixed | `no_overlap` after 2 frames with and without the fix: separate cause (every row repeats the cell labels every 194 px) |
| https://www.carchargeos.com/ | not stitched | `no_overlap` after 1 frame with and without the fix: sticky translucent header changes between frames |
| Other pages, Wayland, the installed package, the overlay flow | not run | |

Verdict: PARTIAL.
