# QA 0.3.4 library, Make a board sheet, Export selected sheet, macOS pass 5 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 5). **Machine:** Apple M4 Pro, macOS 26.6.2, 1512x982 pt.
- **Build:** dev bundle of main a4f28a07, isolated library, config and socket under /tmp/cmq5; no source edits.
- **Method:** CGEvent tool, screencapture of the app window, bundle files. Pictures: QA-0.3.4-shots/macos-pass5-*.png (fixtures only).

| Item | Result | Evidence |
| --- | --- | --- |
| Hover tick, selecting mode, ring, selection bar with X | PASS (seen) | empty circle on hover; after one tick all cards show circles; selected cards ringed; bar "N selected / Make a board / Export selected / Delete / X"; Make a board greyed at 1 selected. macos-pass5-library-selecting.png |
| Esc clears the selection | PASS once, FAIL once (D-LIB1, low) | worked on a fresh page; after closing the Export sheet with Done, Esc did not clear (the X did) |
| Make a board sheet: Pair/Grid/Strip/Free, reorder arrows, title, captions, Finish | PASS (seen) | Pair dimmed for 3 images; Strip showed number badges; arrows reordered; Captions and Title (reached by scrolling the right panel) worked; Finish opened the board in the editor titled "QA board" with captions. macos-pass5-board-sheet-strip.png, macos-pass5-board-result.png |
| Toast after Finish | NOT SEEN | toast windows are excluded from capture |
| Sheet versus design S10 | PARTIAL | not in the sheet: the "Drag screenshots here, 2 to 12" drop tile and drag reorder (arrows only); Cancel and Finish are in the footer, S10 puts them in the header. A 300x3000 image in a Strip becomes a sliver |
| Export selected sheet | PASS (seen, measured) | fits the 1180x792 window, footer visible, sections scroll: Size, Format, Watermark, Blueprint, File names (token keys and example), Save to / Choose. The folder panel opened on a folder from another session and set it without my choosing (EX-1). Export wrote 2 files; summary "2 exported" with Show in folder / Done. Blueprint export of 20 lines gave 20 bars. macos-pass5-export-sheet.png |
| Progress and Cancel | NOT SEEN | the job finished before a picture |
| Delete with a paused upload (B1) | NOT RUN | no paused-upload fixture built |
| Filter chips and sort | PASS (seen) | Screenshots: Imported, Uploaded, Boards; sorts Date, Title, Size (no Duration). All captures: six chips; Duration present |
| Card menu, rename | PASS (seen) | Open, Reveal in Finder, Copy text, Rename..., Delete...; rename worked (typed with osascript). macos-pass5-card-menu.png |
| Sidebar tiles | PARTIAL | Long Shot and Click Guide tooltips seen; active and disabled states NOT RUN |
| Agents & CLI offline shape | NOT RUN | paused by the maintainer |

Verdict: PARTIAL.
