# BRD-1 / BLK-1 manual script: Make a board, Export selected, bulk Delete

Covers what the automated checks cannot: how the sheets look and feel with a mouse. The geometry
(layouts, snapping, resizing, the gap measure, the sizes with a crop), the daemon calls
(`create_board`, `export_images`, the Blueprint step), the single delete question and the CLI/MCP
forms are covered by `cargo test -p cm-render -p cm-daemon -p cm-desktop -p cm-regress` and the
`board.create` and `bulk.export_images` cases of `cm-regress` (standard tier). **None of the
sheets has been looked at on a screen yet; run this script on macOS, Windows and Linux and store a
report in `verification/` (template in its README).**

Nothing is recorded. Use 6 to 8 screenshots of different sizes (take them, or import PNGs), one of
them with a crop (editor: Crop tool), one with a blur, one with visible text, and one uploaded to
Google Drive if an account is connected.

## A. Make a board (BRD-1)

1. Library: tick 2 screenshots. The selection bar offers **Make a board**, **Export selected...**
   and **Delete** (no Copy). Tick 1 or 13: Make a board is disabled.
2. Make a board with 2 pictures. The sheet opens as a Pair: the pictures at one height, the BEFORE
   and AFTER stamps, the size chip (`W x H px`). Switch Stacked, Gap, Margin, Background (all
   five), Radius, Shadow, Captions, Title: the canvas follows each at once.
3. Grid, Strip: reorder with the arrows, remove a row. Pair greys out when the count is not 2.
4. **Free.** Choose Free. Expected: the pictures stay where they were (nothing jumps); each shows
   four white corner handles; the hint says "Drag a picture to place it...".
   - Drag a picture: it follows the pointer, with the open-hand cursor. Near another picture's
     edge or centre, or at the margin line, it snaps (about 8 screen pixels) and a **pink** line
     shows the snap. Beside another picture it snaps to the board's gap, and a pink measure with
     `NN px` shows the distance.
   - Hold **Alt** while dragging: no snapping, no pink lines. Hold **Shift**: the picture moves on
     one axis only.
   - Drag a corner: the picture is resized with its proportions; the opposite corner stays; it never
     gets smaller than a thumbnail, and never goes past the top-left of the board.
   - Let go outside the sheet window: the drag ends (no stuck pink lines).
   - The size chip follows the drag. Finish board: the new board matches the canvas.
5. A source with a crop: its box on the canvas has the cropped proportions straight away (a moment
   after the sheet opens), and Finish makes the board at that size.
6. Finish board: a toast "Board created", the board opens in the editor, the sources are unchanged
   (open one: no new revision). A blurred source is blurred in the board.

## B. Export selected (BLK-1)

1. Tick 3 screenshots, **Export selected...**. Choose Size (longest 800), Format JPEG, a text
   watermark, the pattern `{name}-{n}`, a folder. Start. Progress per file; the files appear with
   the sizes and names expected; nothing is overwritten when run twice (`(2)`).
2. **Blueprint switch (new).** It is enabled, with the text "Replaces every line of text with a
   bar in the files; the screenshots stay as they are". Tick it on a screenshot with visible text:
   the exported files show bars where the text was, and the screenshot in the library is untouched
   (open it in the editor: the text is still there, no new revision). Export the same screenshot
   twice: the two files are identical. On a machine with no text recognizer (Linux without
   `tesseract`): the start is refused with the install hint and no file is written.
3. Cancel at about half: the files already written stay, no half file remains.

## C. Delete selected (one question)

1. Tick 3 screenshots, **Delete**. Expected: **one** dialog, "Delete 3 captures?", the titles
   listed (5 at most, then "and N more"), the button "Delete 3 captures". No dialog per capture.
2. With one selected capture that has a copy on Google Drive: the same dialog gains the unticked
   box "Also delete the copies on Google Drive (permanent)" and the summary names Drive. Delete
   without the box: the copy stays (the toast says nothing about it). Repeat with the box: only the
   capture that has a copy sends the delete to Drive.
3. Cancel: nothing is deleted. After a delete: one toast, "3 captures deleted", the grid refreshes,
   the captures are in the Trash.
4. Delete 1 selected capture: the single-capture dialog as before.
5. Delete a capture with a paused upload together with others (B1): all leave the library.

## Report

Which of the steps above were run, on which system, with which screen scaling (100 %, 200 %), what
looked wrong. Say plainly which steps were not run.
