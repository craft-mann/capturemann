# QA 0.3.4 editor flows: Select text, canvas text selection, Retype, Move area, Blueprint, Zoom Lens, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd`, isolated library/config/socket under `/tmp/cmq2`; the maintainer's app was not running.
- **Fixture:** my 900x600 PNG "cmyk" (text "Customer: Bob Smith", "Email: bob.smith@example.com", ...).

| Item | Result | Evidence |
| --- | --- | --- |
| Select text (G) | PASS | tool selects; a drag over the Email line put "Email: bob.smith@example.com" on the clipboard (checked with pbpaste). The toast is a hidden window, not seen |
| Canvas text selection in a Text box | PARTIAL | Text tool, typed "hello brave world": double-click selects the word (highlight), typing replaces the selection, Shift+Left extends it by characters, Cmd+A selects all. **Triple-click selected only the word (and in a second try the last word plus its space), not all the text** (AC9/AC10 expect "all of the box's text"); synthetic clicks carry click counts 1, 2, 3, so this may be a real defect (E1, low-medium). Copy, cut, paste not run. Remark E2 (low): the default Text colour is white, unreadable on a light picture while typing (`macos-pass2-canvas-text-white.png`) |
| Retype: panel | PASS | no "wider than the box" warning and no "Not a redaction" note; only the blue base-edit note; hint pill "Enter to apply, Esc to cancel" (not the pictured spec wording, seen as "Click a line to retype it..." and "Enter to apply - Esc to cancel") |
| Retype: longer text widens the box | PASS | typed "Customer: Bob Smith and the whole extended family" into the on-canvas box: the box grew from 276 to 673 px, op `retype` rect [60,102,673,26], applied with Enter; the picture shows the long line |
| Retype: busy-background sheet | PASS with defect | sheet "This area has an image or a control in it, so the fill may show" with Cancel / Use anyway. **E3 (medium): the click on "Use anyway" or "Cancel" also reaches the canvas under the sheet**: Use anyway at the Save button position retyped the "Save" line and opened a second sheet; Cancel at a canvas point later created a Spotlight. The sheet's swatch icon is a plain square (`macos-pass2-retype-sheet-clickthrough.png`) |
| Retype: typing into the panel's Text field | FAIL (low-medium) | clicking the panel field and typing did nothing; typing worked only after clicking the on-canvas box (Backspace worked in both). E4: the panel Text field looks editable but does not take typed characters |
| Move area (J) | PASS | drag a source box: tag "filled - #F1F0F0", panel Source 48,194 276 x 44, "Moved to", Fill behind, Snap switch, Remove contents (Del), hint "Hold Shift to keep the move on one row or column", "Not a redaction" note (expected here). Dragging the contents 130 right and 13 down moved the line right and down 12 (snap did not lock a 13 px offset; unclear threshold). With Shift and a drag of +130,+43 the line moved only horizontally (`macos-pass2-move-area-shift-lock.png`) |
| Blueprint | PASS | view-bar command opens the panel: "7 text lines found - 7 become bars - 0 kept", Style Bars/Boxes, Colour Auto/One colour, Corner radius, Kept lines, Apply/Cancel. A click on the Email line kept it ("kept" tag, row "Email: bob.smith@example.", "6 become bars - 1 kept"); Apply made one annotation. Boxes selected looked the same as bars in the preview. Undo not confirmed by eye (two undos changed other items first) |
| Zoom Lens (M) | PASS | after a drag: lens with border, x3 badge and connector; panel Zoom 2x-8x, Border colours and width, Shape Circle/Rounded/Rect, Connector, Zoom badge, Source, "Lenses on this image 1 of 4" (`macos-pass2-zoom-lens.png`). The 0.3.4 first-pass panel clipping was not seen |
| Keep-by-click | PASS | see Blueprint |

Remark E5: the palette keys worked only after focus was on the canvas ("m" did nothing right after a dialog); not a defect by itself.

Not run: Retype colour/weight/align changes, undo/redo of each op by eye, Move area Delete, Remove contents, Blueprint One colour, Zoom Lens on a screenshot with several lenses, Cmd+C/X/V in a text box.

## Verdict

PARTIAL.
