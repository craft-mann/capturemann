# UX and visual-fidelity audit: Boards (BRD-1) and Bulk export (BLK-1), 0.3.4

Date: Nov 15, 2026. Method: read-only. Code (`apps/desktop/src/board_flow.rs`, `ui/board_sheet.rs`, `bulk_flow.rs`, `ui/bulk_sheet.rs`, `ui/library_view.rs`, `crates/cm-render/src/compose_board.rs`, `compose_bulk.rs`, `crates/cm-daemon/src/export_images.rs`, `crates/cm-cli/src/commands/board.rs`, `crates/cm-library/src/board.rs`), the spec (`specs/features/20-boards-and-bulk.md`), the design pack in `specs/capturemann-0.3.4-design/` (S10 `source/R034Board.dc.html`, S11, S12, S20; inline styles read for numbers), the QA reports (`verification/QA-0.3.4-*boards*`, `*bulk*`, `*pass3*`) and the stored PNGs in `verification/QA-0.3.4-shots/`. I did not run the app, cargo or anything that touches the screen. "Not determined" means it needs a live run.

Images I looked at (what I saw): `board-pair-before-after.png` and `macos-pass3-board-before-after.png` (real renders), `macos-pass2-board-sheet-pair.png`, `macos-pass2-vi-board-sheet.png`, `macos-pass3-board-editor-grid.png`, `...-strip.png`, `board-sheet-after-free.png` (the sheet), `macos-pass3-bulk-export-sheet.png`, and the design renders `screens/png/S10-boards.png`, `S12-bulk-export.png`. I did not open the S11 PNG or the S15 board.

---

## A. End-user flows (as built)

