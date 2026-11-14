# CAP-6 Long Shot Auto scroll on macOS report (fix of QA D1 and D2)

- **Date:** 2026-11-14
- **Who:** Claude (agent), maintainer approved real capture and synthetic scrolling of the test page for this task
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26.6.2, built-in Retina display (2x), one display
- **Build:** base `cc36432f` plus the fix commit on branch `worktree-agent-a3db847f10550ff78`; debug build, run with `cargo`-built binaries (no app bundle: see Problems found)
- **FFmpeg:** not used
- **Fixture:** `tests/fixtures/long-shot/long-page.html` (`?screens=5`, `&bands=1`, `&jump=1`) served by `python3 -m http.server` on 127.0.0.1, shown in a fresh Chrome 157 window with its own profile (a window of 1200 x 849 points, 2400 x 1698 px); restarted before every run so the page starts at the top

How it was run: (a) `cargo run -p cm-capture --example long_shot_probe` (engine, real capture, real macOS scroller, no daemon); (b) the daemon path with the real capturer and the real scroller, `capture_screenshot { mode: "long", scroll: "auto" }` as `Actor::User`, from a throwaway example (not committed). The confirmation dialog was not driven (an agent over the CLI answers `user_declined / no_confirmer` without the app, and no UI scripting is allowed here). Rows were counted by Vision OCR (`VNRecognizeTextRequest`, accurate) on the stored `original.png`: the number of every `ROW nnnn` read in top-to-bottom order.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Before the fix, probe, plain page | FAIL | D1 reproduced: 1 frame, `reached_end`, note null, image 2400 x 1698; the content did not move (the wheel event went to the window under the old pointer position) |
| Anchor fix only (pointer moved to the area centre) | FAIL | D2 reproduced: scrolls now arrive, 1 frame, `no_overlap` |
| Cause of D2 (frames dumped and compared) | found | the Chrome toolbar of two captures differs by 1 colour step in scattered pixels (max diff 1, 2300 channels in row 0): the exact header test found header 0, so the join ran over the whole frame and failed. After the first fix the page area also differs by 1 step in ~10,600 pixels between two captures of an unchanged page, so the end of the page was `no_overlap` instead of `reached_end` in about half of the runs |
| After the fix, probe, plain page, 3 runs | PASS | 7 frames, `reached_end`, 2368 x 7830 (header 174 px), rows 1 to 87 each once, in order, none missing |
| After the fix, daemon path, plain page | PASS | 7 frames, `reached_end`, 2372 x 7830, rows 1 to 87 once each, in order; 15 s |
| `?bands=1`, probe and daemon path | PASS | `reached_end`, header 270 px and footer 96 px found, 2382 x 8022, rows 1 to 87 once each, in order |
| `?jump=1`, probe and daemon path | PASS | `no_overlap`, note "Stopped: the next part did not line up", the part before the jump is kept (rows 1 to 43 once each in the probe run, 1 to 40 in the daemon run, in order) |
| `--max-px 2000`, probe and daemon path | PASS | `limit_reached`, image height exactly 2000, 2 frames, rows 1 to 21 once each, in order |
| Window already scrolled to the end (page left at the bottom by the previous run) | PASS | 1 frame, `nothing_scrolled`, note "Nothing scrolled: check that the window can scroll" (before: a silent `reached_end`) |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| CAP-6.AC1 (every number once and in order) | PASS on this Mac, Chrome, Retina, probe and daemon path | rows 1 to 87, 7 frames, image height 7830 px |
| CAP-6.AC2 (fixed header and footer once) | PASS | `?bands=1`: header 270, footer 96, rows 1 to 87 once each |
| CAP-6.AC3 (a jump stops at the last good join, the note is shown) | PASS | `?jump=1` |
| CAP-6.AC4 (`--max-px`) | PASS | 2000 px exactly, note shown |

Not run: Safari, a region target, a window on a second display, mixed-scale displays, horizontal scroll, a scaled (non-2x) display mode, the confirmation dialog and the HUD in the real app (bundle not built), `--delay 4`. The scrolling happened in the maintainer's real screen session, no other window was touched. A macOS prompt "CaptureMann-dev is requesting to bypass the system private window picker" was on the screen when the work started (it belongs to the maintainer's running CaptureMann-dev, not to this work); it was not clicked, and it did not block the capture.

## Problems found

- Causes of the two QA findings, per hypothesis:
  - Pointer ignored the anchor (D1): confirmed. `MacScroller` posted at the current pointer position and ignored `anchor`; the wheel went to whatever was under the pointer (after Allow: the confirm panel). Now the pointer is moved to the area centre first (physical px converted to points with the scale of the display that holds the point), the event location is set too.
  - First frame overlapped by the confirm panel or HUD: not measured; a 400 ms wait for Auto scroll before the first frame was added in the daemon so the panel is gone.
  - Smooth scrolling after the settle check: handled, not observed as the cause. An unchanged frame after a scroll is now looked at again up to 3 times, 150 ms apart, before it counts as "nothing changed".
  - Units: pixel-unit wheel events count points, the engine asks in physical px; on this Retina display the content moved 2 x the request (509 asked, 1018 moved). Now the amount is divided by the display scale. The engine learns the real step anyway (it did before the fix).
  - Event to another window: not the case once the pointer is placed.
  - D2 (`no_overlap` on a plain page): not a scroll or unit problem. Cause is the 1-step colour noise between two captures of the same pixels (toolbar and page). Fix: line comparison with a per-channel tolerance of 2 (`Lines::near`) for the fixed-band search, the join check and "same frame". Unit tests: `header_with_one_step_noise_still_joins` (built from the failing frames: noisy toolbar, noisy content, and a final identical frame that must be `NoChange`), `a_scroll_that_moves_nothing_is_not_the_end_of_the_content`.
- A run whose first scroll changes no pixels now retries once and ends with the new reason `nothing_scrolled` (CLI/MCP value, note "Nothing scrolled: check that the window can scroll", a warning toast in the app) instead of `reached_end`. This also applies to a page that is shorter than its window (it cannot be told from a window that ignores the wheel): the single frame is kept. The engine test `cap_6_page_shorter_than_the_area_is_one_frame` was changed to expect it; the spec text for that case should be read with this.
- The probe example keeps the last 12 frames in memory and writes them at the end with `--dump`, so dumping no longer slows the run (writing PNGs each frame made a 12 s run take 42 s and hid the end-of-page failure).
- Speed: a debug build takes 12 to 15 s for 7 frames; not measured on a release build.

## Verdict

PARTIAL: AC1 to AC4 pass with a real window, real capture and the real scroller on this Mac (Chrome only), at the engine level and through the daemon's action with the real components. Not run: the app bundle with the confirmation dialog and HUD, Safari, other displays and scales.
