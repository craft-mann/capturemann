# QA 0.3.4 summary, macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Apple M4 Pro, macOS 26.6.2. Final dev bundle of main `0bcac6a3`, isolated library under `/tmp/cmq6` (removed at the end). Nothing is called "verified".

Reports (`QA-0.3.4-<topic>-macos-pass6-2026-11-15-claude.md`): long-shot, click-guide, published-recordings, vietnamese, editor-library-misc. Pictures: `QA-0.3.4-shots/macos-pass6-*.png`.

| Topic | Result |
| --- | --- |
| LS-1 fixed left sidebar (and right, and both) | PASS (measured): width 1540 = 770 x 2, no grey or desktop smear, red and blue edge strips kept at every seam, sidebar repeats slice by slice |
| LS-2 moved window, LS-4 user-limit wording | PASS in the run result (end `window_changed`, "Reached the size limit of 3000 px"); the toasts were not seen |
| Auto scroll length | **FAIL / unexplained**: stopped early with "did not line up" on all five fixtures (2 to 8 frames of about 14 possible), also on a page with nothing fixed |
| Hand scroll | NOT RUN through the HUD; the example runs but its three scroll patterns gave 2 frames each (debug build, no HUD) |
| Click Guide 100 steps, drag reorder | PASS |
| 500-step cap | holds, but silent; memory about 6 MB per step |
| Published recordings, details, card menu, B1 | PASS with a fixture |
| Vietnamese new surfaces | PASS (seen), one untranslated confirm |
| Retype busy sheet, Select text, Agents offline | PASS (seen) |

## Defects

| Id | Severity | Steps | Evidence |
| --- | --- | --- | --- |
| LS-9 | medium | Auto Long Shot (CLI, window) of a 140-paragraph page, three fixed-content variants and a plain one | ended after 2 to 8 frames, `no_overlap`; side: identical last and refused frames (page stopped moving); both: overlay scrollbar over the fixed 14 px edge strip counted as "a fixed band changed"; plain control also stopped at 8330. Debug dumps in the report. Cause of the plain case not found (load average 5 to 7 on the machine may play a part) |
| CG-9 | medium-low | Click Guide at 500 steps | clicks 501+ are ignored, the bar and state stay "recording", no toast; `guide.error` is published but nothing in the desktop shows it |
| CG-10 | medium | 500-step guide | app RSS 0.85 GB to 3.8 GB while recording (about 6 MB per step), still 2 GB after Stop |
| LS-10 | low-medium | Long Shot or guide started by CLI | confirm dialog says "You set this tool to Ask first in Settings → Agents & CLI" although the setting is Allow |
| VI-1 | low | Vietnamese UI, agent confirm "Start a Click Guide" | body and the button "Start guide" in English |
| PUB-1 | low | warned record details | the warning is shown twice; account label cut by the long privacy pill; header pill does not show the warning |
| SEL-1 | low | Select text tool | panel says "Settings for new annotations." |
| ED-1 | low | editor, pointer outside a 1540 x 8330 image | "Con trỏ / Pointer" shows 11730, 3589 |
| LIB-1 | low | tall Long Shot cards (1540 x 8330) | thumbnail is a blurred crop |
| LS-11 | unknown | `long_shot_hand` example, wheel, Down key and small steps | 2 frames each, rest of the scroll lost, `note: null`; repeat with a release build |
| CG-1 to CG-8, LS-3 | as in passes 4 and 5 | not rechecked | |

## UX proposals

| Proposal | Why | Suggested fix |
| --- | --- | --- |
| Show a toast "Guide full: 500 steps. Stop to keep them" and stop recording | silent cap | listen to `guide.error` |
| Free the step frames after each step is written | 6 MB per step | write PNG and drop the frame |
| Needs attention and Published chips both look selected | confusing | show Needs attention as a sub-filter or deselect Published |
| Agents & CLI offline: dim or annotate the client config snippet | it still shows the unusable socket path | one line "will not work until fixed" |
| Select text: show the dragged rectangle briefly and say "Copied N words" with a Copy again button | no on-canvas feedback | per ANN-5 plus a visible marquee |
| Retype on a busy background: say that only part of the line was found | box found "Hello Bus" | warn on partial boxes or widen the box |
| Hand scroll HUD and overlay bar: expose accessibility labels | QA and screen-reader users cannot drive them | add roles and names |
| Long Shot Auto: tell what stopped a run in plain words per cause (scrollbar over a fixed strip, page stopped moving) | "did not line up" is the same text for four different causes | map the refusal reason to a hint |

## Not run

Hand scroll through the real HUD, overlay-started Long Shot and its toasts (all of them), sidebar tile active and disabled states, Click Guide bar from the tile, countdown, retake, rich-text note paste, Vietnamese bar and toasts, Windows items, a release build of the example.

## Cleanup

My app instances, the fixture window, my Chrome profile process and temp files were stopped and removed; `/tmp/cmq6` deleted. One fixture folder "Delete me B1.cmrec" remains in the user's Trash (from the B1 delete test; I was not allowed to remove it). The clipboard holds fixture text. Other Chrome windows of mine were closed with the process.
