# QA-0.3.4 summary, macOS, third pass (PARTIAL)

- **Date:** 2026-11-15, Claude (QA agent). Dev bundle of main `5e832011` (`~/Applications/CaptureMann-dev.app`), isolated library, config and socket under `/tmp/cmq3`, Apple M4 Pro, macOS 26.6.2, real mouse and key events. **No source file was changed.** The maintainer's app was not running; their Chrome and Terminal windows were not driven.
- **New method in this pass:** the floating windows (Click Guide bar, picker, toasts, confirm dialogs) are hidden from screen capture but **do have an accessibility tree** (`System Events`, process `cm-desktop`, under a "system dialog" window). It gave button names, toast text and sizes. The Long Shot bar, its preview panel, the picker overlay and the launcher rows have **no** accessibility children, so those contents are still NOT SEEN. I could not look at the Click Guide bar or the Long Shot bar; I measured them.

| # | Item | Report | Result |
| --- | --- | --- | --- |
| 1 | Long Shot (overlay Auto, horizontal, limit, Esc, toasts, agent HUD size) | `QA-0.3.4-long-shot-macos-pass3-2026-11-15-claude.md` | PARTIAL: Chrome unique-text page PASS (16384 px, 15 frames), horizontal PASS, Esc PASS, honest "nothing scrolled"; repeated-text page FAIL (`no_overlap`); several runs flaky; Hand, HUD contents, "Keep scrolling", "Scroll back up a little", Terminal not run |
| 2 | Click Guide (bar states, window closed, picker, editor, export, rich text, titles) | `QA-0.3.4-click-guide-macos-pass3-2026-11-15-claude.md` | PARTIAL: bar height 58 and width at most 720 in all states (measured); editor fit (G1) fixed; exports PASS; **C2 FAIL**; tall picture chip, countdown text, picker purpose copy not seen |
| 3 | Retype, Move area, Blueprint, Zoom Lens, canvas text selection | `QA-0.3.4-touch-up-editor-macos-pass3-2026-11-15-claude.md` | PASS; E1 and E3 fixed; E4 and E2 still there |
| 4 | Library selection, Boards, Bulk export, chips, sorts | `QA-0.3.4-library-select-boards-bulk-macos-pass3-2026-11-15-claude.md` | PASS; L1 to L4 fixed; B1 not run |
| 5, 6, 7, 8 | Sidebar, Agents status shapes, settings, launcher, CMYK and Open With, long path, Vietnamese, local-time titles | `QA-0.3.4-sidebar-agents-settings-import-macos-pass3-2026-11-15-claude.md` | PASS with defect S1; Accessibility not-granted card not run |

## Fixed since pass 2 (checked on this bundle)

E1 (triple-click selects the line), E3 (busy sheet no longer clicks through, both buttons), L1 (selection cleared after Make a board), L2 (bulk export sheet fits, footer visible), L3 (ring on the ticked card, Delete not amber), L4 (no Duration sort on Screenshots), G1 (guide picture fully inside its box), Vietnamese "Click Guides" sidebar label (fits), Agents offline and socket-owner states (ring, "!" badge, reasons).

## Defect list

| ID | Severity | What | Steps | Evidence |
| --- | --- | --- | --- | --- |
| S1 | medium | The **Long Shot** tile (dock) and its rail button have **no icon** (the rail button is invisible but works) | open the app: dock row 2, first tile shows only "Long Shot"; open the editor: rail slot between Mic and Click Guide is empty. Cause: `apps/desktop/assets/icons/long-shot.svg` is not in `ASSETS` in `apps/desktop/src/ui/paint.rs` (`mouse-pointer-click.svg` is) | `macos-pass3-sidebar-long-shot-tile-no-icon.png`; accessibility names the button "Long Shot" |
| C2 | medium | **"Stop and open guide"** in the "window closed" toast saves the guide ("No preview") but leaves the bar open ("Pick a window..."), the sidebar tile "running" and the editor closed | window guide on my fixture window, close the window, press Stop and open guide; then the bar Stop opens the editor | accessibility dumps of the bar after the click; library card "Click Guide 2026-11-15 11.07" "No preview" (pass 2 C1 had a similar stale bar after a CLI stop) |
| N1 | medium, unconfirmed | a full-display **Screenshot** (3022 x 1778 px) appeared in the library during my first window guide, with a full-screen `CaptureMann overlay` window and a screenshot toast, after my second click on the fixture; I pressed no capture shortcut; not reproduced in three later guide runs | first window guide, two clicks on the fixture buttons | library entry "Screenshot 2026-11-15 11.04" (deleted with the isolated library; contained whatever was on screen) |
| LS-flaky | low-medium | Long Shot Auto: `no_overlap` on a page of repeated identical paragraphs; `nothing_scrolled` for one fresh Chrome window and for CLI runs while CaptureMann's own window covered the target's centre (the wheel events probably go to the window on top). The honest wording helps; a person would not know why | Chrome pages above; CLI `capture screenshot --long --window` right after clicking Allow with the app window on top | manifests `long_shot_end`, toast text read by accessibility |
| LS-seam | low | small grey window-corner notches stay at the left and right edges at every seam of a stitched Chrome image | any multi-frame Chrome run | `macos-pass3-long-shot-chrome-seam.png` |
| E4 | low-medium | the Retype panel's Text field takes no typed characters (only the on-canvas box does) | Retype a line, click the panel field, type | seen twice (pass 2 and 3) |
| E2 | low | default Text colour is white | Text tool, type on a light picture | selected swatch is white |
| G-icon | low | in the Click Guides library header "Click Guide of a window..." is an icon-only 40 x 40 button although about 850 pt are free (S16: collapse only when narrower than both) | open Click Guides in a 1180 pt window | accessibility size 40 x 40 beside a 172 pt primary |
| V-esc | low | Esc does not close the guide full-size viewer (X does) | guide editor, click the picture, press Esc | `macos-pass3-guide-fullsize-viewer.png` |
| P-path | low | long library path: the error banner prints the path twice, the cause is at the end; text "connect <path>" is odd | CAPTUREMANN_LIBRARY with 1115 characters | `macos-pass3-library-path-too-long.png` |
| Low, other | low | "Imported" pill does not fade on hover; the CMYK import sheet preview is a generic icon; the horizontal limit toast says "longest image CaptureMann can edit" for a user limit of 6000; the Retype line is drawn a little larger than its neighbours; VI header button "Hướng dẫn theo thao tác mới" differs from the design draft | | |
| Design notes | info | bar widths: 586 (display guide), 688 (following a window), 672 (Vietnamese, following, Undo collapsed to an icon); S16 says the followed-window bar does not change width and S17 says 580 as built; all within 720 and the display limit | | measured |

## Not run (whole)

Long Shot Hand scroll and every HUD content state (counter, live dot, "Keep scrolling", "Esc to stop", "Scroll back up a little", end toasts other than Esc and nothing-scrolled); Click Guide countdown text, "Tall picture" chip, Retake, bar Undo step, picker purpose copy; B1 (delete with a paused upload); Accessibility and Input Monitoring not-granted cards; tile hover tooltips and the disabled tile state; launcher rows by eye; the folder chooser of the bulk export sheet; paste of the rich text into a real app; Zoom Lens pixel fringe; Retype colour, weight and align changes; Safari/Preview; second display.

Pictures: `verification/QA-0.3.4-shots/macos-pass3-*.png` (fixtures only). Window screenshots were checked before keeping; nothing private is in them (the stray display screenshot and a Terminal capture were never stored).
