# CAP-6 Long Shot, Hand scroll through the daemon action (Chrome, Terminal): report

- **Date:** 2026-11-14
- **Who:** Claude (agent), on the maintainer's Mac, screen capture consented for this test
- **Machine:** Apple Silicon Mac, macOS 26, built-in display at 2x (windows captured at 2000x1600 and 1800x1440 px, scale 2.0)
- **Build:** release build of `cm-daemon` example `long_shot_hand` on the branch after `1bf29fe2`; not the app bundle, no HUD, no overlay (the example is `capture_screenshot { mode: "long", scroll: "hand" }` as the user with a `request_id`, stopped through `cancel_delayed_capture` like the HUD's Stop)
- **Fixtures:** three local pages in a throwaway Chrome profile (sticky header with clock and blinking caret, lazy-loaded images, 120 paragraphs; wide always-visible scrollbar; long plain article) and a Terminal window with 1,500 lines of scrollback. Scrolled by a small program posting CGEvents (pixel events, line events, a trackpad-like gesture with phases and a momentum tail, Down arrow). Captures deleted.

## What was reproduced

| Case | Result before | Cause found in the refused pair |
| --- | --- | --- |
| Sticky header with a clock and a caret, first frames | 1 frame, `no_overlap` (reproduced) | Two captures of the page differ only in the clock digits and the caret (about 40 px of a 2,000 px line). No fixed band was found, the bar was taken for content that does not move, no shift matched. Fixed by the tolerant look (up to 3 % of a line's pixels). |
| Same page, a frame that differs only by the clock | refused | not "same": now `NoChange` |
| Overlay preview of a trimmed capture | the daemon worker panicked (`range end index 7556 out of range for slice of length 7552`) | `Stitcher::thumbnail` used the untrimmed frame width. Fixed; unit test added. This killed real runs whenever a scrollbar was trimmed. |
| Trackpad-like flick on a plain page, Terminal with a 40 pt event | 1 frame, `no_overlap` | The content moved 1,245 to 1,309 px of 1,440 to 1,600 px before it settled: no overlap with the last frame. Fixed for flicks that pass through joinable frames (bridging); a single event that moves more than a screen in one jump (Terminal with a large pixel delta) cannot be joined by any method. |
| Down arrow on a plain page | one refusal, then 20 frames | the first pair took blank rows below the toolbar as a fixed band; a later frame had text there ("a fixed band changed"). The band-shrink fallback recovered. |
| Lazy images | a settled frame refused | images that load after the first frame change 440 rows: the 2 % line allowance is not enough. NOT fixed (see below). |

After the fixes: Down arrow on the sticky-and-lazy page through the daemon path: 5 frames, 2000x3276 px, no refusal; plain page with line events: 18 frames; plain page with pixel events: limit reached at 16,384 px; no refusal dumps in these runs.

## Not reproduced or not verified

- Terminal.app with line events scrolled only about 3 lines in total in the test (the Terminal did not follow the later events); the Terminal cursor blink on real frames was therefore not seen. The synthetic cursor test covers its shape only.
- The wheel automation stopped scrolling the pages part-way through the session (pixel events stopped working while the trackpad-like and key events still did), so continuous small wheel steps (40 pt every 250 ms) were not run again on the final build; the Windows report's case is covered by a fake-page test only.
- The real overlay, the HUD placement and that no HUD pixels land in frames: not run (no desktop app instance; the capturer excludes its own process's windows for display and region captures, a window capture holds only that window).
- Retina half-pixel scroll offsets: all joins found exact rows at 2x in Chrome (header 174 px, shifts that are whole device pixels); no blended rows were seen.
- Lazy-loaded images that change in the overlap (and a sticky header that hides mid-animation at the first pair) are not handled by the scoring (a shift needs at most 2 % of its lines to differ); a join that tolerates large changed regions needs a scoring change and was not attempted.

## Verdict

PARTIAL
