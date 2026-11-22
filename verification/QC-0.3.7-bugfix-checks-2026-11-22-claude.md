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
| Windows: Long Shot of maximised Edge with a second tab, Auto scroll via CLI, approved on screen (F-9, P-3) | **FAIL** | Result `reached_end`, 5 frames, 1896x3624, no refusal, but rows are missing: ROW 0021-0030 and 0042-0045 (fixture page is contiguous; checked by eye on the stitched image). Repeated twice, same 3624 px. The frames themselves do not overlap (frame 2 shows rows 8-20, frame 3 rows 29-41): the page moved about 1386 px where 699 was expected, and the stitcher accepted a false join of 466 px instead of refusing. Replaying the 7 traced frames offline with `long_shot_stitch` gives the same image, so it can be debugged without the laptop |

## Problems found

- **Long Shot on Windows (Edge, maximised, second tab) silently drops rows**: see the FAIL row. Not a crash and no warning, so a user would not notice. Cause not yet found (scroll distance larger than expected, and the stitcher accepts a join with no real overlap).
- Not run: Wayland, the strip under the bar (K5).