| # | Flow | How it works today | Source |
|---|---|---|---|
| A1 | Create from the library | Tick cards, floating bar, **Make a board** (enabled only for 2 to 12 screenshots and nothing else selected). The selection clears and a modal sheet "New board" opens. | `library_view.rs:469-492`, `:2376` |
| A2 | Create from the editor | **Not built.** No entry in the editor. | only caller of `board_sheet::open` is the library |
| A3 | Empty board, drag screenshots in | **Not built**, although BRD-1 "Start" promises it. `BoardSheet::add_source` exists but nothing calls it; the sheet has no drop tile (S10 has one). A forgotten picture means cancel and reselect. | `board_sheet.rs` |
| A4 | CLI / MCP | `capturemann board <ids> --layout pair/grid/strip/free [--stacked --columns --slot x,y,w,h --gap --margin --radius --background --shadow --before-after --caption --board-title --name]`; MCP `create_board`. Returns id, size, sources, inline PNG up to 8 MB. | `cm-cli/commands/board.rs`, `export_images.rs:220` |
| A5 | Layout per count | 2 sources start as Pair (labels on). 3 to 12 start as Grid (columns auto: 2 up to 4 pictures, else 3; engine allows 1 to 6, sheet offers Auto/2/3/4). Pair is greyed unless exactly 2. Removing a source from a pair falls back to Grid. Strip and Free for any count. | `board_flow.rs` `new`, `layout_enabled`, `remove_source` |
| A6 | Reorder / swap | Up, Down, X buttons per row. **No drag**, though the hint says "drag order = list order" (EN) and "kéo để đổi thứ tự" (VI). In Pair, swapping rows swaps BEFORE and AFTER. | `board_sheet.rs:592-640` |
| A7 | Free layout | Starts from the current layout's rectangles; drag to move, corner drag to resize (proportion kept), snap to margin/edges/centres/gap (8 screen px), Alt = no snap, Shift = lock axis, pink guides and gap measure. Pointer only; no numeric entry, no remove. Not determined: whether the UI calls `raise()`. | `board_flow.rs`, `board_sheet.rs:231-429` |
| A8 | BEFORE / AFTER | Pair only. Words come from the UI language at creation (`tr!("BEFORE")` uppercased: TRƯỚC / SAU) and are baked into the pixels. CLI has no word option (JSON `label_words` only). | `board_flow.rs:630` |
| A9 | Style | Gap 0/8/16/24/40, margin 0/16/32/48/64, radius 0/8/16/24, shadow, background Transparent/White/Light grey/Dark/Tint (hex field), title, captions. Defaults: gap 24, margin 32, radius 0, shadow off, white, captions off. Engine accepts gap 0-200, margin 0-400, radius 0-100; the sheet cannot reach them. | `board_sheet.rs:731-826`, `BoardOptions` |
| A10 | Captions | On = each source's library title under its picture. No typing per image in the sheet (CLI `--caption` can). | `board_flow.rs:619` |
| A11 | Annotated sources | Flattened by `render_annotations` on `original.png` with the capture's ops (annotations, redactions, crop) before layout; no unblurred pixels (AC2). The sheet preview uses thumbnails. | `export_images.rs:60-100` |
| A12 | Source deleted later | Board is a standalone screenshot bundle; `manifest.board.sources[]` holds ids, titles, sizes only. Nothing breaks (AC3 passed on Windows by SHA-256). | `cm-library/board.rs` |
| A13 | Undo | Creation is a new capture; undo = delete it. The board then opens in the editor as an ordinary screenshot. No "Undo" on the "Board created" toast. | `board_sheet.rs:1143-1200` |
| A14 | Edit after creation | **Layout is not editable.** The manifest keeps layout and options but no UI reopens them, so "Layout editable until Finish" is true only inside the sheet. Afterwards: annotate, crop, redact the flat picture. To change gap, order, a label or a caption: delete and redo from the library. Refresh from sources is out of scope by spec. | no reopen code |
| A15 | Export / share / copy | As any screenshot. Transparent background: PNG keeps alpha; JPEG goes on white (the sheet hint says so). | `compose_board.rs` |
| A16 | Library | Badge `BOARD . PAIR`, chip **Boards**. Thumbnail is a 1280 px poster shown Contain in the 16:10 frame, so a pair (about 2.7:1) or strip (4:1) is letterboxed in a thin band. | `library_view.rs:1002-1075` |
| A17 | Editor strip | A board is a plain picture there: no marker, no "made from N screenshots". (`...board-editor-strip.png` shows the sheet in Strip layout, not the 0.3.3 image strip.) | grep |

## B. Completeness, clarity, states

