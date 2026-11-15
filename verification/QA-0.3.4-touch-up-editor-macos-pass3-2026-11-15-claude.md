# QA 0.3.4 editor: Retype, Move area, Blueprint, Zoom Lens, canvas text selection, macOS pass 3 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, pass 3)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `5e832011`), isolated library `/tmp/cmq3`
- **Fixture:** my 900 x 600 PNGs ("flat", "grad", "photo" with a noise block, "rgb", and the CMYK JPEG); real mouse and key events (CGEvent, System Events). Window screenshots of the app (editor is a normal window, so it was looked at).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Retype: panel | PASS | panel "Retype - New in 0.3.4"; Text, Weight, Align, Text colour, Text size, Background (#EDEDED Solid); only the blue "base edit" note; **no "wider than the box" warning and no "Not a redaction" note**; pill "Enter to apply - Esc to cancel" (`macos-pass3-retype-panel.png`) |
| Retype: longer text widens its box | PASS | typing into the on-canvas box widened the box (no warning); the text of the panel field followed |
| Retype: Vietnamese text with stacked marks | PASS | pasted "Tiếng Việt: Nguyễn Thị Ngọc Ánh, ế ỗ ặ ợ" (real diacritics, stacked marks) into the box and applied: all marks and the dot-below tails are drawn whole, nothing cut (`macos-pass3-retype-vietnamese-widened.png`; the board "BEFORE" picture shows it too). Remark: the retyped line is drawn a little larger than the neighbouring original lines (size taken as 24, the originals are 22); low. Typing Vietnamese letters with `System Events keystroke` lost the diacritics (turned into "a") - that is a limit of that tool, not checked as a product defect |
| Retype: typing into the panel's Text field | FAIL (low-medium, E4 still there) | clicking the panel Text field (orange focus border) and typing " Zq" changed nothing, in the field or on the canvas; typing worked only after a click on the on-canvas box |
| Retype: busy-background sheet | PASS | dragging a box over the noise block opened "This area has an image or a control in it, so the fill may show" with **Cancel** and **Use anyway** |
| Busy sheet does not click through (E3) | PASS | "Use anyway" at a point above the recognized "Save" line opened the type-in box for the dragged area and did **not** retype "Save"; "Cancel" returned to the tool with nothing opened (`macos-pass3-retype-busy-sheet.png`, `...-after-cancel-no-clickthrough.png`) |
| Move area (J) | PASS | dragged a source box, then dragged the contents: the line moved, the hole was filled, panel with Snap switch, "Hold Shift..." hint, "Not a redaction" note |
| Blueprint | PASS | view-bar command: "7 text lines found - 7 become bars - 0 kept", Bars / Boxes, Auto / One colour, corner radius, Apply / Cancel; a click on the Email line kept it ("kept" tag, row "Email: bob.smith@example.", "6 become bars - 1 kept") (`macos-pass3-blueprint-keep.png`). Cancel closed the panel. Apply / undo by eye: NOT RUN |
| Zoom Lens (M) | PASS (no fringe seen at normal zoom) | a dragged source made a lens with border, a "x3" badge and a connector; panel: Zoom 2x to 8x, Border colour and width, Shape, Connector, Zoom badge, "Lenses on this image 0 of 4". Pixel-level fringe check NOT RUN (looked at the 2x screenshot only) |
| Keep-by-click | PASS | see Blueprint |
| Select text (G) | NOT RERUN | pass 2 PASS |
| Canvas text selection: double-click | PASS | selects the word |
| Triple-click selects the line | PASS (E1 fixed) | the whole "hello brave world" was highlighted (`macos-pass3-canvas-text-double-click.png` shows the double-click selection; the triple selection was checked by eye in the live screenshot) |
| Shift+Right | PASS | three presses selected "hel" |
| Cmd+A | PASS | whole text highlighted |
| Text tool default colour | FAIL (low, E2 still there) | the swatch selected by default is white; typed text is unreadable on a light picture |
| Undo / redo of each op by eye | NOT RUN | |

## Problems found

- **E4** (low-medium): the Retype panel Text field takes no typed characters.
- **E2** (low): default Text colour is white.
- Fixed since pass 2: **E1** (triple-click), **E3** (busy sheet click-through).
