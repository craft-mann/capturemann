# QA 0.3.4 Long Shot on real sites, Windows pass 8 (PARTIAL)

- **Date:** 2026-11-16, Claude (QA agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %, one monitor.
- **Build:** main `0e73b3c2` (git archive, rebuilt on the laptop with `build-windows.ps1`, release, LTO off, no speaker engine; unsigned), installed silently over the old copy. `capturemann 0.3.4`. SHA-256: zip `DF7B1B4C041EE66DED964661C43EB9E262B104F01855A15805D6C7C4CA9A7D3F` (73,500,413 bytes), setup `647399B21B3A06F1E726ECAC0E3807D7F0A8392C0846E24E8F219C808CADBDC6` (51,531,610 bytes), installed `cm-desktop.exe` `35DF06B23D22F34CC45139FB9BB827954A747FC7BD1B5A2F29C0ACA1457B5081`, `cm-regress.exe` `C954169F630B04CCCC35760AF2F6268B8CCC5F362B31FDFF2454C82A2751CB3D`.
- **Method:** the installed `cm-desktop.exe` with a private library and config (`C:\lab\p6\lib`, `C:\lab\p6\cfg`), started by a scheduled task in the logged-in session; `capturemann capture screenshot --window "<title>" --long --out <png> --json` from the CLI (Auto, vertical, whole window); the Allow dialog clicked through UI Automation. Edge in its own profile (`--user-data-dir=C:\lab\p8\edge`, deleted before each site), `--mute-audio`, a 1100x800 window at (100,40) (1082x791 px captured), no login, nothing played, no forms. The profile shows a round avatar in the toolbar (Edge takes the Windows account picture; no sign-in was done by me). `CM_LONG_SHOT_DEBUG_DIR` was set for runs 2 and 3.
- **Pointer parking (`park_at`, Windows `SetCursorPos` 20 ms after the wheel) did run on Windows:** the cursor sat at 1215,435 (15 px right of the window edge, outside the area) during the runs. A hand test confirmed the routing assumption: wheel at the page centre scrolls Edge, wheel sent then pointer moved 20 ms later scrolls the same amount, wheel with the pointer already outside the window does not scroll it.

## Real sites (3 runs, same build)

Runs: 1 = first pass, 2 = pass with the debug dump, 3 = pass with screenshots every 1.5 s. Column format: end reason, frames, size.

| Site | Run 1 | Run 2 | Run 3 | Read of the image | Verdict |
| --- | --- | --- | --- | --- | --- |
| carchargeos.com | `no_overlap`, 8 frames, 1048x2891 (header 120, footer 8) | `no_overlap`, 1 frame, 1082x791 | `no_overlap`, 1 frame | Run 1: continuous from the browser bar and site header (once) to the "Every system a charging business needs" section; no doubled or missing text; the fixed scroll-to-top button shows once per frame (half circles at the seams, as on macOS). Stops about a fifth into the page | **FAIL** (not repeatable) |
| Wikipedia, Rust | `no_overlap`, 1 frame, 1082x791 | same | same | One frame: browser bar, header, title, infobox top. The page does scroll during the run (screenshots every 1.5 s showed it moving about 600 px), the join of the first pair fails | **FAIL** (3 of 3) |
| YouTube @Google/videos | `no_overlap`, 1 frame | same | same | One frame (channel banner, tabs). The refused frame shows the sticky header growing into a second sticky row (tab bar) | **FAIL** (3 of 3) |
| docs.rs/serde | `no_overlap`, 1 frame | same | same | One frame | **FAIL** (3 of 3) |
| paulgraham.com/greatwork.html | `limit_reached`, 35 frames, 1048x16383 (header 120) | same | same | Read at 4 places over 16383 px with ffmpeg crops: continuous text, no doubled or missing paragraph at the seams, the fixed left nav once (top), browser bar once. The note says "Reached the size limit of 50 megapixels" although the image is 17 MP and stopped at the 16384 px height limit | **PASS** (note wording wrong, defect 4) |

Debug dump of the refused frames (run 2 and 3, carchargeos and YouTube; Wikipedia and docs.rs wrote no refused-frame set): carchargeos `no shift matches (header 120, footer 8, 791 lines); closest: shift 630 with 17 of 17 compared lines differing`. The hero has a gradient background and a translucent header (blurred, tinted differently in the second frame); at a shift of 630 px only 33 px of overlap is left below the header, and no looser mode (up to 32 differing pixels per line, stretches left out, band of columns) matched. YouTube `closest shift 605 (header 147) / 551 (header 204)`: the sticky header differs between the two frames (the tab bar sticks in the second one only), so the header guess moves from frame to frame.

## Fixture cases that failed in pass 7 (rerun; HKCU\Console\%%Startup delegation set to classic conhost for the console cases and removed afterwards)

| Case | Runs | Result |
| --- | --- | --- |
| Fixed left and right sidebars plus a classic scrollbar (`sidebar.html`) | 3 | **FAIL 3 of 3**: `nothing_scrolled` 1 frame (once, before the console keys were set), `no_overlap` 1 frame (twice). Control (`#plain`, same rows): `limit_reached`, 30 frames, 1048x16384 (3 of 3) |
| Classic console host (conhost, `powershell` in a normal window, 300 lines, scrolled to the top by hand) | 4 (two sessions of two) | **2 of 4 pass**: first run after the window was created `nothing_scrolled`, 1 frame, 1182x751 (both sessions); the repeat run on the same window `reached_end`, 12 frames, 1165x6007, header 47, footer 15 (both sessions). Pass 7: 4 of 5 `nothing_scrolled` |
| Windows Terminal, 400 lines (side check; the first fixture run opened the console in Windows Terminal because the delegation keys had been removed) | 2 | `reached_end`: 10 frames 1182x4111 (header 78, footer 40) twice for the "conhost" script while it ran in WT; 13 frames 1182x5791 for the 400-line script |

The remaining pass-7 cases (busy, closed window, HUD Stop, Esc, move, resize) were not rerun: the script hung after the Windows Terminal case (Edge did not start in the fixture profile) and was stopped by hand.

## Defects, ranked

1. **Real pages still end on one frame on Windows (high).** Wikipedia, YouTube and docs.rs 3 of 3, carchargeos 2 of 3 (8 frames once). The page scrolls (hand test and screenshots) and the pointer is parked; the first join fails. The macOS and Linux fixes did not carry over (or the cause is another one here: Edge's smooth wheel, the sticky headers and the translucent header on carchargeos). This is the maintainer's acceptance test and it fails on 4 of 5 sites.
2. **A big first step leaves almost no overlap (medium).** carchargeos: shift 630 of a 663 px usable height; a gradient hero has no lines to match. The first step should be smaller (or retried at half the size) when the join fails with a small overlap.
3. **Fixed side bars with a scrollbar still fail (medium)**, 3 of 3 (1 frame).
4. **Classic console: the first run on a fresh window is `nothing_scrolled` (medium)**; the repeat on the same window works. Something is not ready on the first run (focus, the console not yet the foreground window at the first wheel).
5. **Wrong note (low):** "Reached the size limit of 50 megapixels" for an image that stopped at the 16384 px height limit (1048x16383); the fixture control prints "Reached the longest image CaptureMann can edit", which is the right one.
6. Start toast for a CLI run still says `Started by agent via MCP` (carried over, low).

## UX proposals

- When a run ends on one frame, say what the page did (the first step moved, the next part did not line up) and offer Hand mode.
- Retry the first join with a step of half the size before giving up with `no_overlap`.
- The debug dump should also write a set when the first pair is refused with no accepted frame to compare (Wikipedia and docs.rs wrote none).

## Not run

Hand scroll (refused for agents), the pass-7 cases busy/closed/HUD/Esc/move/resize, a second monitor, mixed DPI, other browsers, Windows 10. The pass-7 report's wheel-routing assumption was checked by hand on Wikipedia only.

## Files

Images and logs stayed on the Mac scratchpad (`p8out/`); downscaled images were read, not stored. Debug dump text: see the quotes above.

## Verdict

PARTIAL