| Flow | Verdict | Notes |
|---|---|---|
| A1 create from selection | OK | Disabled for 1 or 13+ or mixed kinds; I found no text saying why (tooltip not determined). |
| A3 empty board | MISSING | Spec promise not met. |
| A5 layout choice | GAP | Text-only tiles 44 px, no diagrams; Pair greyed with no reason. |
| A6 reorder | GAP | Arrows only; hint promises drag. 12 rows by arrow is slow. |
| A7 Free | GAP | Pointer only, no keyboard nudge, no numbers, no remove or reset. The board grows when a picture is dragged outward (chip follows). Spec says "not seen on a screen". |
| A8 labels | GAP | Words not editable; the design's hint "Written in your app language" is missing; a Vietnamese app bakes TRƯỚC / SAU into an English audience's picture. |
| A9 style | GAP | Presets instead of sliders; no 12 px radius (the design value). Fixed pixel presets mean the same "24" is a hairline on a 3000 px board (see D P1-3). |
| Inspector | GAP | At 1180x792 the inspector shows Arrangement to Shadow and cuts **Captions and Title** below the fold (seen in the pair, grid and strip shots; in Free the bottom note is also cut). It scrolls (`overflow_y_scroll`) but no scrollbar or fade cue. |
| Errors, busy | OK/GAP | Plan errors show as warning text in the canvas; Finish errors above the footer; button reads "Making the board...". No progress for big boards (decode, Lanczos, PNG encode in one blocking call); responsiveness not determined. Esc closes **without confirmation** and loses the setup. |
| Loading | GAP | Cells show the title on a raised box until thumbnails load; a cropped source corrects its size after opening (`refine_sizes`), so the canvas and size chip can jump once. |
| Keyboard, a11y | GAP | Dialog role, aria label, aria names on the row buttons; Esc closes. No Enter to finish, tab order and focus rings not verified, canvas has no description, swatches show their name only when selected, Free is pointer only. |
| Vietnamese | OK/GAP | The VI sheet fits at 1180 (seen). S20 says the layout column is 200 px and tile labels wrap to 2 lines; built is 220 px (180 narrow). VI uses "Bảng ảnh" for Board while S20's draft says "board" ("Tạo board"): choose one. |
| 2 vs 12 images | GAP | Grid cells are min width x min height of all sources; mixed sizes leave white bands. A 12-picture horizontal Strip of 1900 px frames is 22800 px wide and is **silently scaled down** to 16384 (`plan.scale`, `MAX_EDGE`); only the chip changes. |
| Extreme ratios | GAP | Pair/Strip scale to the smallest height (side by side) or width (stacked): 4000x300 next to 300x4000 gives 4000x300 and a 22x300 sliver. In Grid both shrink to a 300x300 cell. No warning. |
| Very different sizes | GAP | The larger source loses resolution (QA: two roughly 900 px screenshots gave 1888x697). Matches AC1 ("same height") but the design shows 2400x1200 for 2560x1600 and 2880x1800 sources. Which the maintainer meant is not determined. |
| HiDPI | not determined | No Retina-specific code found. |
| Dark / light | not determined | Sheet uses theme tokens; board backgrounds are fixed colours (correct). No light-theme image stored. |
| Bulk sheet | GAP | No thumbnail stack; presets not sliders; at 1180x792 the footer shows "Choose a folder to save to." and the body scrolls; no default or remembered folder. |
| Bulk progress / summary | OK | Progress with Cancel and "Cancel keeps the files already written."; summary with Show in folder, Retry skipped, Done; reasons translated. No one ran this UI: all QA used the CLI. |

---

## C. Visual fidelity

### C1. Boards (design S10)

Design values: a full page with breadcrumb "Screenshots", H 18/600 "New board", the pill, Cancel and **Finish board** (check icon). Left column **200 px**, canvas card radius 18 with an 18 px dot grid, inspector **300 px** card radius 14, padding 16, titled "Board" with a "New in 0.3.4" pill; columns gap 16.

| Item | Design | Built | Verdict |
|---|---|---|---|
| Surface | page with header | modal sheet max 1180x820, radius 18, header 64, footer 66, X button | Deviation; acceptable for a modal, not what S10 drew |
| Layout tiles | 2x2, gap 8, padding 8, radius 10, 46 px diagram + 12 px label, selected = 1 px `#EDEDEF` on `#1B1B20` | 2x2 buttons 44 px, text only | **Diagrams missing** |
| Source rows | drag handle, 56x34 thumb radius 7, title 12.5, mono "2560 x 1600" 10.5 | 40 px row, 36x26 thumb radius 4, title, up/down/X; no sizes, no handle | Deviation |
| Drop tile | dashed 1.5 px `#34343C`, "Drag screenshots here / 2 to 12" | none | **Missing** |
| Canvas chrome | dot grid `#1E1E24`, dashed 1 px blue outline offset 6 around the board, mono size chip 24 px high at left | plain surface card, no dots, no outline, chip centred | Deviation |
| Preview content | bg `#ECECEF`, radius 6, padding 30/40, title 19 px 700 centred, gap 20, images 186 px high radius 12, shadow `0 12px 30px rgba(0,0,0,.18)`, captions 13 px `#3F3F46` centred (gap 10), stamps on the image at 14,14 | no shadow, radius clamped `min(12.)`, **captions not drawn**, title 9 to 22 px semibold left, labels as outlined stamps in the band | **Preview is not WYSIWYG** |
| Inspector | "Board" header and pill; Arrangement seg; labels toggle + hint; Gap and Margin **sliders** with mono "24 px"/"40 px"; swatches 22 px; radius slider 12; Shadow, "Captions under each image"; Title; copy note | no header; seg; toggle; **presets**; 30 px swatches; radius presets; Shadow, Captions; Title below fold; note under the canvas | Deviation |
| Defaults drawn | gap 24, margin 40, radius 12, shadow ON, captions ON, bg `#ECECEF` | gap 24, margin 32, radius 0, shadow OFF, captions OFF, white | **The board in the design is not what a user gets** |
| Light grey / dark | `#ECECEF`, `#18181B` | `#ECEDF0`, `#18191C` | Negligible |
| Pink guides | `#F472B6`, dashed, tag mono 11 radius 999 | `#F472B6`, solid, tag 10 px radius 4 | Close |
| Finish | check icon | no icon | Minor |

