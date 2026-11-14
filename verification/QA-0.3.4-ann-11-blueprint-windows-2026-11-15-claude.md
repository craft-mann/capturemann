# QA 0.3.4 ANN-11 Blueprint on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data / Method:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md`

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `blueprint` op with one box `[84,394,411,32]` over the e-mail line, style `bar`, colour `auto` (AC1 in small) | PASS | revision 2; OCR of the render no longer contains `Contact: person@example.com`; the other lines (title, customer, total, status, button) are all still read |
| Bulk export with `--blueprint` to WebP (BLK-1 + ANN-11) | PASS | job `exported: 1`, 1332x951, 8,390 bytes (a render with every text line replaced by a bar compresses far below the PNG); the capture itself unchanged |
| `suggest blueprint` (AC4) | PASS (earlier run) / not captured here | the final lines of this run were not kept; `L1-windows-0.3.4-main-a1bf57dc-feature-cases-...` has 11 boxes, `recognizer: windows_ocr` |
| Base-edit order: an `arrow` before the `blueprint` | PASS | final build: `invalid_input`, `ops[1]: \`blueprint\` is a base edit ...` |
| 20 lines with 2 kept (AC1) | not run | the page has 8 lines |
| Bars lie inside their boxes, equal height and gap in a paragraph (AC2) | not run | not measured |
| Keep-list change is one op update (AC3) | not run (editor) | |
| Copy text after Blueprint returns only the kept lines (AC5) | PASS (same as AC1: `capturemann text` reads the rendered image) | |

## Verdict

PARTIAL
