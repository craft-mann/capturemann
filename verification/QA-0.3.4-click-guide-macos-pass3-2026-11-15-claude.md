# QA 0.3.4 Click Guide (GDE-1 to GDE-4, S16, S17, S19) on the final dev bundle, macOS pass 3 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, pass 3)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `5e832011`), isolated library `/tmp/cmq3`; Input Monitoring was already granted to the bundle.
- **Fixture:** my own AppKit window "QA Fixture Window" (5 buttons, "Contact: jane.doe@example.com"); guides were made on that window, so the pictures contain no private content. One display guide was started and stopped with no clicks (nothing stored).
- **Method note:** the bar, the picker and the toasts cannot be captured, but **they have an accessibility tree** (found in this pass) and a measured window rectangle. So this report states button names, toast text and window sizes that were **read**; it still says plainly that **I could not see the bar**, so colours, the dot, the pulse, "Step n" and the second line were not looked at.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Start from the app ("New Click Guide" button, library header) | PASS | bar opened, 586 x 58 pt at (463,45) for a display guide |
| Start from the sidebar tile | NOT RUN (the button and the launcher entry were used instead; the tile is the same command) | |
| Start from the launcher ("guide" + Return) | PASS | display guide bar; no launcher entry for "Click Guide of a window..." was found (typing "click guide of a window" matched nothing; the app menu has it) |
| "Click Guide of a window..." (library header, app menu), picker | PASS (flow) / copy NOT SEEN | picker dialog read as "Capture a window" ("Chụp một cửa sổ" in Vietnamese) with Search windows, an Apps list, Grid / List, Cancel, **Start guide**; the S16 purpose text ("Follow a window / Only clicks inside this window become steps...") is not in the accessibility tree and was NOT SEEN; a followed window got a frame window (712 x 534, layer 90) |
| Countdown | NOT SEEN | the countdown state ended before my first poll (the bar window existed at once at 688 x 58); text "Starting... 3" not read |
| Bar while recording (S17) | PARTIAL (measured) | **height 58 in every state; width 586 (display guide), 688 (following "QA Fixture Window"), 672 (Vietnamese, following)**; all at most min(720, 1512 - 48). Accessibility buttons: Stop 44 x 44, Pause (84 wide), Note step (106), Undo step (108) with full labels. Design text says "English 580 as built" and, for a followed window, "the bar width does not change": the followed-window bar is 102 pt wider than the display one (688 vs 586); not a limit violation |
| Paused | PASS | the Pause button became **Resume** (96 wide); width stayed 688 |
| Following a window | PASS (name seen only through the toolbar name) | |
| Window closed | PASS | closing the window: toast text read: "QA Fixture Window closed. The Click Guide is paused. Your 3 steps are kept." with buttons **Pick another window**, **Stop and open guide**, Dismiss; the bar swapped to **Stop** + **Pick a window...** only |
| "Stop and open guide" (toast) | FAIL (medium, C2) | the guide was saved to the library (card "Click Guide 2026-11-15 11.07", "No preview") but the **bar stayed open** ("Pick a window..."), the sidebar Click Guide tile stayed in its running state (red border and dot) and the editor did not open; pressing Stop on the bar then opened the editor with 3 steps |
| Vietnamese, long second line, narrow collapse | PARTIAL | Vietnamese bar 672 wide: labels Dừng, Tạm dừng (106 wide), Bước ghi chú (124 wide); **Hoàn tác bước collapsed to a 44 pt icon button** (collapse order Undo first, as S17), nothing wrapped by the accessibility sizes. Collapse of Note and Pause was not forced (no narrower display) |
| Click ring and number badge | PASS | step pictures show a red ring with a badge number (`macos-pass3-guide-editor-fit.png`, PDF picture) |
| Titles from the clicked control | PASS | "Click "Save"", "Click "Open"", "Click "Share"" |
| Note step, Undo (editor), reorder | PASS | `+ Note step` made "New note step" (step 3); the editor Undo removed it; moving step 1 down renumbered ("Open" became 1) |
| Undo step / Note step / Pause by clicking the bar | PASS | a bar Note step click added a note step (3 steps kept) ; Pause/Resume as above; the bar's own Undo step was not clicked |
| Retake | NOT RUN | |
| Editor picture fit (S19, G1) | PASS | the picture is fully inside its box, caption "click at 640, 252 - taken on mouse-down" below it visible (`macos-pass3-guide-editor-fit.png`; pass 2 had the lower edge cut) |
| "Tall picture" chip and the tall-picture rule | NOT RUN | needs a step picture taller than wide; the fixture window is 700 x 492 |
| Full-size viewer | PASS (opens) / low | clicking the picture opens "Step 2 · full size 1,400 x 984" with a close X (`macos-pass3-guide-fullsize-viewer.png`); **Esc does not close it** (low); X closes it |
| Export Markdown / HTML / PDF (CLI `guide export`) | PASS | markdown folder with `guide.md` and `images/step-01.png`, `step-02.png`; one HTML file (144 KB, images inline); a 1 page A4 PDF with both pictures, rings and numbers right after the reorder (`macos-pass3-guide-export-pdf.png`). The export sheet in the app was opened and cancelled (steps notice "Steps 1, 2 still have redaction proposals", Review step 1) |
| Copy as rich text | PASS (pasteboard) | after the button, the pasteboard held `public.html` (142 KB, base64 images) and plain text "Click Guide ... 1. Click "Open" 2. Click "Save""; a paste into a real app was NOT run |
| Default title in local time | PASS | "Click Guide 2026-11-15 11.04" and "...11.07" while the clock read 11:04 and 11:07 (UTC+7) |
| Window-guide button collapse (S16) | FAIL (low) | in the library header the secondary "Click Guide of a window..." is an icon-only 40 x 40 button next to the 172 pt primary although about 850 pt of header width is free; the empty state shows both with labels. S16 says it collapses only when the header is narrower than both |
| Window guide with a CLI stop (C1 of pass 2) | NOT RERUN | but C2 above shows the same kind of stale bar |

## Problems found

- **C2** (medium): "Stop and open guide" in the window-closed toast leaves the bar and the running state; the guide is saved with "No preview". Steps: guide on a window, close the window, press Stop and open guide.
- **N1** (medium, unconfirmed): during the first window guide a full-screen `CaptureMann overlay` window appeared after my second click on the fixture and a normal **display Screenshot** ("Screenshot 2026-11-15 11.04", 3022 x 1778 px) was saved to the library, with a Screenshot toast (Annotate, Copy, Copy text, Save as, Delete). I did not press a capture shortcut; not reproduced in three later guide runs. The file contained whatever was on screen (deleted with the isolated library).
- **G-icon** (low): secondary window-guide button collapsed with room to spare.
- **V-esc** (low): Esc does not close the full-size viewer.
