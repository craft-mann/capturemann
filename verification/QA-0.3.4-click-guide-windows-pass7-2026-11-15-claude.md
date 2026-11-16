# QA 0.3.4 Click Guide on Windows, pass 7 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %, 31.7 GB. **Build:** final build from main `011b5000` (`capturemann 0.3.4`, `cm-desktop.exe` SHA-256 `46211DC2723E2B5E0C05B8BE132C24285DDC31D0BF5F82E52241A9C4635187EE`).
- **Method:** as in pass 6 (private library, scheduled task in the logged-in session, real `mouse_event` clicks on the small WinForms window `QA Target` with buttons Alpha, Beta, Gamma, UI Automation for names). The guide was started with a real click on the sidebar tile.

| # | Case | Result | Evidence |
| --- | --- | --- | --- |
| 1 | 100 clicks about 100 ms apart (burst took 12 s) | PASS (memory), see note | `cm-desktop.exe` working set 72 MB -> 265 MB at the peak -> 94 MB after stop (pass 6: 109 -> 828 MB). 9 to 12 steps were saved; the rest were dropped with the toast `A click was not saved: CaptureMann is still saving the earlier clicks`. `guide stop` took 7 s (pass 6: minutes); the guide was saved with 18 steps |
| 2 | 520 clicks about 100 ms apart (burst took 64 s) | PASS (memory, stop time) | working set 96 MB at the start, max 325 MB, 96 MB after stop; 49 steps when the burst ended, 58 saved; `guide stop` 10 s (pass 6: 11 minutes, 3.5 GB). The queue is bounded; the price is that a fast burst loses most clicks (about 0.8 steps per second are written on this laptop) |
| 3 | The 500-step cap at a sustainable pace (500 clicks, 1.5 s apart, 779 s) | PASS | steps 50, 100, ... 450 counted one for one; at 450 the toast `50 steps left. Stop and start a new guide to keep going.`; at 500 the guide stopped by itself with the toast `The guide reached 500 steps and was saved.. Start a new guide to keep going.` (a doubled full stop); the saved guide holds exactly **500** steps in `guide.json` and 500 step bundles; working set stayed at 50 to 100 MB; a later `guide stop` answered `invalid_input` ("guide_id: is required when no guide is recording"), as the guide was already stopped |
| 4 | Same crop rectangle in every step | NOT VERIFIABLE on this screen | the guide crop is the bounding box of all clicks padded by the default focus size (1200x800 logical x 1.5 = 1800x1200 px here), which is more than 85 % of a 1920x1080 frame, so the crop is off (`none` in every step bundle, 4 steps and 4 steps with a wider spread). The "identical rects" rule needs a larger screen or a smaller scale; the unit tests (`gde_1_every_step_has_the_same_crop...`) cover it. Not claimed |
| 5 | Vietnamese UI name `Hướng dẫn theo thao tác` | PASS | after switching to `Tiếng Việt`, UI Automation names: the sidebar row button `Hướng dẫn theo thao tác` and `Hướng dẫn theo thao tác, 2` (with the count), the tab item `Hướng dẫn theo thao tác, 2`, the library cards `Click Guide ..., Hướng dẫn theo thao tác, Hôm nay 22:46` and their action buttons. One name everywhere that was inspected. The editor header and the dock tile were not inspected in Vietnamese |
| 6 | Agent confirmation in Vietnamese, Start button | NOT RUN | |

## Defects and notes

1. **A fast burst loses most clicks (low to medium, by design of the fix).** The queue holds 8 frames; at about 10 clicks per second only 9 to 12 steps per 100 clicks were kept, and the toast `A click was not saved...` appears repeatedly. Memory and Stop are fixed. A person clicking quickly (menus, forms) would lose steps; consider saving the frame at a lower cost (smaller PNG compression, JPEG) so that 3 to 5 clicks per second are kept.
2. **Doubled full stop** in the cap toast: `...was saved.. Start a new guide to keep going.` (low).
3. **Crop identity not checked** on this display (see case 4).

## Not run

Retake, Tall picture chip, viewer, Esc, countdown look, red Stop (the bar is excluded from capture), agent-started guide bar (CG-2, known; pass 6).

## Verdict

PARTIAL
