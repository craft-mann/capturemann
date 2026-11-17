# BUG-6 report: Long Shot (Auto scroll) on Windows, Edge, sticky headers

- **Date:** 2026-11-17
- **Who:** Claude (agent), on the lab laptop (dedicated to Claude by the maintainer)
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11, one display at 150 % scale; Edge in a fresh guest profile (`--guest --disable-sync`), window placed at 1280x900 (the captured window was 1902x1097 px)
- **Build:** worktree branch `track/long-shot-win` on top of `32c308fd`, release build of the `long_shot_hand` example (`cm-daemon`) in `C:\wt`; not the shipped app. First the 0.3.4 sources (baseline), then with the three changes below.
- **Fixture:** real sites (YouTube `@Google/videos`, carchargeos.com, Wikipedia Rust, docs.rs/serde, paulgraham.com/greatwork.html) and the local page `tests/fixtures/long-shot/sticky-lazy.html`. Captures of the pages were deleted after the runs; only counts and text are kept here. Screen lock `C:\lab\screen.lock` held and removed.

## What was found

| Finding | Evidence |
| --- | --- |
| Reproduced on the 0.3.4 sources: YouTube ended after 2 frames | trace: first step 329 px, `NoOverlap` three times, a scroll back joined (`Added 299`), the next step (338 px, expected shift 612) was `NoOverlap` and the ladder ended: 2 frames |
| Cause 1 (wheel quantum): the Windows scroller sent **whole wheel notches only** (at least one). In Edge at 150 % one notch moves about 300 px. A request of 165 to 330 px became 2 to 3 notches (600 to 900 px). YouTube pins a masthead and, after the first scroll, a tab bar, leaving about 820 of 1097 px of content, so the first step, and later steps sized from a learned distance, passed the end of the last frame (the pair shared no row) | a first step of 329 px moved 972 px; steps of 205 to 250 px asked after the rescue moved 3.0 times the request when fine deltas were sent |
| Cause 2 (slow failure, not the cause of the end): a refused pair of 1902x1097 frames takes the stitcher about 8 s on a Mac (release) and several times that on the laptop; the first-pair retries looked three times at the same unchanged frame. A run of 60 to 120 s therefore showed only the first lines of the ladder and ended `stopped_by_user` | `long_shot_stitch` on frames 001+002 of a failing run: 7.9 s |
| Cause 3 (found by the rerun, Wikipedia regressed to `no_overlap` after 12 frames once steps were exact): a pinned contents list whose current item changes (`History` bold, `(Top)` blue) differs in about 4 % of the lines in its text columns. `masked_sides` cut the run of unchanging columns at the first column over 2 % of the lines, so the panel's other columns stayed in the comparison. At the true shift the page's own columns match with 2 rows per mille differing | `long_shot_pair` with columns 400..1000 only: shift 482, 2 per mille differ; the full pair: 79 % differ; with the share at 0.08 `long_shot_stitch` joined the pair offline |

carchargeos.com: its 0.3.4 "stops after 19 frames" case was not reproduced as such before the fix (no baseline run of it in this session); after the fix it ran to the end of the page.

## Changes

- `crates/cm-capture/src/sys/long_scroll.rs` (Windows): a target that is not a console window gets a wheel in proportion to the request (`wheel_deltas_fine`, 120 units per 100 px asked, at least 12 units, events of at most 240 units) instead of whole notches. Console windows (conhost, Windows Terminal) keep whole notches, because conhost ignores a part of a notch. The driver's learned distance now works with a continuous wheel.
- `crates/cm-capture/src/long_shot/run.rs`: the first-pair retry does not push a frame equal to the one just refused (the same answer, many seconds).
- `crates/cm-capture/src/long_shot/stitch.rs`: `STATIC_SHARE` 0.02 to 0.08.

## Steps and results (Auto scroll, Edge guest, 1902x1097 px window)

| Step | Result | Notes |
| --- | --- | --- |
| YouTube, 0.3.4 sources | FAIL (reproduced) | 2 frames, `no_overlap` (the run then waited for the harness stop: `stopped_by_user` after 120 s) |
| YouTube, fine wheel only (60 s run) | inconclusive | 1 frame when stopped: the run was cut off during the first slow refused pair (see cause 2); a 400 s run of the same build reached 27 frames and the 16,384 px limit |
| carchargeos.com, fine wheel | PASS | 24 frames, `reached_end` |
| Wikipedia, fine wheel only | FAIL | 12 frames, `no_overlap` (cause 3) |
| YouTube, all three changes | PASS | 27 frames, `limit_reached` (16,384 px) |
| carchargeos.com, all three | PASS | 24 frames, `reached_end` |
| Wikipedia, all three | PASS | 28 frames, `limit_reached` |
| docs.rs/serde, all three | PASS | 5 frames, `reached_end` |
| paulgraham.com/greatwork, all three | PASS | 26 frames, `limit_reached` |
| `sticky-lazy.html` (sticky masthead + sticky tab bar + lazy pictures), all three | PASS | 44 frames, `limit_reached` |

Only the end reason and frame counts were checked, not the stitched image row by row (no OCR or visual comparison was done for the full images; the YouTube result of an earlier 60 s run was looked at as a single frame only).

## Tests (Mac, no screen, no input)

`cargo test -p cm-capture --lib --locked`: 277 passed. New: `fine_wheel_deltas_follow_the_request` (long_scroll), `bug_6_a_tall_sticky_header_and_a_wheel_of_three_times_the_request_joins_to_the_end` (engine: header 160 and 275 of 1100 px, wheel 3x; it passes without the other changes, since the engine already coped, so it guards that behaviour only), `bug_6_a_panel_whose_current_item_changes_is_left_out_of_the_comparison` (fails with `STATIC_SHARE` 0.02, passes with 0.08). `cargo clippy -p cm-capture --all-targets --locked -- -D warnings` clean, `cargo fmt --all` clean.

## Not run

- No `cm-regress` case: Long Shot has none yet and a case needs a browser launch and window placement in a desktop session; the local page is stored as a fixture (`tests/fixtures/long-shot/sticky-lazy.html`) and was run by hand on the laptop only.
- The installed app and the HUD flow (the daemon action was driven through the `long_shot_hand` example, as in earlier runs); Hand scroll; Windows 10; a second monitor; mixed DPI; other scales than 150 %; NVIDIA/AMD; Chrome and Firefox on Windows; macOS and Linux were not touched by the wheel change (Windows scroller only) but the `STATIC_SHARE` and run.rs changes are shared code and were checked only by the engine tests there.
- Single runs per page (no repeat for variance); YouTube and carchargeos.com content changes daily.
- No baseline run of Wikipedia, docs.rs and paulgraham.com on the 0.3.4 sources in this session (they are recorded as working in the 0.3.4 reports).

## Verdict

PARTIAL. The 2-frame end on YouTube was reproduced and, with the three changes, YouTube, carchargeos.com, Wikipedia, docs.rs, paulgraham.com and a local sticky/lazy page all ran to the end of the page or the 16,384 px limit on the laptop (one run each).
