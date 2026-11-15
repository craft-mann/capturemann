# QA 0.3.4 Long Shot (CAP-6) on the final dev bundle, macOS pass 3 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, pass 3)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `5e832011`), isolated library, config and socket under `/tmp/cmq3`; the maintainer's app was not running; their Chrome and Terminal were not driven (my own Chrome profile `--user-data-dir=/tmp/cmq3/chrome` and one new Terminal window of mine, both closed afterwards).
- **FFmpeg:** not used.
- **Fixture:** a 120-paragraph page with a sticky header and random words (so the overlap matcher has unique text), a 12000 px wide page of 40 coloured columns, a Terminal window with 250 numbered rows.
- **How things were seen, measured or not:** every Long Shot window (bar, preview panel, toast, picker overlay) is excluded from screen capture. New in this pass: the toasts, the confirm dialog and the Click Guide bar have an **accessibility tree** (`System Events`, process `cm-desktop`; the earlier passes missed it because it hangs under a "system dialog" window), so their **text and button names were read**, and window sizes were measured with `CGWindowListCopyWindowInfo`. The Long Shot bar, the preview panel and the picker overlay expose **no** accessibility children: their contents (frame counter, live dot, "Keep scrolling", "Esc to stop", "Scroll back up a little") were **NOT SEEN**.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Start from the sidebar tile / rail button (overlay opens, pick a window by clicking it) | PASS (seen as windows, not looked at) | the tile starts the full-screen overlay (`CaptureMann overlay`, layer 1000, share 0); a click on a window starts the run. **The tile has no icon (defect S1, see summary)**; in the rail the Long Shot button is an invisible 44 px hit area |
| Start from the launcher (`Option+Shift+Space`, type "long shot", Return) | PASS | overlay opened |
| Start from the app menu | PASS (entry seen) | menu bar `CaptureMann-dev` lists Long Shot, New Click Guide, Click Guide of a window... |
| Overlay mode, Auto, vertical, Chrome page with sticky header, unique text | PASS | run 2400 x 16384 px, 15 frames, manifest `long_shot_end: limit_reached`; strips read by eye at the top and at the first seam: header once on top, Para 001 to 008 continuous, no repeat, no gap |
| Same, page of repeated identical paragraphs | FAIL | 2 frames, `no_overlap`, 2400 x 2208 px, then the Long Shot stopped (same result as pass 2 run 1 on the repeated page) |
| Same, fresh page window, two further tries | INCONCLUSIVE | one overlay run and two CLI runs answered `nothing_scrolled` ("Nothing scrolled: check that the window can scroll"). The CLI cases ran with CaptureMann's own window on top of the target's centre, so the wheel events probably went to the wrong window; the overlay case is unexplained (see summary LS-flaky) |
| Seam quality | PARTIAL | the stitched image keeps small grey window-corner notches at the left and right edge at every seam (visible at 1/3 scale in `macos-pass3-long-shot-chrome-seam.png`); also seen in pass 2 |
| Horizontal (`capture screenshot --long --horizontal`) | PASS | 6000 x 1520 px, 4 frames, `limit_reached` at the given `--max-px 6000`; Col 001 to 010 continuous, no gap or repeat (`macos-pass3-long-shot-horizontal.png`). The toast wording is "Reached the longest image CaptureMann can edit" even though 6000 was my own limit, not the product limit (low) |
| The limit | PASS | 16384 px (product limit) reached after 15 frames |
| Esc stops and keeps frames (overlay run, Esc one second after the click) | PASS | manifest `stopped_by_user`, 1 to 2 frames kept; toast text read: "Long Shot saved. Stopped with Esc. Everything up to here is kept: 1 frame, 2,400 x 1,698." with Open and Copy (matches S18 wording; the design shows "9 frames" plural, the singular was right here). Esc five seconds after the click came too late: the run had already finished at the limit |
| Nothing scrolled wording | PASS | toast: "Nothing scrolled: check that the window can scroll. 2,400 x 1,698 px · 1 frame" with Open and Copy (honest, not a success wording) |
| Agent / CLI passive HUD | PASS (size only) | after the Confirm dialog ("Allow this action? Capturemann Cli is asking to run "Take screenshots". You set this tool to Ask first...", Cancel / Allow, 520 x 387) a bar of **340 x 56** pt at the bottom centre; contents NOT SEEN |
| HUD size rule S20 (Long Shot bar max 640 and at most display minus 48) | PASS (measured) | overlay-mode bar 570 x 56, 616 x 56 once the counter grew; preview panel 256 to 278 pt wide, as tall as the window |
| Hand scroll, the "Keep scrolling" cue, the live dot, "Scroll back up a little", "Stop and keep", the "stopped where the page jumped" toast | NOT RUN / NOT SEEN | Hand is refused for agents and the overlay and its bar cannot be seen; I did not drive it blind |
| Terminal | INCONCLUSIVE | a Terminal window of 250 rows answered `nothing_scrolled` (it was at the bottom; my Cmd+Home may not have scrolled it) |
| Safari, Preview, Spotlight in Terminal | NOT RUN | |
| The sidebar tile while a Long Shot runs (active state, Click Guide disabled) | NOT RUN | the run is too short to read the accessibility state while it runs |

## Problems found

- **S1** (medium): the Long Shot sidebar tile and its rail button have no icon: `apps/desktop/assets/icons/long-shot.svg` exists but is not listed in `apps/desktop/src/ui/paint.rs` (`ASSETS` has `mouse-pointer-click.svg`, no `long-shot.svg`). Not fixed here (needs a rebuild of the bundle).
- **LS-seam** (low): window-corner notches at every seam.
- **LS-flaky** (low-medium): `nothing_scrolled` when another window covers the centre of the target; no `no_overlap` on a unique-text page but `no_overlap` on a repeated-text page.
