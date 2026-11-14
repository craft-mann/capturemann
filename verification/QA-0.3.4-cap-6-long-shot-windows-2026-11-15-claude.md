# QA 0.3.4 CAP-6 Long Shot on Windows (PARTIAL)

- **Date:** 2026-11-15 (the run began on the laptop's clock at Nov 14, 22:50 and ended after midnight)
- **Who:** Claude (agent), overnight QA on the dedicated lab laptop; nobody watched
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, one display 1920x1080 at 150 %, logged-in desktop session (`win-run-in-session.ps1`). Edge (InPrivate window), Chrome for Testing 157.0.8103.78 (a portable copy: the normal Chrome installer needs administrator rights), Windows Terminal (the default terminal; `cmd /k` ran in it).
- **Build:** main `26a05100`, release build on the laptop of the Windows package (`packaging\windows\build-windows.ps1`, `--no-default-features`, no speaker engine, LTO off); zip `capturemann-0.3.4-windows-x64.zip` (SHA-256 `838f30f043632eb97d09f53501df2fd9d67dd17c26097fbf4a956fb1d60f7ea6`). The probe examples `long_shot_probe` and `long_shot_stitch` are built from the same tree (the probe gained `--region` in `ac730b17`).
- **Data:** isolated library, config folder, pipe `\\.\pipe\cm-qa-claude`, secrets file; test pages `tests/fixtures/long-shot/long-page.html` (`?screens=`, `bands=1`, `jump=1`, `wide=1`), and one extra wide page. Everything on the laptop is deleted at the end. No picture with a browser toolbar was stored (Edge shows the signed-in profile picture).
- **Method:** the shipped CLI against the daemon hosted by `cm-desktop` (the agent confirmation dialog was answered with Tab and Enter), the probe for Hand scroll (wheel notches, wheel steps and the Down key sent with `SendInput`), OCR of the stitched image through `capturemann text` (Windows OCR).
- **Not seen:** the Long Shot overlay bar, the HUD and the side preview are floating windows that the app excludes from capture. They were not seen. A CLI-started Long Shot shows no HUD (only the overlay flow does), so Esc and Stop could not be exercised (below).

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Auto, Edge, `?screens=5`, 4 runs through the daemon | PASS (1 of 4 first-run failure) | runs 2 to 4: `reached_end`, 7 frames, 1447x4946, OCR rows 1 to 73 once each, in order. The very first run after the fresh start ended after 1 frame with `no_overlap` (size 1482x1097) with the same page; not reproduced in the 3 runs after it, nor in 3 probe runs (7 frames each). Edge was starting up and the confirmation dialog lay over the area, but the cause is not proven. |
| Auto, Chrome, `?screens=5` | PASS | `reached_end`, 7 frames, 1462x5034, a picture of the result shows ROW 0001 to ROW 0073 once, in order (the OCR missed 3 labels at the top of this picture, my own look found them all) |
| Fixed header and footer (`bands=1`), Edge and Chrome | PASS | Edge: header 192 px, footer 80 px, 8 frames, 1455x5090, OCR `HEADER` once, `FOOTER` once, rows 1 to 73; Chrome: 9 frames, 1463x5178, header 287 px, footer 73 px, rows 1 to 73 |
| Page that jumps (`jump=1`), Edge and Chrome (AC3) | PASS | `no_overlap` with the note "Stopped: the next part did not line up"; the frames before the jump kept (Edge 3 frames 1447x2365, rows 1 to 34; Chrome 4 frames 1462x2996); nothing guessed |
| Limit, `--max-px 2000`, Edge (AC4, small) | PASS | `limit_reached`, 3 frames, 1447x2000, note "Reached the longest image CaptureMann can edit" |
| Limit at the real 16,384 px, `?screens=20`, Edge (AC4) | PASS | `limit_reached`, 22 frames, height 16384, same note, image saved |
| Horizontal, `?wide=1` / `wide.html`, target = **the whole browser window**, Edge and Chrome | FAIL (not fixed) | 1 frame, `no_overlap`, 1482x1097, 0.55 s, 3 of 3 plus the offline replay (`long_shot_stitch` with `LS_HORIZONTAL=1` on the dumped frames: the first join is refused). Also without scrollbars (`--hide-scrollbars`) and on a page with no vertical scrollbar. Likely cause (not proven): the browser's tab strip and toolbar are static rows that span the scroll axis for hundreds of pixels, so no column of the frame matches the shifted column of the next frame; the engine only cuts a few px of moving edge (`MAX_LEFT_STRIP` 48, `MAX_RIGHT_STRIP` 64). Vertical captures of the whole window are fine because the toolbar is a header band along the scroll axis. |
| Horizontal with **a region** (the page area only), Edge | PASS | `reached_end`, 7 frames, 7493x800, OCR `COL 01` to `COL 25` once each in order (25 columns x 200 CSS px x 1.5 = 7500) |
| Horizontal with a region, Chrome | PASS (not read by OCR) | `reached_end`, 7 frames, 7481x700; the OCR returned no `COL` label, so the order was not confirmed by text |
| Hand scroll, 12 wheel notches 500 ms apart, Edge and Chrome | PASS | `stopped_by_user`, 13 frames, 1455x2897 / 1463x2897; OCR rows 1 to 42 once each in order |
| Hand scroll, Down key, 40 presses 300 ms apart, Edge and Chrome | PASS | 34 and 29 frames, 1455x3497 / 1463x3497; OCR rows 1 to 51 once each in order |
| Hand scroll, 30 small wheel steps (delta 40) 250 ms apart, Edge and Chrome | FAIL (still open) | 1 frame, `no_overlap`, 1482x1097 in both browsers. Hand scroll takes a frame only when the content stops; a slow continuous scroll never stops (known since the first report, `L1-windows-0.3.4-main-a1bf57dc-feature-cases-...`; not changed, it needs a design decision) |
| Console window with a long output (Windows Terminal running `cmd /k`, 200 numbered lines, scrolled to the top by the wheel, 110x30) | PARTIAL | `no_overlap` after 4 frames, 1721x2495; OCR lines 1 to 86 once each in order, then "Stopped: the next part did not line up"; the 200 lines were not reached (line 87 onward missing, no END marker). Identical text lines that differ only in the number are a hard case for the join. The classic console host (`conhost`) was not tried: this Windows opens `cmd` in Windows Terminal. |
| Hand scroll refused for agents (AC7) | PASS | carried from the earlier report on `a1bf57dc` (`invalid_input`, `needs_user`); not repeated |
| Esc and Stop during a run (AC "Esc or Stop keeps what has been captured") | not run | a Long Shot started from the CLI has no HUD; pressing Esc during a 40-screen run (sent by `SendInput` 9 s after the start) did not end it (`limit_reached`, 22 frames, 16384 px). Esc and the HUD's Stop belong to the overlay flow (launcher, bar, Start), which needs UI driving that was not built. `stopped_by_user` through the Hand loop's own stop was seen (rows above). |
| Overlay bar, HUD and preview | not run | floating windows, not seen; their placement rules are pure functions tested elsewhere |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| AC1 every number once, in order | PASS (Edge, Chrome) | rows above |
| AC2 fixed header and footer once | PASS | `bands=1` runs |
| AC3 a jump stops at the last good join with the note | PASS | `jump=1` runs |
| AC4 the 16,384 px limit with the message, image saved | PASS | 16384 px run |
| AC5 Wayland Hand only | not run (Windows) | |
| AC6 macOS Accessibility | not run (Windows) | |
| AC7 agent: Auto is Confirm, Hand refused | PASS (Auto confirmed on screen each run; Hand refusal carried from the earlier report) | |

## Problems found

1. Horizontal Long Shot of a whole browser window ends after one frame (FAIL, open). Reproduced offline from the dumped frames; the region target works. Suggested direction: detect rows that stay identical across the whole scroll axis as a static band (as the vertical case does for header and footer) instead of only a thin edge strip.
2. Hand scroll with continuous small wheel steps captures nothing (open, known).
3. One first-run Auto capture ended after one frame (intermittent, not reproduced in 6 later runs).
4. Windows Terminal with 200 identical-looking lines stops at line 86 (PARTIAL).
5. `long_shot_probe` had no way to capture an area; `--region` was added (commit `ac730b17`) so the horizontal case could be reproduced without the toolbar. `long_shot_stitch` takes `LS_HORIZONTAL=1`.

## Not run

Esc and Stop during a run, the overlay bar, HUD and side preview, the classic console host, Excel or Calc, Slack, a PDF viewer, a second monitor, other scales than 150 %, Windows 10.

## Verdict

PARTIAL
