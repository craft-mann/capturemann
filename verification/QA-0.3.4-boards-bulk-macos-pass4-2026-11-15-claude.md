# QA 0.3.4 Boards and Bulk export, macOS pass 4 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, macOS pass 4)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** `~/Applications/CaptureMann-dev.app` (dev bundle of main `a4f28a07`), isolated library, config and socket under `/tmp/cmq4`; only my own app instance was started and killed; my own Chrome profile (`--user-data-dir=/tmp/cmq4/chrome`), one Terminal window and a small AppKit fixture window (`qafix`, 5 buttons, a fake e-mail label) were used and closed. The maintainer's library, Chrome and Terminals were not touched (the window picker listed their windows; I only searched for my fixture).
- **FFmpeg:** not used.
- **Method:** pictures taken with `screencapture` of the app window; windows excluded from capture (Long Shot overlay, HUD, preview, bar, toasts, dialogs) were read through the accessibility tree and `CGWindowListCopyWindowInfo` rectangles, or marked NOT SEEN. Synthetic clicks (CGEvent) only work on a non-key window after a pointer move first. The Long Shot overlay bar has no accessibility children, so Hand scroll could not be driven (NOT RUN).
- **Fixture:** my window screenshots (a Chrome page 1540 x 1720, a Terminal 1400 x 900), fixture pictures 900 x 600 (fake "Bob Smith" dialog), banner 3000 x 300, tall 300 x 3000, tiny 40 x 30, a CMYK JPEG. Boards were made through the CLI (`capturemann board`); the Make a board sheet itself was NOT driven or seen.

## Steps

| Case | Result | Evidence |
| --- | --- | --- |
| Pair with before/after labels, portrait + landscape sources | PASS | 2294 x 1005; both pictures at the same height, label pills above each image, 16 px margin (looked at the picture) |
| Pair with shadow, radius 24, title, captions, light grey | PASS | 1888 x 794; title top-left, captions under each image, soft shadow (looked at it) |
| Strip 3 / grid 2x2 / free 3 slots | PASS | 2812 x 664 / 1888 x 1288 / 832 x 765, valid images; library thumbnails fine |
| Banner + tall pair, and banner + tall + photo grid | PARTIAL (known B7) | 3118 x 364: the tall picture is a 22 px sliver; the grid is 688 x 688 with tiny pictures; no warning |
| Tiny sources 40 x 30 pair + labels + captions + title | PASS (valid) | 168 x 195; labels are larger than the pictures |
| CMYK JPEG import and board | PASS | import converts to RGB (`space: RGB`), board colours look right |
| **Compared with design S10 / S11** | DIFFERS | S10 shows rotated red/green STAMP labels (ANN-4 style) over the picture corner; the render uses flat grey "BEFORE" / green "AFTER" pills above the picture (clean, but not the stamp). S11 card badges "Board · Pair", "Long Shot · 12,480 px", "Click Guide · 8 steps" and the "Long Shots" filter chip are NOT on the cards or in the chip row (Boards and Click Guides chips exist) |
| Overall look of the pair board | Good: balanced, readable, equal heights; matches S10's structure (title, equal heights, captions, shadow) but not the stamp look | |
| Bulk export CLI: 3 pictures, JPEG, longest 600, text watermark | PASS | files written, never upscaled (tiny stays 40 px) |
| Name clash | PASS | second run: `photo (2).jpg`, `grad (2).jpg`, no overwrite |
| Pattern `../x` / empty | PASS | `_x.png` (no traversal) / "name_pattern: must be 1 to 200 characters" |
| Cancel, disk full, read-only folder, 200 files, watermark positions, Retry skipped, board source deleted, 12 images, Free drag, sheet and bulk sheet by eye, Vietnamese | NOT RUN | |

## Problems found

- **BB-1 (low)**: library cards carry no kind badges (S11) and no Long Shots chip.
- **BB-2 (low)**: labels are flat pills, not the S10 stamps (decide which is intended).
- **BB-3 (low, known)**: extreme aspect ratios give a sliver with no warning.

## Verdict

PARTIAL (renders good; sheet UI not run).
