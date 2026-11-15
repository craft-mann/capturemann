# QA 0.3.4 library filter chips, hover-select and the Make-a-board sheet, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd` (bundle id `dev.capturemann.app.dev`), isolated library/config/socket under `/tmp/cmq2` (`CAPTUREMANN_SECRETS=file`); the maintainer's app was not running. Real mouse and key events (CGEvent, System Events); window pictures cropped to the app window.
- **Fixtures:** my own PNG screenshots, a synthetic recording and audio bundle (`make_fixture`), a copy of the first-pass guide, a local HTML page in a separate Chrome profile, a new Terminal window.

## Results

| Item | Result | Evidence |
| --- | --- | --- |
| Chips per page | PASS | All captures: Imported, Uploaded, Published, Boards, Click Guides. Recordings: Imported, Uploaded, Published (no Boards, no Click Guides). Audio: Imported, Uploaded only. Screenshots: Imported, Uploaded, Boards. Click Guides: no chips. Published: no chips, no sort bar. Pictures `QA-0.3.4-shots/macos-pass2-chips-{all,audio,screenshots,guides}.png` (cropped) |
| Hover tick | PASS | hovering a card shows an empty circle top-left and the "..." menu top-right (`macos-pass2-library-select.png`). On a screenshot card the source pill ("Imported") stays under the tick; I did not see a kind pill hide because screenshot cards have none. The Audio card has an "Audio" pill; hover on it not looked at |
| Ticking starts selecting | PASS | after one tick every card shows its circle; a click on a card body toggles it; bar "1 selected / 2 selected" |
| X in the bar clears | PASS | bar and ticks gone |
| Unticking the last ends selecting | PASS | bar and circles gone |
| No Select/Done button | PASS | none on any page seen |
| Bar actions | PASS with remarks | "Make a board" is greyed with one card and enabled with two; "Export selected..." opens the export sheet; "Delete" opens a confirm (its own window, hidden from capture; Return = Cancel default, Tab then Return deleted the card, library count 8 to 7) |
| Make-a-board sheet | PASS | Pair (BEFORE/AFTER labels, Side by side/Stacked), Grid (Columns Auto/2/3/4), Strip, Free (corner handles; a drag moved the second picture, size line 1888 x 664 px); Finish board created "Board" (kind screenshot, Boards chip showed 1) and opened it in the editor. `macos-pass2-board-sheet-pair.png` |

## Problems

- **L1 (low).** After "Make a board" the two source cards stayed selected, so the next tick made "3 selected". The design does not say; ending the selection after the action would match Delete.
- **L2 (low-medium).** The export-selected sheet is taller than the 792 pt window: its title sits at the top edge and the footer buttons ("Export 1 image") are cut at the bottom (`macos-pass2-export-selected-sheet-overflow.png`).
- **L3 (low).** The selected card shows a small tick but no 2 px ring (design S11 has a ring); the "Delete" word in the bar is amber, the design draws it in the text colour.
- **L4 (low).** The Duration sort is offered on the Screenshots page.
- Make-a-board right panel: the last control row is cut at the bottom edge until scrolled (it scrolls).

## Not run

Export selected to a folder, bulk export result, the upload-waiting-for-sign-in card, hover on an Audio or Guide card, multi-page drag-reorder in the sheet, Finish of Strip/Grid/Stacked.

## Verdict

PARTIAL.
