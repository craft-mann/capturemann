# Long Shot engine fixes: Linux mid-page `no_overlap`, LS-12, LS-14, LS-15, seam defects (PARTIAL)

- **Date:** 2026-11-16, Claude (agent `ls-fix`).
- **Machine:** this Mac only; no real screen, no browser, no real window was used. Synthetic pages, scripted scrollers and frame sources (`cm-capture` 265 tests, `cm-daemon` `long_shot` tests, `cm-desktop` long_shot model tests).
- **Not run:** the ThinkPad on Linux (not available), Wikipedia and YouTube in Firefox, carchargeos.com, macOS with the app's own window on top, a window closed on a real Mac, Windows. **Nothing here is "verified" on a real screen.**
- **Pre-existing failure, not from this work:** `cm-daemon` `tests/long_shot.rs::cap_6_hud_state_pause_resume_and_manifest_source` fails (frame count moves while paused) on the base commit `0e73b3c2` as well (checked with the changes stashed, two runs).

## 1. Linux Firefox, Wikipedia and YouTube end `no_overlap` mid-page

**What could be established.** No frames of the failing pair were reachable (no `/tmp` dump, the laptop was not to be touched), so the cause is **not proven**. From the code, a run carries four things from earlier pairs that a fresh stitcher does not: the mask of the first pair (`trim`, `full`), the bands (header, footer), the expected step (hint) and the learned distance. Checked one by one:

- the hint only breaks ties between several candidate shifts (strict look) and cannot refuse a unique exact shift, so it is not the cause of a failure where a fresh stitcher (no hint) joins;
- the bands are re-found from the pair when narrower (existing path) and the robust look leaves a pinned header's lead rows out;
- **the mask is the one that matters**: a vertical run that joined its first pair on the whole width keeps `trim = (0, 0)` for ever. `join_masked` (the retry added for overlay scrollbars) only widened to 16 left and 32 right columns. Wikipedia pins its contents list and Appearance panel, YouTube its left rail and sticky tabs, as side columns that carry ink in many separate stretches of lines; when they grow (the contents list opens a section) no shift matches and the robust look (at most 3 stretches) refuses, whereas a fresh stitcher finds the static columns of that pair (`masked_sides`) and joins. This matches "deterministic at the same place" and "a fresh stitcher joins the same two frames". It does not by itself explain the 120 px step.

