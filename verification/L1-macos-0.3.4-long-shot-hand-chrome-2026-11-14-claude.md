# CAP-6 Long Shot, Hand scroll on Chrome: "did not line up" report

- **Date:** 2026-11-14
- **Who:** Claude (agent), on the maintainer's Mac, screen capture consented for this test
- **Machine:** Apple Silicon Mac, macOS 26 (Darwin 25.6), built-in display at 2x
- **Build:** `55506bad` plus the uncommitted fix of this change, release build of the `long_shot_probe` example (`cargo build --release -p cm-capture --example long_shot_probe --no-default-features`); not the app bundle, no daemon
- **Browser:** a separate Google Chrome instance with its own throwaway profile (`--user-data-dir` under /tmp), window 1000x800 pt, so the maintainer's Chrome was not touched
- **Fixture:** two local static pages (long lorem-ipsum article; the same with a sticky header bar that hides once the page has scrolled 100 px), scrolled by a small program posting `CGEvent` pixel wheel events (40 pt per event, 500 ms apart; 120 pt / 150 ms) at the window; the probe ran Hand scroll (`--hand-secs`). All captured frames and images were deleted after the run.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Plain article, Hand scroll, wheel 40 pt x10 | PASS (before the fix too) | 11 frames, header 174 px (the browser toolbar), no stop |
| Plain article, wheel 120 pt x6 and 8 pt x30 | PASS (before the fix too) | frames joined, no stop |
| Sticky-header page, first wheel step (before the fix) | FAIL, reproduced | `end: no_overlap`, 1 frame, note "Stopped: the next part did not line up", 3.6 s after the start |
| The failing pair run offline through `long_shot_stitch` (frame 1 and the frame after the step) | reproduced | frame 2 `NoOverlap`; `find_join` gave header 182 and 0 candidates, then header 262 and one candidate (shift 80) on the core, and the edge check then found no strips |
| Sticky-header page after fix 1 (strips found in the bands) | PARTIAL | first pair joined (header 262, shift 80), then ended `no_overlap` at the third frame once the page's header hid (hypothesis b) |
| Sticky-header page after fix 1 and fix 2 (band shrinks) | PASS | 11 frames, header 174 px, no stop, `stopped_by_user`, text continuous in the stitched image (looked at it) |

## Cause (what the dumped frames showed)

Frame 1 has no scrollbar. Once the page scrolls, Chrome's overlay scrollbar fades in; its short thumb sits near the top of the track, on the 80 rows of the sticky header bar alone, in the right margin of the window (outside the bar). The first pair then looked as if the fixed top band ended at the thumb (182 px, the toolbar) so the bar became "content", and the bar, which does not move, found no join. The existing edge-trim (which cuts moving scrollbar columns) only looked at the overlap rows, not at the band rows the thumb was in, so it found no moving column and gave up. Hand scroll then ended the whole capture on the first refused frame.

Hypotheses: (a) scrollbar fading: confirmed as the cause above (not the fade between frames, but a thumb that exists in one frame of the pair only, lying on band rows); (b) hiding sticky header: confirmed as a second failure (the stored band no longer matches); (c) smooth scrolling: not seen (Hand scroll takes a frame only when two polls are equal; no failure on the plain page); (d) blank page: not tested on a real page (a unit test already covers refusing ambiguous joins); (e) small Down-arrow step: not tested with the arrow key (keyboard injection was not possible here); the 40 pt wheel step on the plain page joined.

## Fix

1. `stitch.rs` `edge_strips`: moving columns are now counted over the band rows too, so a scrollbar thumb lying in the bands is cut.
2. `stitch.rs` `Stitcher::join`: when the stored bands no longer match, the bands are found again from the pair and accepted only when narrower; the rows that were band become content.
3. `run.rs` `run_hand_ex`: a frame that does not join no longer ends the capture; it is left out (not retried while unchanged), the next frame that joins is taken; stopping while the last frame was left out ends with `NoOverlap` so the existing note is shown.

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| AC1 | tested with fakes (overlay scrollbar over the header, hiding header, hand and auto) and PARTIAL on Chrome | `cargo test -p cm-capture --no-default-features` 199 passed; the rows above |
| AC3 | tested with fakes (Auto stops; Hand leaves the frame out and notes on Stop) | `cap_6_ac3_*`, `cap_6_hand_scroll_*` |

## Problems found

- Not run: the arrow key and a trackpad (momentum) scroll, a mostly blank real page, any site other than the two local pages, Chrome's own profile with extensions, a second display, Windows and Linux.
- The stitched image of the sticky page shows small black marks at the far left edge; not investigated (possibly the capture of the window's rounded corner). Not caused by this change as far as seen, not compared with a build before the change.

## Verdict

PARTIAL