**The picture itself.** From `compose_board.rs`:
- Margin and gap are native board pixels. For two roughly 900 px screenshots (board about 1900 px) margin 32 is 1.7 % and gap 24 is 1.3 %. The design's proportions (margin 40/720 = 5.6 %, gap 24/720 = 3.3 %) are about three times roomier. In `board-pair-before-after.png` the pictures sit about 24 px apart and about 28 px from the edge: tight.
- Default radius 0, no shadow, white background: a white-on-white board has no visible edge. With shadow on (as in the stored render) the edge is clear. Shadow is 8 stacked rings at alpha 9/255; the design is one blur. The shadow of picture 2 is painted after picture 1, so where spread (`max(unit*0.5, 6)`, up to 40 px) exceeds the gap (24) it darkens picture 1's edge. From code, not seen.
- Title: `unit*1.6` (about 48 px), regular weight, **left-aligned at the margin** (seen: "Pair title" top left). Design: bold, centred.
- Captions: `unit` px, ink `#1B1C1F`, **left-aligned** under each image (seen: "Before text", "After text"). Design: centred, `#3F3F46`.
- Labels (Pair): filled rectangle in a band above the picture, left aligned, square corners, white text `unit*0.9` (about 27 px), BEFORE `#555B66` grey, AFTER `#1F8A4C` green. Design: outlined stamp, 3 px border, radius 7, white 92 % fill, 15 px 800 with .06em tracking, rotated -4 degrees, on the picture, red `#DC2626` and green `#16A34A`. The band is a recorded maintainer decision of Nov 14 ("the label never covers the screenshot"); I think it is right for legibility, but the colours and the stamp look are not the design's and BEFORE is grey, not red.
- **The sheet preview and the output disagree on the labels.** The preview (`board_sheet.rs:315-326`) draws the outlined red/green stamp; the renderer draws grey/green boxes. Seen: outlined red BEFORE / green AFTER in `macos-pass2-board-sheet-pair.png`; grey BEFORE / green AFTER in `macos-pass3-board-before-after.png`.
- Preview labels use a fixed 13 px font while the output scales with content; in the VI preview of a tall 3135x3099 board the stamps look large and crowd the top.
- Strip numbers: dark circle alpha 235, 1.7 unit, white digit over the image corner (S10 has no strip frame design; not determined).

**Pair board verdict.** The renders are clean and correct (equal heights, aligned, crisp, legible, labels do not hide content, annotations and the zoom lens carried over). They do **not** follow the design's look: tighter spacing, no default shadow, radius or grey background, left-aligned regular title and captions, grey/green boxes in a band instead of red/green stamps. It reads as a competent plain collage, not the polished board of S10.

### C2. Library card (S11) and Bulk export (S12)