**Fix** (`crates/cm-capture/src/long_shot/stitch.rs`, `Stitcher::join_masked`): a pair that does not join is tried again with the side columns that stay put in that pair (`masked_sides`, never narrower than the run's mask) left out of the comparison, the image composed from whole frames as before. Only when no shift matched at all: a refusal that says several shifts match, too little or too alike overlap, "far from the expected" or "only look alike" is not retried with columns left out (that made a wrong join in the terminal test, `cap_6_terminal_near_identical_lines_with_a_status_strip_and_blinking_cursors`, caught by the existing suite).

**Diagnosis for the next real run** (opt-in dump): `refused-NN.txt` now ends with the run's state (trim, full mask, bands, hint) and whether the same pair joins with the hint left out, with the bands left out, and with both left out. One real run of Wikipedia with `CM_LONG_SHOT_DEBUG_DIR` set names the culprit.

**The 120 px step** (the next forward step moving about 120 px instead of 700): not explained. Candidates not separable offline: a nested scroller under the scroll point (a code block or table taking the wheel first and passing the rest on), smooth-scroll animation, or lazy content. The engine already treats a join shorter than the learned distance as normal progress (it looks again after a longer wait, `CONFIRM_BELOW_PREDICTED`); nothing was changed there.

| Test | Failed first? |
| --- | --- |
| `ls_fix_side_columns_that_became_fixed_after_the_first_pair_are_masked_later` (a pinned list of 60 columns, white between ink rows, after a first pair that joined on the whole width) | yes (`NoOverlap`, "no shift matches") |

**What I need from a Linux session** (one run, about 5 minutes): Firefox on Wikipedia Rust with `CM_LONG_SHOT_DEBUG_DIR=<dir>` set, Auto, whole window, vertical; send `refused-01.txt` with its two PNGs (look at them first); then the same on YouTube @Google/videos. If the new build still ends mid-page, the `.txt` says which of hint, bands or mask the pair needs.

## 2. LS-12 (macOS): `nothing_scrolled` when the app's own window covers the target

Cause (from the code, not reproduced on a Mac): the point ladder avoided only windows owned by the asking process (`Capturer::own_window_rects`, matched by pid). A CLI or agent run answered by another process than the one that owns the studio window, or any other app's window on top, was not seen. And the run tried only 3 points in all.

Fix: `Capturer::windows_in_front_of(window_id)` returns the visible windows in front of the target (the OS lists front to back), whoever owns them; the daemon adds them to the covered list for the point order and the park point (`scroll_points`, `park_point` unchanged). The run now tries every point the scroller offers (`EXTRA_POINT_TRIES`, at least the old 3 tries).

| Test | Failed first? |
| --- | --- |
| `windows_in_front_of_a_target_are_those_listed_before_it` (`cm-capture`) | new function |
| `ls_12_the_last_of_five_points_is_tried_before_nothing_scrolled` | yes (failed with `EXTRA_POINT_TRIES = 0`) |

Not shown on a Mac: that this was the cause of the two QA failures.

## 3. LS-14: window closed during the run

Cause: a failing grab (`internal` "Failed to copy data") propagated out of the loop and the frames were lost. Fix: a failed grab after the first frame ends the run with what it has when the OS says the window is unknown (`not_found`) or the source says its target is gone (`FrameSource::target_gone`; the daemon's frames list the windows then). New end reason **`window_closed`** (CLI json `long_shot.end`): the note names the window (`The window "<title>" was closed`), the toast says "The window was closed. Everything up to there is kept: {n} frames, {size}." Docs: `specs/02-agent-interface.md`, `specs/features/01-screenshot.md`, MCP tool description, CHANGELOG; Vietnamese entries are `#, fuzzy`.

| Test | Failed first? |
| --- | --- |
| `ls_14_a_window_closed_during_the_run_ends_it_and_keeps_the_frames` (`cm-capture`: `not_found` and an `internal` error with `target_gone`; an unrelated error still fails the run) | yes (no `WindowClosed`, did not compile) |
| `ls_14_a_window_closed_during_the_run_ends_it_names_the_window_and_saves` (`cm-daemon`) | yes (same) |
| `done_toast` case `window_closed` in `model.rs` tests | new branch |

## 4. LS-15: run on a page already at its end

Choice: scroll to the top first (once), because the person wants the page, and a message leaves them with one useless frame. When the first forward steps move nothing at any point and the scroller can scroll back, it scrolls up in screen-sized steps (at most 200, Stop honoured) until the picture stops changing, takes the first frame again there and runs from the top. A page that moves neither way ends `nothing_scrolled` as before. No new end reason; the JSON is unchanged.

| Test | Failed first? |
| --- | --- |
| `ls_15_a_page_already_at_its_end_is_scrolled_to_the_top_first` | yes (`nothing_scrolled` before) |
| `ls_15_a_page_that_does_not_scroll_either_way_is_nothing_scrolled` | guard, passes before and after |

## 5. Seams

- **A band drawn late (carchargeos "IT approves it. Marketing launches", top half of a heading missing): fixed in the engine.** A reveal animation starts after the screen looked still, so the frame joined with the heading half drawn. After each join the same position is looked at again (up to 3 times, 120 ms apart); a later look that differs from the joined frame only in a few stretches (not as a scroll does) replaces the rows that frame supplied (`Stitcher::refresh_last`); the same applies to a look taken by the "late scroll" check. Cost: about one extra grab and 120 ms per frame. Test `ls_seam_a_band_that_is_drawn_late_is_in_the_image` failed first (with `refresh_last` disabled). Not shown on carchargeos itself.
- **Scroll bar thumb marks at seams on overlay-scrollbar pages: not fixed.** No general technique found that is not a guess: the thumb is drawn over the page's outermost columns only while it moves, differs per frame, and the rows of every frame hold it somewhere; choosing the thumb-free frame for each row would need a second look per row, and masking the columns loses page content (the 16 and 32 column masks already exist for the join and compose from whole frames on purpose). A thumb-free look would need to wait for the overlay to fade (about a second per frame). Left as known.

## Files changed

`crates/cm-capture/src/capturer.rs`, `crates/cm-capture/src/long_shot/{mod,run,stitch,tests}.rs`, `crates/cm-daemon/src/long_shot_ops.rs`, `crates/cm-daemon/tests/long_shot.rs`, `crates/cm-mcp/src/tools.rs`, `apps/desktop/src/ui/long_shot/model.rs`, `locales/{capturemann.pot,vi.po}`, `specs/02-agent-interface.md`, `specs/features/01-screenshot.md`, `CHANGELOG.md`, `docs/development/long-shot-debug.md`.

Verdict: PARTIAL.
