# QA 0.3.4 ANN-10 Move area on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data / Method:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md` (same capture of `text.html`, 1332x951; `annotate --ops <file>`; OCR with `capturemann text`)

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `move_area` of the line "Status: pending approval" (source `[84,446,328,32]`) to `[84,590]`, white fill (ANN-10.AC1, text level) | PASS | revision 3; OCR order before: `... Contact ... Status ... Save all`; after: `... Save all` then `Status: pending approval` (the line moved below the button); the old place is empty |
| `move_area` with `to: null` on the "Customer" line (AC2) | PASS | revision 4; the OCR text no longer contains `Customer: Example Person` |
| Pixel check that the hole matches the bar colour within 6 levels (AC1) | not run | no pixel measurement of the fill ring on this build (the earlier report `L1-windows-0.3.4-main-a1bf57dc-feature-cases-...` ran the same op: PASS, hole filled) |
| Source overlapping the destination (AC3) | not run | covered by the golden test in the crate |
| Base-edit order: an `arrow` before the `move_area` (AC4) | PASS | final build: `invalid_input`, `ops[1]: \`move_area\` is a base edit ...` |
| Undo | PASS | six undo calls restore the original image text |
| Editor tool | not run | |

## Verdict

PARTIAL