| Item | Design | Built | Verdict |
|---|---|---|---|
| Board badge | "Board . Pair", Geist 11 px sentence case, h 22, radius 6, 75 % black, left 8 top 8 | `BOARD . PAIR`, mono, uppercase, h 24, radius 7, left/top 10 (`library_view.rs:1015-1031`) | Deviation; consistent with REC/SHOT badges (ask design) |
| Board thumbnail | not determined | Contain, letterboxed | Not determined against the S11 PNG |
| Sheet width | 740 | `SHEET_W = 740` | Match |
| Header | stack of 4 thumbnails, "Export 23 screenshots", "Rendered with annotations and blurs . originals stay as they are" | icon tile, title, 2-line text | Deviation: no thumbnails |
| Size | Longest/Width/Height/Percent, value, "Allow larger" toggle | adds "Keep size"; same otherwise | Close |
| Format | segments; Quality **slider**, 85 | segments; presets 60/70/80/90/100, **60 selected** in the stored shot | Deviation; default 60 is the lowest quality |
| Watermark | None/Text/Image, text, Opacity slider 60 %, Size slider 4 %, 3x3 grid beside | presets 20-100 % and 10-60 %, default size **25 %**, grid below, no "Position" label | Deviation; 25 % of the width is large |
| Blueprint | toggle row with explanation | present | Match (not seen live) |
| File names, Save to | pattern, keycaps, example, path + Choose... | present in code; below the fold in the shot; "No folder chosen", Export disabled until chosen | Not determined visually |
| Footer | Cancel, "Export 23 images" with icon | Cancel, "Export 3 images" disabled 45 % | Match |
| Progress, summary | toast cards bottom right | one panel in the sheet | Different surface; whether the sheet can close mid-run is not determined |

---

## D. Proposals

### P1

1. **Preview must equal the picture.** Problem: the canvas draws outlined red/green stamps, no shadow, no captions, radius capped at 12; the output has grey/green boxes, shadow, captions, any radius. Evidence: `board_sheet.rs:273, 315-326` and the shots. Change: render the preview with `compose_board::render_board` on thumbnail-size sources (debounced, off-thread), or share the label/shadow/caption drawing. Effort M.
2. **One label look, in the design's colours.** Problem: band labels are grey/green filled boxes; design is a red/green stamp. Change: keep the band, draw the design stamp in it (3 px scaled border `#DC2626`/`#16A34A`, white 92 % fill, radius about 0.45 unit, weight 800, .06em tracking, optional -4 degrees) in both preview and renderer; or ask design for a "stamp in a band" variant. Effort M, design request.
3. **Spacing that scales, and a design-like default.** Problem: 24/32 px fixed; defaults radius 0, no shadow, white. Change: define gap, margin and radius as percent of the cell height (or an "Auto" style: gap 3 %, margin 5 %, radius 1.3 %, shadow on, light grey `#ECECEF`), make Auto the sheet default; keep the numeric CLI defaults compatible. Effort M.
4. **Title and Captions reachable.** Problem: below the fold at 1180x792. Change: put Title right after Arrangement, add a scrollbar or fade, tighten preset rows. Effort S.
5. **Copy that promises missing features.** Problem: "drag order = list order", "Layout editable until Finish", the spec's empty board. Change now: "Use the arrows to change the order" (EN, VI) and pill "Preview"; then build the drop tile and drag handle (S10). Effort S copy, M to L the tile (needs a library picker).
6. **Warn on extreme ratios and downscaling.** Problem: min-height scaling gives slivers and lost resolution; boards over 16384 px shrink silently. Change: one line under the size chip ("Pictures are scaled to the shortest: 1440 px high"; "Scaled down to fit 16384 px"), suggest Free or Grid when ratios differ by more than 4:1; optional "fit to the tallest". Effort S for the notes, M for the option.

### P2

