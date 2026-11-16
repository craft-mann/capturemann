# Long Shot: translucent sticky header, floating button, horizontal cells, real pages on Linux X11 (PARTIAL)

- **Date:** 2026-11-16, Claude (agent), ThinkPad T490, Ubuntu 24.04, X11, Firefox (snap), 1280x972 window, own profile, no logins, nothing played.
- **Build:** worktree on top of `12302949`; built on the laptop in `~/wt/main54` (release, `cm-capture` example `long_shot_probe`, whole window, Auto, vertical unless noted). Only `stitch.rs` differs there; the last edit before the report (clippy clean-up of two loops) was not re-run on the laptop.
- **Cause A (carchargeos.com):** the page header is opaque white in the first frame and frosted over the scrolled page in every later frame, so its rows differ between any two frames. No per-line tolerance covers a stretch of 75 lines, and the pair was refused ("no shift matches"). The edge-strip detection then took every column for a moving edge (header rows differ in all columns, dithered gradients differ by one colour step) and cut 48 and 64 columns from the sides.
- **Cause B (horizontal whole window):** not the repeated cell labels. The old stitcher ended after 2 frames, the new one joins all 9 frames; the same page on the same window with the old binary: 2 frames 1380 px, new: 9 frames 8252 px (c001 to c040 continuous). The old failure is the same one as A (a stretch of lines that differ in the first frame pair; here the browser chrome rows and the scroll bar). A fake with cells every 194 px and a label per cell joined correctly before the change as well, so it is no proof.
- **Hint:** the join already searches every shift; the expected step only breaks ties (and refuses a tolerant best shift far short of it unless it is exact). The macOS pass-7 explanation (the search looks only near the asked step) is not what the code does; the refusals it quotes name the "closest" shift, not a searched range.

## Changes (`crates/cm-capture/src/long_shot/stitch.rs`)

- `find_join_robust`: last look after the exact and the tolerant one. A shift is believed when at most half of the compared lines differ, in at most 3 stretches, the lines before the first matching line (a header drawn over the page) are left out, every other line matches exactly over at least 40 inked lines (lines of one colour prove nothing) and over as many as the inked lines that differ. The best shift must beat the next by 2x or be the only one near the expected step. The first pair asks for the edges first (`find_join_opt(.., robust = false)`), so a moving scroll bar thumb is cut as an edge and not taken for a stretch; the robust look is the fallback with trim (0, 0).
- `edge_strips`: rows in which more columns differ than the two strips can hold, and one-step colour noise, no longer make every column volatile.
- `same_but_for_a_region`: two frames of the same position that differ in at most 3 stretches of lines, in at most 70 % of the columns (Firefox's link status bar, a preview card) are the same frame (`NoChange`, not `no_overlap`).
- A third idea (join with the differing columns left out, for a hover card over a third of the page) was built, produced a **wrong join on Wikipedia** (static sidebar columns match at any shift) and was removed again.

## Tests (`crates/cm-capture/src/long_shot/tests.rs`, fakes `SyntheticPage::with_frosted_header`, `with_floating_button`, `with_cells`)

| Test | Failed before the fix |
| --- | --- |
| `cap_6_a_translucent_sticky_header_with_a_floating_button_joins` (browser band, scroll bar, frosted header, button) | yes (`NoOverlap`) |
| `cap_6_a_translucent_sticky_header_alone_joins` | yes |
| `cap_6_a_floating_button_alone_joins` | yes |
| `cap_6_a_translucent_header_page_that_jumps_still_ends_no_overlap` | passes before and after (protection) |
| `ls_13_a_wheel_that_moves_far_less_or_more_than_asked_on_a_translucent_header_page` (gain 0.3 to 1.9) | yes (same refusal) |
| `ls_b_horizontal_cells_that_repeat_every_194_px_are_joined_at_the_true_shift` | passes before and after (does not reproduce) |
| the 248 existing `cm-capture` tests, `cap_6_ac3_*` included | pass (254 in all) |

A smooth-scroll variant (first grabs show 0.3, 0.55, 0.55 of the distance) on the frosted page ends `no_overlap` after 3 frames: the distance learned from the unsettled first join makes the third step overshoot, and the fake scroller has no `scroll_back` for the ladder. Not fixed, no test kept.

## Real pages (Auto, vertical, whole window, new build)

| Page | End, frames, size | What the PNG shows |
| --- | --- | --- |
| https://www.carchargeos.com/ (before: `no_overlap`, 1 frame) | `reached_end`, 17 frames, 1280x7339 (twice) | Browser bar once (header 85), the site header once (the first frame's), content continuous to the footer, no doubled or missing section; the scroll-to-top button once at the very end. A few short grey marks of the page's scroll bar thumb remain at the right edge (trim (0, 0), see below) |
| `long-page.html?jump=1&screens=12` | `no_overlap`, 8 frames, 1276x5664, note "Stopped: the next part did not line up" | same as the jump fix report (the part before the jump kept); not re-read row by row |
| `long-page.html?jump=0&screens=12` | `reached_end`, 16 frames, 1276x10733 | not read row by row |
| `long-page.html?wide=1`, horizontal | `no_overlap`, 9 frames, 8252x873 (before: 2 frames, 1380 px) | c001 to c040 continuous, page end reached, tab strip and toolbar cut as static rows; ends `no_overlap` and not `reached_end` (the last short step shows the horizontal scroll bar, not investigated); the last frame's scroll bars are in the image |
| https://docs.rs/serde/latest/serde/ | `reached_end`, 3 frames, 1280x2053, header 117 | not read |
| https://en.wikipedia.org/wiki/Rust_(programming_language) | `no_overlap`, **1 frame** (3 of 3 after the final change; with the removed column-mask look: a wrong join, 2 frames, text doubled at the seam) | not fixed. The pointer the scroller parks in the page opens link preview cards (a different card in each frame), the link status bar changes, and the left contents list and the right panel are fixed columns |
| https://www.youtube.com/@Google/videos | `no_overlap`, **1 frame** (3 of 3) | not fixed. A sticky tabs bar appears when scrolling, the card under the pointer gets a grey hover background and thumbnails start a preview, so the lines of the overlap differ in the middle of the page |

## Not fixed, and what it needs

- Pointer-induced changes (link preview cards, hover backgrounds, thumbnail previews): the stitcher cannot tell these from content that only looks alike without risking wrong joins. The scroller should **park the pointer where nothing reacts** (over the scroll bar gutter or a page margin: the wheel scrolls from there too) instead of the target's centre; that is a change in `long_scroll.rs` and the daemon's point list, not tried here.
- Scroll bar thumb marks at the right edge of carchargeos.com: the first pair joined only through the fallback with trim (0, 0), so the edge strips were not cut.
- Smooth scrolling that is still running at the first frame (see above), and `no_overlap` instead of `reached_end` at the end of the horizontal page.
- Not run: Wayland, the desktop app flow, macOS and Windows, a second monitor.

Verdict: PARTIAL.
