# QA-0.3.4 editor and library report (PARTIAL)

- **Date:** 2026-11-14
- **Who:** Claude (QA agent, maintainer away)
- **Machine:** Apple Silicon Mac, macOS 26 (Darwin 25.6), one built-in display at 2x
- **Build:** signed dev bundle `target/bundle/CaptureMann-dev.app` built from main `182ceee7` (bundle id `dev.capturemann.app.dev`); sidebar label reads `v0.3.3` because the workspace version is still 0.3.3. Not rebuilt, not modified.
- **Data:** isolated library, config and socket (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET=/tmp/cmqa1.sock`, `CAPTUREMANN_SECRETS=file`). Fixtures were 6 synthetic PNGs drawn with Swift (a fake Settings dialog on a flat background, one with gradient buttons, one with a noisy "photo" strip, a gradient, a wide and a tall banner). The CLI had no daemon binary in the bundle (`cm-daemon` is not in `Contents/MacOS`), so the app instance itself hosted the daemon and the CLI talked to it.
- **Method:** CLI actions with pixel and OCR checks (Apple Vision through `capturemann text`) and `screencapture -l <window id>` of my own window; GUI input by synthetic mouse and keyboard events on my own window only (worked: Accessibility is granted to the host). Everything was looked at once, on one machine; nothing here is "verified" in the sense of `verification/README.md`.
- **Not run:** see the end.

Another agent was driving a second `cm-desktop` (pid other than mine) on the same screen at the same time; one of its clicks changed the filter chip in my Library window once, and my click helper had to refuse once because another window was on top. GUI results are therefore a little less repeatable than usual.

## A. Actions and rendering (CLI/MCP)

| Item | Result | What was seen |
| --- | --- | --- |
| `annotate` retype, flat background (ANN-9.AC1) | PASS | "Save" retyped to "Save all" on a blue button: 0 pixels changed outside `rect`; OCR of the region reads "Save all" and "Cancel", not "Save". Text is a little lighter in weight than the surrounding UI font (expected: system font, hint says so). |
| retype on a vertical gradient (AC2) | PASS | `suggest_blueprint` measured `linear` stops with angle 270; a retype over "Save" on the grey gradient button is seamless (edge pixels 229/203 vs 229/203 original). The editor/CLI measuring path only, I did not draw the box in the editor on a gradient. |
| retype on a photo (AC3, CLI part) | PASS (partly) | `suggest_blueprint` returns `busy: true` for the noisy case; the editor sheet was seen (section B). Cancel-stores-nothing not tried by hand. |
| `move_area` move 120 px (ANN-10.AC1) | PASS | Icon appears at the new place, crop of source and destination identical (0 differing pixels), hole filled `#EDEDED` (matches). |
| `move_area` with `to: null` (AC2) | PASS | Area filled, only the source rect changed. |
| `move_area` overlap, 20 px shift (AC3) | PASS | No ghost of the old position; result looks like cut, fill, paste. |
| `magnify` (Zoom Lens), blur + arrow + lens | PASS | Lens renders sharp, connector drawn, a blur before the lens stays blurred inside it, OCR finds no email text anywhere (AC3 for zoom 3). Only zoom 3 checked, not 2/4/8 and not blur after lens. |
| ANN-8.AC10 (retype under a lens) | PASS | The lens shows "Customer: Bob Smith", the edited pixels. |
| `magnify` limits (AC5) | PASS | zoom 9, source 6x6, border_width 33, a 5th lens: each `invalid_input` naming `ops[0]` / `ops[4]` with a clear message, nothing stored. Loupe 800x800 on a 900x600 image was accepted (no rule against a loupe bigger than the image; below the 32 MP limit). |
| base-edit ordering | PASS | arrow then retype: `invalid_input: ops[1]: retype is a base edit and must come before every other op (ops[0] is not one)`. |
| `suggest_blueprint` | PASS with a defect (D2) | 10 boxes on the flat dialog with `fill`, `color`, `busy`; text previews correct. |
| `blueprint` op, bar and box, keep-list | PASS with defects (D2, D3) | 8 of 10 boxes applied: bars sit on the lines, same height, deterministic (two renders identical). OCR on the render: only the two kept lines plus one junk token ("1ー", read from a bar). On the gradient dialog every line is replaced and OCR reads nothing. |
| revert to original | PASS | `annotate --ops '[]' --replace` restores a render whose pixels equal the source (same pixel digest); `original.png` is read-only and its pixels equal the imported file. (Import re-encodes the PNG, so file bytes differ from my source file; I did not hash `original.png` before the first edit, so "byte identical before and after" was only shown by mtime and pixels.) |
| export a PNG and check pixels/OCR | PASS | via `--out` for every case above. |
| `list_library --kind guide` | PASS | Returns `{captures: [], next_cursor: null}`, no error. No guide capture exists in the library, so the filter with content was not tested (BLOCKED, needs a Click Guide recording). |
| `create_board` Pair | PASS with remarks (D6) | 1888x761 for two 900x600 shots with gap 24, margin 32, white, radius 12, shadow, captions, title, BEFORE/AFTER. Output looked right. |
| `create_board` Pair stacked, Grid (3 columns, 6 images), Strip, Free | PASS | Sizes 564x547, 964x652, 1641x364, 1032x432; free slots overlap as asked. |
| `create_board` errors | PASS | pair with 3 images and free with missing slots: `invalid_input` with the field named; 1 image is refused by the CLI parser. |
| `export_images` resize + JPEG + watermark + pattern | PASS | `{name}-{n}-{w}x{h}` gave `dialog_flat-1-400x267.jpg`; "DRAFT" watermark bottom right is legible with an outline. |
| `export_images` WebP, twice | PASS | second run wrote `dialog_photo (2).webp`, nothing overwritten. |
| `export_images` percent, longest 2000 (no upscaling) | PASS | 250x150 at 50 %; 500x300 kept (not enlarged). |
| `export_images` blueprint | PASS with defect (D3) | Output differs from the library capture (library untouched), two runs are byte-identical in size (37000 bytes). The icon was destroyed (see D3). |
| `export_images` cancel | BLOCKED | The jobs finish too fast on 1 to 3 small files to cancel mid-run; not tried. |
| `recognize_text` | PASS | Returns reading-order text; after a Retype it returns the new text, after a Blueprint only kept lines. |
| delete of a capture with a paused upload (B1) | BLOCKED | Needs a configured destination with a paused job; none in an isolated library and no credentials. |

## B. GUI (my own window, 1180x792 points, dark theme; one light screenshot)

| Item | Result | What was seen |
| --- | --- | --- |
| Library grid, sidebar, chips | PASS with notes | Click Guides item and chip present (count 0), Boards chip (5), Imported chip, board badges `BOARD . FREE/STRIP/GRID/PAIR`. Strip and Free board thumbnails look blurry and crop to a square (D7). |
| Selection bar | PASS | "1 selected" (Make a board greyed), "2 selected" (all enabled), "3 selected": Make a board, Export selected..., Delete, close. It floats over the last card row and covers captions (acceptable, floating). |
| Make a board sheet | FAIL (D1) | Layout columns overlap and clip; see D1 and `QA-0.3.4-shots/make-a-board-sheet.png`. Free, Gap, Margin chip behaviour and the pink snap lines were not reached. |
| Export selected sheet | PASS | Size, Format (PNG, JPEG, WebP (lossless)), Watermark, Blueprint toggle with the expected help text, File names with `{name} {date} {n} {w} {h}` chips and a live example, Save to, disabled "Export 3 images" and "Choose a folder to save to." until a folder is chosen. Layout fits. I did not choose a folder (native dialog), so the actual start from the sheet was not run. |
| 'Upload waiting for sign-in' state | BLOCKED | Cannot seed without a destination and a revoked token. |
| Editor opens with `--open` | PASS | Toolbar with grouped tools (triangle corners), view bar with Copy text, Auto-redact, Blueprint, zoom, Images, image strip with annotation count badges. |
| Retype tool (key Y) | PASS with notes | "New in 0.3.4" pill; recognized lines as dashed boxes; click opens an inline edit with the text selected; typing and Enter made one annotation (the strip badge went to 1, rendered text replaced the line). Notes: the panel shows the red text colour swatch selected although the hint says the colour is measured (D8); the inline editor clips the old text at the box width ("alice.examp"). |
| Zoom Lens tool (key M) | PASS with defect (D4) | Panel with Zoom 2x to 8x, border colours and widths, shape, Connector, Zoom badge, "Lenses on this image 0 of 4". Drawing a lens by mouse was not completed (see D5). |
| Move area tool (key J) | PASS (panel) / unresolved (D5) | Panel and hint text correct. |
| Blueprint command | PASS with notes | Panel replaces the inspector: "10 text lines found . 10 become bars . 0 kept", Bars/Boxes, Auto/One colour, corner radius, Kept lines, Apply Blueprint / Cancel. The tooltip of the Blueprint button overlaps the Cancel button. The "keep" click was not tried. |
| Busy-background sheet | PASS | On the photo capture, clicking the "Save" line showed "This area has an image or a control in it, so the fill may show" with a colour patch, Cancel and Use anyway (light theme screenshot). Cancel and Use anyway not clicked. |
| 'New in 0.3.4' pill | PASS | On Retype, Zoom Lens, Move area and Blueprint panels. |
| Canvas text selection | BLOCKED | Not reached (Select text tool G not tried). |
| Light theme | PARTIAL | One screenshot (`--theme light`, editor): canvas panel stays near-black in light theme with the image at full brightness behind the dim; panels, sheet and buttons render in light colours without visible contrast problems. Library and sheets not looked at in light. |

## C. Problems found, by severity

D1 (high). **Make a board sheet layout is broken at the default window (1180x792) and at 1346x827.** Repro: Library, Select, tick 2 screenshots, Make a board. Expected: the design board S10 (layout tiles on the left, canvas in the middle, a settings column on the right with sliders, toggles and a Background swatch row). Actual (`QA-0.3.4-shots/make-a-board-sheet.png`): the layout list ("Pair, Grid, Strip, Free") is overlapped by the canvas panel from x~615 so the caption "2 screenshots, drag order = list o..." and the reorder arrows rows are cut; the right column is narrower than its content ("Side by side" sticks out, "BEFORE and AFTE" is clipped); Gap and Outer margin are tall vertical stacks of number chips (0, 8, 16, 24, 40 / 0, 16, 32, 48, 64) instead of sliders, and the stack runs past the panel bottom (64 is cut); Radius, Shadow, Background, Captions and Title are not visible and the panel did not scroll with the wheel I could send; the canvas is cut off at the right by the settings column (the board preview is truncated) and shows dark placeholder boxes with names instead of the thumbnails. The BEFORE and AFTER stamps are small rectangles, not the red/green rotated stamps of the design. Suspected: the board sheet view in `apps/desktop/src/ui` (flex widths without `min_w_0`/shrink, `overflow` not scrollable). Because of this, the Free layout (drag, snap, pink guides, handles) and every control behind the visible ones are untested.

D2 (medium). **Text recognition merges "Save" and "Cancel" into one line, and Blueprint then paints one wide bar/fill across both buttons.** Repro: flat dialog with two side-by-side buttons; `capturemann annotate <id> --suggest-blueprint` gives a box `[621,526,247,35]` "SaveCancel" with a linear fill and `busy: true`; applying all boxes (`QA-0.3.4-shots/blueprint-buttons-bleed.png`) draws one light bar across both buttons and the fill spills 8 px outside the right button edge. The same merge made Retype's "Save" box busy on the photo fixture (`[634,533,99,19]` spanning into the gap). Expected: one box per button label. Suspected: the line merging in the recogniser result handling used by `suggest_blueprint` (cm-render ocr layout), or no split on a large horizontal gap.

D3 (medium). **Blueprint in Bulk export (and Apply with no keep-list) erases non-text shapes the recogniser reads as text.** Repro: the fixture's round icon was recognised as the line "O" (box `[759,82,65,63]`); `export-images <photo id> --dest d --blueprint` replaced the icon with a light blue rectangle and left a bar in the Save button. Also tight boxes leave glyph slivers (the left edge of the "A" of the first line, descender dots between rows) visible next to the bars. Expected: bars only on real text lines (a confidence/shape check) and boxes padded so no glyph edge shows. Related: after blueprint, OCR on the render still reads a junk token ("1ー") from a bar.

D4 (medium). **The inspector column clips its content with no scroll.** Retype (below the amber "Not a redaction" note a second card is cut off at y~1335 and runs under the "All changes saved" card), Retype with a line picked (the Background row is visible but the controls below, such as the fit warning and any Cancel/Apply, are cut), Zoom Lens (the hint "Drawn from the original, so it stays sharp..." is cut mid-line, `QA-0.3.4-shots/zoom-lens-panel-clipped.png`). At 1180x792 the inspector height is the issue; a taller window gives the same cut on a smaller scale. Expected: scrolling or a collapsed hint.

D5 (medium, not reproduced reliably). **After a drag with the Move area tool selected, the canvas showed the image shifted up by about 590 px and clipped** (only the bottom 180 px of the picture with the Save/Cancel buttons visible; the Pointer readout said `272, 760` on a 900x600 image); no `move_area` op was created (1 annotation, the earlier retype); the offset stayed through Blueprint and zoom indicators until I closed the app. The drag came from synthetic events (a mouse-down, 12 moves, a mouse-up) after pressing M then J; it may be an artefact of my events (the tool had been switched to Zoom Lens, then Move area, with no click between), but a real pan state that cannot be reset is worth a look. Manual retry: Move area tool, drag a box on a freshly opened capture.

D6 (low). **Board output details.** (a) Pair: the BEFORE and AFTER pills sit on the top-left corner of each image and cover the screenshot's content (in my fixture the title bar text; a stray character of the covered text peeks out to the right of the pill); the design draws them as larger rotated stamps over the image corner. (b) Grid: cells are uniform and images are centred vertically in them, leaving a 50 px band above the first row on top of the margin (`QA-0.3.4-shots/board-pair.png` for the pair). (c) Captions sit very close to the shadow. Looks like choices, not bugs; listed for the maintainer's call.

D7 (low). Library thumbnails of boards (Strip, Free) are blurry and square-cropped (`thumbs/poster.jpg` is small); the Pair board thumbnail loses its right half.

D8 (low). Retype panel: the red colour swatch is highlighted by default while the hint says "colour measured from the box"; after a click on a line the panel shows size 18 (measured) but the swatch row does not show the measured colour (nothing selected). The `{n}` token in `capturemann export-images --help` is printed as a line break (the clap help turned `{n}` into a newline): "File names: {name}, {date}, \n, {w} and {h}". Fix by escaping it in the help text in `crates/cm-cli`. The editor view bar: the keyboard and "..." buttons sit outside the rounded bar at the right edge, half covered by the bar border. The Blueprint button tooltip covers the panel's Cancel button. The sidebar shows `v0.3.3` (the workspace version has not been raised).

## Not tested and why

- Free layout (drag, snap lines, Alt, Shift, resize), Pair/Stacked/Gap/Margin/Background/Radius/Shadow/Captions/Title in the sheet, Finish board, a board with a cropped or blurred source: blocked by D1 (controls not reachable) and by time.
- Canvas text selection, Boards chip filtering, Click Guides sidebar with content, 'Upload waiting for sign-in', delete with a paused upload (B1), cancelling an export mid-run, Export selected start with a real folder: need seeded destinations, a guide recording, a native folder dialog, or longer jobs.
- Zoom Lens drawn by mouse in the editor (loupe visible while the pointer is down, one op after release, undo): not completed; Retype gradient and photo by hand in the editor; Move area move and Delete in the editor (see D5).
- Dark and light theme across all pages; only one light screenshot. No design-board comparison beyond S10 and S7 (the Blueprint panel looks like the design).
- Long Shot, Click Guide capture, recording HUD and bubble: outside this brief.

Screenshots of synthetic fixtures only are stored in `verification/QA-0.3.4-shots/`.