7. Layout tiles with 46 px diagrams, tooltip on disabled Pair ("Pair needs exactly 2 screenshots"). S.
8. Source rows: 56x34 thumbnail, mono sizes, drag handle and Alt+Up/Down. M.
9. Free: keyboard nudge, numeric X/Y/W/H, remove, reset; confirm raise-on-click is wired. M.
10. Captions editable per image; title centred and bold with an alignment choice; captions centred like S10. M.
11. Label words as editable fields (default translated), the hint "Written in your app language", CLI `--label-before/--label-after`. S.
12. "Edit board" while the sources still exist, otherwise state "The board is final". The manifest already holds layout, options, sources. L, or S for the statement.
13. Confirm on Esc/X when settings changed; "Undo" action on the "Board created" toast (deletes it). S.
14. Card badge: sentence case "Board . Pair", 11 px Geist, 22 px, radius 6, 8/8, or ask design to harmonise the other badges; letterboxed thumbnail on a neutral tile. S, design request.
15. Bulk: sliders for quality/opacity/size, default quality 85, default watermark size 4 to 8 %, remembered folder, thumbnail stack, progress and summary as toasts so the library stays usable. M.

### P3

16. Undo/redo inside the sheet, "Match heights". M.
17. Dot grid, blue dashed outline, left-aligned size chip from S10. S.
18. Tint colour picker. M.
19. Reach engine ranges (radius 100, gap 200) by a numeric field. S.
20. Info line for board captures in the editor (made on, N sources). S.
21. Light theme and HiDPI check of the sheet; unify Vietnamese "Board" wording. S.

---

## E. LIVE-QA SCRIPT (final bundle)

Use the dev bundle, a private library (`CAPTUREMANN_LIBRARY`), imported images only (no screen recording). Observe by eye, then read the result with `capturemann library get` and open the PNG in the capture folder when sizes matter. Store one report in `verification/`.

Fixture (Import): `s2000.png` 2000x1200, `phone.png` 400x1600, `banner.png` 4000x300, `tall.png` 300x4000, `tiny1.png` 16x16, `tiny2.png` 24x12, `trans.png` (RGBA 50 % alpha), `cmyk.jpg`, `big.png` 20000x12500, `huge.png` 25000x20000 (500 MP), `shot.png` and a copy with blur, arrow, text; `n01..n14` small PNGs.

### Boards

| Case | Setup | Steps | Expected | Observe |
|---|---|---|---|---|
| B1 Pair defaults | `s2000`, `phone` | Select 2, Make a board | Pair, labels on, chip shows size | Chip vs final size; equal heights; labels in a band |
| B2 Preview vs output | same | Shadow, captions, radius 24, title, light grey | Preview = output | Known risk P1-1: compare screenshots |
| B3 Inspector reach | 1180x792 | Find Title, Captions | Reachable | Currently below the fold |
| B4 12 images | `n01..n12` | Make a board; Grid, Strip, Free | Valid, Grid 3x4 | Finish time; list scroll; scaled-down notice on strip |
| B5 13th image | select 13 | look at the bar | Disabled with a reason | Tooltip or text |
| B6 Wrong selection | 1 shot; shot + recording | | Disabled | same |
| B7 Mixed extremes | `banner`+`tall` | Pair, Strip, Grid, Free | Usable or warned | 22 px sliver; any note |
| B8 Different sizes | `big` vs `tiny1` | Pair | Equal heights | Resolution loss; warning |
| B9 Transparent | `trans` x2, Transparent | Finish; export PNG and JPEG | PNG alpha kept; JPEG on white | Alpha in a viewer; shadow on alpha |
| B10 CMYK | `cmyk.jpg` + `shot` | Pair | Colours right | vs source |
| B11 Annotated | annotated + blurred | Board of 2 | Annotations and blur visible; blur holds no clear pixels (AC2) | Zoom the board's `original.png` |
| B12 Source deleted | board made | Delete a source | Board unchanged (AC3) | SHA-256 before/after |
| B13 Tiny | `tiny1`+`tiny2` | Pair, labels, captions, title | Valid; text may exceed the picture | proportions (label min 14 px) |
| B14 Same image twice | Duplicate then select both; CLI same id twice | Pair | Defined behaviour | sheet rejects duplicate ids in `add_source`; CLI result |
| B15 500 MP guard | `huge`+`s2000` | Pair | Clear error or scaled board; no crash | Memory in Activity Monitor; message; sources decode in full before any guard: not determined |
| B16 Vietnamese labels | Language VI | Pair, labels | TRƯỚC / SAU with diacritics | Zoom; same in preview |
| B17 Swap order | Pair | Down on row 1 | Labels follow positions | |
| B18 Board of boards | 2 boards | Make a board | Works | Quality; badge |
| B19 Rename, duplicate | card menu | | Works; badge kept | badge after duplicate |
| B20 Export a board | editor Export | JPEG, lossless WebP at 1600 px | Sensible sizes; alpha to white | |
| B21 Free drag | 3 pictures | move, corner-resize, Alt, Shift at 1x and 2x | Pink guides and tag; no jump on release | never seen on screen |
| B22 Blueprint on a board | open a board | Blueprint | Text of all sources becomes bars | time; Undo |
| B23 Close with changes | edit, Esc | | Currently discards silently | expect confirm (P2-13) |
| B24 Library card | Boards chip | pair, strip, grid | badge, thumbnail aspect | vs S11 |
| B25 CLI / MCP | `capturemann board a b --layout pair --before-after --caption x --caption y --board-title T --json`; MCP | | Same as sheet | `label_words` for VI |

