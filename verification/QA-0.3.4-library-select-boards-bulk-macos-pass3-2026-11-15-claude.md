# QA 0.3.4 library selection (S15), Boards (BRD-1), Bulk export (BLK-1), chips, macOS pass 3 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, pass 3)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `5e832011`), isolated library `/tmp/cmq3`
- **Fixture:** 5 imported pictures (flat, grad, rgb, photo, CMYK JPEG), then boards and long shots made during the pass.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Hover-select: tick at the card's top-left on hover | PASS | grey round tick at the top-left, "..." menu at the top-right |
| Tick starts selecting; every card shows its tick; bar appears | PASS | bar "1 selected / Make a board / Export selected... / Delete / X" |
| Click on a card toggles it while selecting | PASS | second card ticked by a click on the card |
| Ring on a ticked card | PASS (L3 fixed) | white ring and a check mark, the picture dimmed (`macos-pass3-library-selection-bar.png`) |
| Delete not amber | PASS | the Delete label is white now |
| Make a board needs 2 | PASS | disabled with 1 selected, enabled with 2 and 3 |
| Esc clears the selection | PASS | all ticks and the bar gone |
| X clears | NOT RUN separately | |
| Selection stays after Make a board (L1) | PASS (fixed) | the selection was cleared after "Finish board" |
| Selection bar, Vietnamese | PASS | "Đã chọn 3 / Tạo bảng ảnh / Xuất mục đã chọn… / Xoá" fits, no clipping (with diacritics in the app) |
| Kind pill fading on hover (S15) | NOT SEEN | the "Imported" pill stayed fully visible on hover |
| Make a board sheet: Pair, Grid, Strip, Free | PASS | Pair disabled with 3 sources; Grid 1888 x 1288, Strip 2812 x 664 (numbered 1 2 3, "Arrangement" Side by side / Stacked), Free (drag handles, "Drag a picture to place it..." hint); columns, gap, margin, background, corner radius, shadow, captions, title; "Layout editable until Finish"; Finish board opened the board in the editor (`macos-pass3-board-editor-grid.png`, `...-strip.png`) |
| BEFORE / AFTER (CLI `board --layout pair --before-after`) | PASS | 1888 x 697 board with BEFORE and AFTER labels, annotations baked in (`macos-pass3-board-before-after.png`) |
| Bulk export sheet fits the window (L2) | PASS (fixed) | the sheet is inside the window, the footer ("Choose a folder to save to." / Cancel / Export 3 images) is visible, the body scrolls (`macos-pass3-bulk-export-sheet.png`); also in Vietnamese |
| Size, Format, Watermark, Blueprint, File names in the sheet | PASS | Keep size / Longest side / Width / Height / Percent; PNG / JPEG (Quality 60 to 100) / WebP (lossless); Watermark None / Text (text field, opacity, size, 9-position grid) / Image; Blueprint switch; File names `{name} {date} {n} {w} {h}` with "Example: rgb.png" |
| Cancel | PASS | closed the sheet, selection kept |
| Export to a folder (CLI `export-images --longest 600 --format jpeg --quality 80 --watermark-text DRAFT --name-pattern "{date}-{n}-{name}-{w}x{h}" --blueprint`) | PASS | 3 files `2026-11-15-1-flat-600x400.jpg` etc., 600 x 400, bars instead of text, "DRAFT" mark (`macos-pass3-bulk-export-jpeg-blueprint-watermark.jpg`); sources unchanged. The folder chooser of the sheet (NSOpenPanel) and the real "Export 3 images" button were NOT RUN |
| Filter chips per page | PASS | Screenshots: Imported, Uploaded, Boards; Recordings: Imported, Uploaded, Published; Audio: Imported, Uploaded; Click Guides: none; All captures: Imported, Uploaded, Published, Boards, Click Guides (read by accessibility) |
| No Duration sort on Screenshots (L4) | PASS (fixed) | Screenshots and Click Guides offer Date, Title, Size; Recordings and All captures offer Duration |
| Delete with a paused upload (B1) | NOT RUN | no Drive destination can be set up in the isolated profile |

## Problems found

- Fixed since pass 2: L1, L2, L3, L4.
- Low: the "Imported" source pill does not fade on hover (S15 says the kind pill fades); thumbnails of two very tall long shots in the Screenshots list looked blurred (once, may have been loading).
