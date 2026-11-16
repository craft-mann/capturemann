# Long Shot: hover-safe pointer, settle and step fixes, banded join, real pages on macOS and Linux (PARTIAL)

- **Date:** 2026-11-16, Claude (agent).
- **Machines:** this Mac (Apple Silicon, macOS 26, Chrome 157 in its own profile and window, 2200x1600 px window at scale 2, Accessibility allowed); ThinkPad T490 (Ubuntu 24.04, X11, Firefox snap in its own profile, 1280x960 window at scale 1).
- **Build:** worktree on top of `0385d8ea` plus the changes below; release builds of `cm-daemon`'s example `long_shot_hand --auto` (the daemon action `capture_screenshot { mode: "long", scroll: "auto" }` in a throwaway library, so the daemon's point list and park point are in use) and of `cm-capture`'s `long_shot_probe`. Mac: scratch target dir; ThinkPad: `~/wt/main54` with the changed `cm-capture` and `cm-daemon` source copied over.
- **Not run:** Windows (the `SetCursorPos` park compiles for `x86_64-pc-windows-msvc`, never executed), Wayland (no injection), a second monitor, the desktop app flow (HUD), Safari, Edge.

## Causes and fixes

| # | Problem | Cause | Fix |
| --- | --- | --- | --- |
| 1 | Wikipedia and YouTube `no_overlap` after 1 frame (Linux), carchargeos 1 to 3 frames (Mac) | The pointer rests in the page centre: hover cards, hover backgrounds, thumbnail previews change what a frame shows | New `Scroller::park_at(point)`. The scroller posts the wheel at the scroll point, then moves the pointer away (macOS `CGWarpMouseCursorPosition` after the events, Windows `SetCursorPos` after 20 ms, X11 XTest motion). The daemon picks the point (`park_point`): beside the area on its display in the widest margin of at least 16 px (right, left, above, below; 1 to 24 px from the edge; never under one of the app's own windows), else the scroll bar gutter inside the area (right edge for vertical, bottom for horizontal). The existing point ladder (`next_point`) is untouched. X11 waits 60 ms after the pointer enters the window before the first click |
| 2 | Wikipedia (Mac, Chrome) still `no_overlap` with the pointer parked | The text matches exactly at the true shift (489 px: PSNR 40 dB over the text, infobox glyph edges differ because the table is drawn at another sub-pixel position); no per-line tolerance covers a third of every line | `find_join_banded`: the last look joins on a band of columns (left 65 %, right 65 %, middle 60 %, halves), believed only when the band moves between the frames, matches exactly over at least 80 ink lines (stretches excepted as in the robust look), the lines of the overlap are at least 80 % distinct from each other, and the shift beats the others. Static side columns cannot decide it (they do not move), repeated rows are refused |
| 3 | macOS Chrome: after the first small step, every big step (1140 px asked) overshot (about 1355 moved), needed the scroll-back ladder, and the ladder gave up on the 3rd page (`no_overlap`) | The wheel moves more for a big request than for the 480 px first step; the learned distance cannot know that, and a join found after a scroll back teaches nothing | `share` of the area a step asks for shrinks by 25 % (down to 35 %) every time a step needed the ladder, so the next steps stay inside the overlap |
| 4 | "scrolled only 520 to 700 of 1469 asked" and the Linux smooth-scroll `no_overlap` after 3 frames | The first join of a run, and a join shorter than the learned distance predicts, were believed after one settle check; a browser animating each step can hold still for longer | After such a join the screen is looked at again after a longer wait (up to 3 times, while more keeps arriving; `CONFIRM_BELOW_PREDICTED` 0.9). The "520 to 700" on the unmodified bundle was both: smooth-scroll partial frames and the step-size dependent wheel (3). Whether the bundle's number came from animation or wheel gain alone was not separated |
| 5 | docs.rs on Linux regressed to `no_overlap` at 3 frames with the new pointer (the page was not at its end) | The first pair called 133 rows a fixed header (a 105 px bar plus blank page rows under it); a later pair had text there ("a fixed band changed"). The band was found again from the pair (header 117) but also 14 equal blank rows at the bottom, which were refused as a wider footer | The pair found again keeps the footer the capture had (a second join with `(new header, old footer)`) |

## Tests (`cm-capture` 258 pass, `cm-daemon` `long_shot` 9 pass, fmt, clippy 1.100.0 clean, Windows target checks)

| Test | Failed first? |
| --- | --- |
| `cap_6_a_wheel_that_moves_more_for_big_steps_does_not_need_the_ladder_every_time` (fake wheel with gain 1.25/1.4/1.6 above 120 px) | yes (ladder used 47 times; with the fake that can scroll back the run ends `no_overlap` without a ladder) |
| `cap_6_a_third_of_the_width_that_changes_in_every_frame_does_not_stop_the_join` (right 30 % differs in every frame; a page with a jump still ends `no_overlap`) | yes (`NoOverlap`) |
| `cap_6_smooth_scroll_on_every_step_with_pauses_does_not_poison_the_distance` (12 animation shapes x 3 gains) | **no**: passes before the fix too. The fake page is too forgiving to reproduce the poisoned distance; kept as a guard |
| `cap_6_a_header_band_that_reached_into_blank_rows_and_a_blank_page_end_still_join` | **no**: passes without the fix. The docs.rs case was reproduced offline from the real frames with `long_shot_stitch` (NoOverlap before, join of shift 384 after) but not with a fake |
| `ls_park_beside_the_area_in_the_widest_margin`, `ls_park_whole_display_uses_the_gutter_and_avoids_our_own_windows`, `ls_park_every_scroller_of_the_list_gets_the_park_point` (daemon) | new functions |

The platform scrollers' park code is not covered by a test (it moves a real pointer); `Scroller::park_at` is covered through the daemon fakes.

## Real pages (Auto, vertical, whole window, daemon path with the park point)

| Page | macOS, Chrome | Linux X11, Firefox |
| --- | --- | --- |
| carchargeos.com | before: `no_overlap` at 3 frames (and 1 to 2 on pass 7). Now `reached_end`, 25 frames, 2190x14443. Read: continuous top to footer, header and browser bar once. The fixed scroll-to-top button appears once per frame (about 8 times as a half circle at the seams); one section heading ("IT approves it. Marketing launches", a page reveal animation) has its top half missing | `reached_end`, 17 frames, 1276x7339 (as before) |
| Wikipedia, Rust | before: 1 frame. Now `limit_reached` (16384 px), 18 frames, 2181x16384. Read 3 of 7 slices: continuous, no doubled or missing text, infobox once, sticky header once | before: 1 frame. Now `no_overlap` after **7 frames**, 1276x4410, up to "Syntax and features". Read: continuous, sticky contents list and Appearance panel repeat once per slice (designed). Not solved: at that position the next forward step moves 120 px instead of about 700, and the join of that pair fails in the run although a fresh stitcher joins the same two frames (state of the first pair's trim). Not found |
| YouTube @Google/videos | before: 1 frame. Now `limit_reached`, 20 frames, 2134x16384. Read 2 of 4 slices: continuous thumbnails and titles, no doubled row, the fixed left rail repeats per slice | before: 1 frame. Now `no_overlap` after 11 to 13 frames, 1228x6125 to 6767. Read one slice: continuous. Ends where Wikipedia does (same cause suspected, not checked) |
| docs.rs serde | `reached_end`, 5 frames, 2172x4327. Read: continuous, sticky sidebar once per slice | before this fix `no_overlap` at 3 frames (page not finished); now `reached_end`, 3 frames, 1276x2053, header 117 |
| a long essay (paulgraham.com/greatwork.html, plain text) | `limit_reached`, 15 frames, 2200x16384. Read one slice: continuous prose; the overlay scroll bar thumb shows as a small grey mark at some seams | `limit_reached`, 24 frames, 1280x16384 (not read) |

Small copies of 6 of the images: `verification/QA-0.3.4-shots/real-sites-*.jpg` (300 px wide, public pages only).

## What still fails and what was not checked

- Linux Wikipedia and YouTube end `no_overlap` mid-page (7 and 11 to 13 frames, was 1). Cause not found. The tail frames showed a forward step moving a fraction of what it moved before while the wheel clearly works.
- Fixed floating elements (scroll-to-top button, a fixed rail, sticky panels) appear once per frame; a scroll-reveal heading can be cut at a seam. Both are page behaviour that a join cannot see.
- Pass 7's Chrome "520 to 700 of 1469 asked" was not reproduced by itself; I could not tell animation from wheel gain on the old bundle.
- Hover safety was not shown by a before/after pair on the same page; the evidence is the end reason and frame count change (Wikipedia 1 to 18 frames on macOS, 1 to 7 on Linux) together with the banded join. On Linux the probe without the park ended after 2 frames, with it after 7 (same page, same build).
- Not run: Windows execution, Wayland, other browsers, other pages with lazy images, a 16 GB limit run.

Verdict: PARTIAL.