### Bulk export

| Case | Setup | Steps | Expected | Observe |
|---|---|---|---|---|
| X1 Basic | 3 pictures | Export selected, longest 1600, JPEG 90, folder | Files written; library unchanged | the real button and folder chooser (never run) |
| X2 200 files, clashes | 200 copies of 5 names + folder with some | `{name}` | `name (2).jpg`, no overwrite (AC3); `200 exported` | file count, time, progress |
| X3 Cancel mid-way | 200 big pictures | Cancel at about 50 % | Written files kept, no partial file (AC2) | `.tmp`/zero-byte files; wording |
| X4 Disk full | `hdiutil create -size 20m` volume | export there | Skipped "No space left"; Retry skipped after freeing | Retry runs only the skipped |
| X5 Not writable | `chmod 555` folder | | Clear error, no crash | where it shows |
| X6 Watermark tiny | `tiny1`, `tiny2` | text 25 % at 9 positions | Mark fits (text floor 8 px, margin at least 4 px) | mark larger than picture |
| X7 Opacity | 5 values, light and dark pictures | | Legible (white with black outline) | |
| X8 Image mark | PNG with alpha wider than picture | | Scaled to size; never taller than picture | |
| X9 Blueprint | screenshot with text | Blueprint + watermark + JPEG | Bars, annotations kept, capture unchanged | time per image |
| X10 Size rules | Longest/Width/Height/Percent 1 and 400, allow larger on/off | | `target_size` rules | upscale only when allowed |
| X11 Formats | PNG, JPEG 60/100, WebP | | WebP lossless only; JPEG alpha | sizes |
| X12 Pattern | `{date}-{n}-{name}-{w}x{h}`, empty, `../x` | | Empty blocked; no traversal | names |
| X13 501 captures | | | Daemon limit 500 (`MAX_CAPTURES`) | sheet message not determined |
| X14 Mixed selection | add a recording | | Export disabled or filtered | |
| X15 Vietnamese | language VI | open, run | No truncation | footer line, summary |
| X16 Retry skipped | after X4 | | New job, skipped files only | |
| X17 Sheet height | 1180x792 | | Footer visible, body scrolls (seen) | scrollbar cue |

---

## Not determined (needs a live run)

Free drag on screen and `raise()` wiring; HiDPI guides; light theme; Esc while busy; memory on huge sources (no pre-decode pixel guard found in `rendered_with`, only the 100 MP output limit); board card thumbnail against the S11 PNG; keyboard order and focus rings; tooltip on the disabled Make a board; the lower bulk sections as drawn; the real folder chooser and button; closing the bulk sheet mid-run.
