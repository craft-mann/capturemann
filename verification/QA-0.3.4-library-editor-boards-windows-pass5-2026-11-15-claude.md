# QA 0.3.4 editor ops, boards, bulk export, CMYK on Windows, pass 5 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (agent). **Machine:** ThinkPad T490, Windows 11, 1920x1080 at 150 %. **Build:** main `0bcac6a3`, installed 0.3.4 (see the summary for hashes).
- **Method:** shipped `capturemann.exe` (`import`, `annotate --ops`, `--suggest-blueprint`, `board`, `export-images`) against a private library; images made with System.Drawing; outputs pulled to the Mac and looked at. No GUI driving.

| Case | Result | Evidence |
| --- | --- | --- |
| Text op without colour on a light screenshot | PASS | `auto_text` colour `#111111`, outline null; an explicit `color` is not reported |
| Same on a dark screenshot | PASS | `#FFFFFF`, outline `#111111` width 2 |
| Same on a busy random-block photo | PASS | `#111111` with white outline width 2, readable (looked at the image) |
| Retype with Vietnamese in a tight box (36 px box, 30 px font) | FAIL (low) | the dot-below marks (ọ, ặ, ị, ợ) and the bottoms of g are clipped flat at the box edge; the old line's lower marks stay visible below the box as specks. A 56 px box renders correctly. |
| Retype bold + centre, roomy box | PASS | looked at the image |
| Retype after another op | PASS | `invalid_input`: `ops[1]: retype is a base edit and must come before every other op` |
| Move area (move, and `to:null` delete) | PASS | moved line appears at the new place, hole filled white |
| Blueprint (bar and box styles) from `--suggest-blueprint` boxes | FAIL (medium) | Windows OCR boxes for "Plan:", "Team", "Amount: 99.00" are 17-19 px high for a 28 px font; the bar or box covers only that, so the tops of the old text stay visible above it, and the last letter of "Example" sticks out. For a privacy use this leaks text. Same boxes feed `export-images --blueprint`. |
| Zoom Lens (circle with red border, rounded, rect without connector and badge, two lenses) | PASS | drawn, connector lines to the source, clipped at the image edge when the lens is bigger than the picture (my choice of numbers) |
| `--ops` file saved as UTF-8 with BOM (Windows PowerShell `Set-Content -Encoding UTF8`) | FAIL (low) | `invalid_input: --ops: invalid JSON: expected value at line 1 column 1`; strip a BOM |
| Boards pair with BEFORE/AFTER, pair stacked, strip (gap, margin, dark, radius, shadow), grid 2x2 with captions and title, free with slots | PASS | looked at the PNGs. Notes: in the strip with `--background dark` the dark screenshot vanishes into the background and its badge "2" is hardly visible; in the grid, white screenshots on the white board have no edge, captions are 12 px on a 1688 px board |
| Board errors: one id, pair with 3, free without slots, 13 ids in grid | PASS | clear messages (the 13-id case was a CLI arity error in my command) |
| Bulk export 9 captures: jpeg q80, longest 600, watermark DRAFT, name pattern | PASS | 9 files, sizes right |
| Bulk export again to the same folder | PASS | never overwrites: `light-600x375 (2).jpg` |
| Bulk export `--blueprint`, long (300 char) destination, destination on a missing drive | PASS | job ends `job_failed` with the OS error text; the long-path message prints the whole 300-character path |
| CMYK JPEG (4 components, made with ImageIO) import and export | PASS | cyan patch R0 G255 B255, red patch R255 G0 B0, black bar R0 G0 B0 |
| Not run | | Open With, Vietnamese UI of the new surfaces, B2/B3 HUD and bubble drag, boards and Retype in the editor UI, undo of ops, library-path error through the app |

## Verdict

PARTIAL
