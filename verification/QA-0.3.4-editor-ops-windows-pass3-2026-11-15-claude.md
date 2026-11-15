# QA 0.3.4 touch-up ops, boards, bulk export, CMYK on Windows, pass 3 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11. **Build:** main `5e832011`, installed 0.3.4.
- **Method:** shipped CLI against its own daemon with a private library, a 1200x800 fixture drawn with System.Drawing (flat `#F5F5F7`, 20 text lines, a gradient block, one tight line `Total amount due is 1200 dollars`), pixel comparisons of the rendered PNG against the original in C#, OCR with `capturemann text`. The editor window itself was not driven. Note: `import` without `--wait` returns before the capture exists (documented; my first runs got `not_found`).

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| ANN-9 Retype, Vietnamese stacked marks in a tight box (rect = tight bbox 394x20, text `ẤẾỘỪẶẪ Tổng số tiền 1500`, size 22) | PASS | 0 pixels changed outside the rect grown by 90 % of its height vertically; 125 changed pixels above the rect top and 39 below (the grown erase and clip); 60 dark pixels in the 14 rows above the tight rect (marks drawn, not clipped); OCR reads `1500` and no `1200` (Windows OCR has no Vietnamese, the diacritics come back as other letters; glyph shapes not judged by eye) |
| Retype undo | PASS | OCR contains `1200` again |
| Retype with a text far wider than the box | PASS | accepted and clipped silently by `annotate` |
| Base edit order (retype after an arrow) | PASS | `invalid_input: ops[1]: retype is a base edit and must come before every other op` |
| ANN-10 Move area (line 5 to 640,400, solid fill) | PASS | 10080 of 10080 destination pixels equal the source; source area fully light (hole filled) |
| Move area with overlap and delete (`to: null`) | accepted, rendered, undone | not pixel-checked |
| ANN-11 Blueprint, 22 suggested boxes, 20 lines, keep 2 | PASS | `suggest_blueprint` 22 boxes; after the op OCR finds only the two kept lines (`Setting 04`, `Setting 07`) |
| ANN-8 Zoom Lens zoom 4 rounded | PASS (limited) | the loupe area differs from the original (42,929 px), outside it only the connector lines changed (1,538 px); zoom 1 and zoom 9 rejected with `ops[0]: zoom must be in 2.0..=8.0` |
| BRD-1 boards: pair, strip, grid | PASS | 2450x840 each for two 1200x800 sources with gap 10, margin 20 |
| Board before/after with captions, title, dark background, radius, shadow | PASS | OCR reads `Title`, `BEFORE` and the source text |
| Board `free` with `--slot` | NOT RUN | PowerShell split my `--slot 0,0,600,400`; not repeated |
| Board with one image | PASS | refused by the CLI parser |
| Deleting a source leaves the board unchanged | NOT RUN | the delete needs an on-screen confirmation I did not answer; the hash line in my log is not evidence |
| BLK-1 export of 3 images (`--longest 600 --format jpeg --quality 80 --watermark-text DRAFT --name-pattern`) | PASS | three files `bulkN-600.jpg`, 37,649 bytes each |
| Second run into the same folder | PASS | 6 files, nothing overwritten |
| 50 files (`--width 400 --format webp`) | PASS | 50 files in 2 s |
| Cancel a bulk job | NOT RUN | job id not read in my script |
| Blueprint + WebP bulk | job accepted | output not inspected |
| IMP-6 CMYK JPEG (Pillow, RGB 200,40,40 converted to CMYK, quality 95) | PASS | imported 64x64, centre pixel 200,40,40 |

## Problems

None in the product from this pass.

## Verdict

PARTIAL
