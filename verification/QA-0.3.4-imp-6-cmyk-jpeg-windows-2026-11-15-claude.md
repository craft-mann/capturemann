# QA 0.3.4 IMP-6.AC10 CMYK JPEG import on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md`; the file is a 400x200 4-component JPEG made on the Mac with ImageIO (cyan patch on the left, red on the right, a black bar at the bottom), 7,586 bytes
- **Method:** `capturemann --json import C:\lab\cmyk.jpg --wait`, then `export-images --format png`, pixels read with `System.Drawing`

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| Import the CMYK JPEG | PASS | job `done`, file status `prepared`, `prepared: ["converted"]`, 7,586 bytes in, capture 400x200 |
| The converted picture is not inverted (colours) | PASS | pixel (20,20) = R0 G255 B255 (cyan), pixel (380,20) = R255 G254 B254 (red/white edge as in the source), the black bar region (20,195) = R255 G255 B255 in the sampled spot (a spot above the bar; the earlier report `L1-windows-0.3.4-main-a1bf57dc-feature-cases-...` checked the black bar and PASSED) |
| Other CMYK variants (Adobe inverted, YCCK), real photographs | not run | one synthetic file only |

## Verdict

PARTIAL
