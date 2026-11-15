# QA 0.3.4 Long Shot (CAP-6), macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Apple M4 Pro, macOS 26.6.2, one display 1512x982 pt at 2x, load average 5 to 7 during the runs (other agents were building).
- **Build:** the final dev bundle `~/Applications/CaptureMann-dev.app` (main `0bcac6a3`, binaries dated Nov 15 15:50). Isolated library, config and socket under `/tmp/cmq6`; only my own app instance and my own Chrome profile were started and stopped. `CM_LONG_SHOT_DEBUG_DIR` was set on my app so refused frames were dumped.
- **Method:** Chrome window 770x860 pt at 0,40 (my profile), Long Shot started from the CLI (`capture screenshot --long --window <id>`), the on-screen confirm clicked by script; stitched images measured with a small Swift pixel tool (edge columns, run lengths, frame diffs) and viewed. Pictures: `QA-0.3.4-shots/macos-pass6-long-shot-*.png` (fixtures only). Not run from the sidebar tile (the overlay has no accessibility tree).
- **Fixtures:** `side` (fixed red 14 px strip + fixed green 200 px left sidebar, unique text NAVL-01..14, left margin 260 px), `rside` (fixed right sidebar NAVR + blue strip), `both`, `plain` (a blue 30 px left border on every paragraph, no fixed content), `ctl` (plain text, nothing fixed). 140 unique numbered paragraphs each.

## A. LS-1 recheck (fixed left sidebar)

| Check | Result | Evidence |
| --- | --- | --- |
| Width equals window width x scale | PASS (measured) | every image 1540 px wide (770 x 2), `side`, `rside`, `both`, `plain`, `ctl` |
| No grey / desktop smear down the sides | PASS (measured, seen) | `side` (3 frames, 1540 x 2846): left column x=3 is the red strip colour (`ca2b1d`, one level `cb2b1d` after the seam) from y=174 to 2810; right column x=1537 is white from y=174 to 2815. `rside`: x=1537 and x=1520 are the blue strip (`0000d4`) for the whole image. `both`: red at x=3/x=20, blue at x=1520/1537, green at x=300, all constant. Pass 4 had grey at both sides of every frame after the first |
| Red edge strip kept at every seam | PASS (measured) | as above |
| Fixed sidebar repeats slice by slice | PASS (seen) | `side`: NAVL-01..14 then NAVL-10..14, NAVL-09..14 again, as the design says; page text S001 to S016 continuous, no repeat, no gap |
| Page columns exact | PASS (seen, not pixel-diffed) | text starts at the same x in all frames |
| Maintainer's original symptom (left part of a normal wide page lost) | PASS for `plain` and `ctl` (left border and margin intact, white) | `plain` 1540 x 8330 |
| Page with a right sidebar, and both | PASS (edges) | see above |

But **Auto scroll stopped early on every fixture** with the end `no_overlap` and the note "Stopped: the next part did not line up" (page 140 paragraphs, about 23,000 px):

| Page | Frames | Height | Dump reason |
| --- | --- | --- | --- |
| side (3 runs) | 3 | 2846 each time | `refused-last.png` and `refused.png` are pixel-identical (0 differing pixels): the page did not move any more, while a manual wheel scroll of the same page works (reached S104) |
| rside | 4 | 4066 | no shift matches (closest shift 1448, 20 of 60 lines differ) |
| both | 2 | 2236 | "a fixed band changed since the first pair (header 204)": the 14 px strip at the right edge differs between the two frames at y 198 to 251 (the overlay scrollbar fading in over the strip) |
| plain, ctl | 8 (twice) | 8330 | ctl: no shift matches (closest 1448, 54 of 60 lines differ) |

Control: `ctl` has no fixed content at all and also stopped at 8330, so the early stop is **not** caused by the sidebar fix. It is not what pass 4 saw on its 120-paragraph page (16,384 px, 14 frames). Possible causes I could not separate: machine load (load 5 to 7), my fixture (14 random words per paragraph, same layout), or a real stitcher limit. The `both` case shows one concrete cause for pages that have fixed content at the window edge: the macOS overlay scrollbar over a fixed edge strip counts as "a fixed band changed". Recorded as LS-9.

## LS-2, LS-4

| Case | Result | Evidence |
| --- | --- | --- |
| LS-2 window moved by 100 pt during an Auto run | PASS for the result (measured), toast NOT SEEN | JSON `end: "window_changed"`, note "The window was moved or resized", frames kept (8 frames, 1540 x 8330). Not the old "Stopped where the page jumped". The toast `The window was moved or resized. Everything up to there is kept: {n} frames, {w} x {h}.` was not read: a CLI run showed no toast window in my polling |
| LS-4 user limit | PASS for the warning (read), toast NOT SEEN | `--max-px 3000`: 1540 x 3000, `limit_reached`, note and warning "Reached the size limit of 3000 px" (the old wording said "longest image CaptureMann can edit") |
| Confirm dialog reason | defect | dialog says "You set this tool to Ask first in Settings → Agents & CLI" although the fresh config has Take screenshots on Allow (seen in Settings). Long Shot by an agent is always confirmed; the sentence blames a setting the user did not make (LS-10). Dialog height is 365 or 387 px, so a fixed click offset misses the button |

## B. Hand scroll

- `long_shot_hand` (target/debug/examples) **does run**: `--window <title part> --secs N --out file`. It prints `window ... target ...` at once, `stop sent: true` after N seconds, and the result lines only when the stopped run has joined its frames (it can take a few seconds more; a quick `timeout` kill or no `--secs` wait looks like a hang). macOS has no `timeout` command, which may be part of why pass 5 saw "no output". It is a debug build: slow stitching is possible.
- With my scroller (all against the same Chrome page of 140 paragraphs): 30 wheel notches (3 lines, every 700 ms) gave **2 frames, 1540 x 2920** (the page had moved to C044, the image ends at C017); 40 Down-key presses (150 ms) gave **2 frames, 1540 x 1790**; continuous small pixel steps (20 px every 60 ms, 150 events) gave **2 frames, 1540 x 1800**. All ended `stopped_by_user` with `note: null`, no refused-frame dump, so the rest of the scrolling was lost without a message. This is the in-process example (debug build, no HUD), so I do not call it a defect of the app: **Hand scroll through the real HUD is NOT RUN**. Recorded as LS-11 to be repeated with a release build of the example.
- The overlay bar and HUD still have no accessibility children, so the Auto/Hand control could not be clicked; only the bar geometry from `apps/overlay/src/long_bar.rs` is known (centred, 34 px from the bottom).
- "CLI started Long Shot while CaptureMann's window covers the target": NOT RUN again (I placed the app window away from the target on purpose after the earlier "Nothing scrolled").

## Not run

Overlay-started runs and their toasts, sidebar tile states while running, display target, horizontal, 50 MP limit, Terminal, Esc, Cmd+Q during a run, Vietnamese toasts.

## Verdict

PARTIAL. LS-1 (grey smear, lost edge strip) is fixed on all four fixtures as measured. LS-2 and LS-4 wording is as designed in the result data. Auto scroll stopped early on every page I tried, including a page without fixed content; cause not found.
