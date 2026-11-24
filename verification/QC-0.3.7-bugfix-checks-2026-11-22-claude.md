# QC 0.3.7 bug-fix checks report

- **Date:** 2026-11-22
- **Who:** Claude (lab laptop, scripted with xdotool on Ubuntu and a PowerShell probe on Windows)
- **Machine:** ThinkPad T490, Intel UHD 620. Ubuntu 24.04, GNOME 46 on Xorg; Windows 11 (1024x768 console session)
- **Build:** Ubuntu: commit 43ce8c1d, release, `--no-default-features`. Windows: commit b4542260, release, `--no-default-features`
- **Fixture:** none (isolated CAPTUREMANN_LIBRARY, CONFIG_DIR, SOCKET; test output deleted)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Linux X11: drag the bar by its background | PASS | 8 pointer moves of (+25,+15) moved the bar from 529,540 to 729,660 (xwininfo). The move only took effect once the request's reply was awaited |
| Linux: Tab in the Edit preset sheet (N-7) | PASS | After two Tab presses the focus ring moved from Screenshot to Video (screenshots taken) |
| Windows: bar frame after open (F-4) | PASS | Opened at 159,400 882x107, unchanged after 2 s and after the picker opened and closed (before the fix: 397 then 400) |
| Windows: no "window not found" on close (F-8/K3) | PASS | No ERROR line in the log after open, picker, Esc, Esc (before: one ERROR line) |
| Windows: picker above the bar (F-3) | Observed, no change | The window grows upward to 159,15 882x492; its bottom edge stays at the bar's. By design; spec statement to be added |
| Windows: "RefCell already borrowed" on the recording bar (F-8) | PASS | Quick Capture video + Display recording started; recording bar (571x101) and frame appeared; no such line in the log. Test recording deleted |
| Windows: Long Shot of maximised Edge with a second tab, Auto scroll via CLI, approved on screen (F-9, P-3) | **FAIL** (fixed, see the retest row) | Result `reached_end`, 5 frames, 1896x3624, no refusal, but rows are missing: ROW 0021-0030 and 0042-0045 (fixture page is contiguous; checked by eye on the stitched image). Repeated twice, same 3624 px. The frames themselves do not overlap (frame 2 shows rows 8-20, frame 3 rows 29-41): the page moved about 1386 px where 699 was expected, and the stitcher accepted a false join of 466 px instead of refusing. Replaying the 7 traced frames offline with `long_shot_stitch` gives the same image, so it can be debugged without the laptop |
| Windows (retest, 2026-11-23, office network): the same Long Shot on the fixed build (commit 93b24358, release, `--no-default-features`), approved on screen | PASS | `reached_end`, 8 frames, 1920x4548, `restored: true`. The trace shows the step of 466 px (expected 699) refused as `NoOverlap`, then "scrolled back by 233 (retry 1)" joined, then six steps of 176 px, then the end. The stitched image (checked by eye) holds ROW 0001 to ROW 0067 once each and in order, with no rows missing. The page still moves about twice the asked step on the first steps (not changed); the retry now recovers. Test output deleted |
| Windows, 2026-11-23: Boards intro card (F-7), 0.3.7 lab build (commit 404ad729), checked by the maintainer on the laptop | PASS | Two screenshots ticked, Make a board: the sheet opens with the Preview intro card at the top right, shown once; it does not cover the close button; closing the sheet removes the card. Test library deleted |
| Windows, 2026-11-24: the strip under the bar takes clicks (macOS K5), before and after the fix | PASS after the fix | Bar window 159,300 882x107; a click 12 px above the window's bottom edge (in the empty strip), with the main window below. Before (commit 698334d8 build): the foreground window after the click was the bar's own window (the click was taken). After (commit 115c3955, release): the foreground window was the main CaptureMann window below (the click went through). A second plain window placed under the strip did not receive the click because the main window lay over it, so this shows pass-through to the next window, not to a different program. Linux X11 and macOS not yet run |
| Linux X11 (GNOME 46), 2026-11-24: the strip under the bar takes clicks (K5) | PASS after a fix | Bar window 867x100 at 969,968, bar 60 high. First build (`3bf16480`): the input region was not changed reliably (the shape requests were sent and the connection closed before the server answered), so the pointer over the strip still reached the bar. After the fix (requests wait for the server's answer): XQueryPointer at the bar gives the bar window, at the strip gives the Desktop Icons window below, four times in a row, both directions. Tested with pointer queries, not a real click on another program |

## Problems found

- **Long Shot on Windows (Edge, maximised, second tab) silently dropped rows**: fixed in `698334d8` (the stitcher joined frames with no real overlap over repeating bars; `ROBUST_BAD_SHARE` 0.5 to 0.35) and retested on 2026-11-23 (PASS row above). Still open: the wheel moves about twice the asked step on the first steps; the run recovers by scrolling back.
- Not run: Wayland (a client cannot do it there), macOS strip (maintainer).
