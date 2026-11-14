# QA 0.3.4 ANN-8 Zoom Lens (op `magnify`) on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data / Method:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md`

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `magnify` source `[129,510,117,26]` at `[900,640]`, zoom 3, shape `rounded` | PASS | revision 5; render written; OCR text unchanged except as expected; undo removes it |
| A loupe and a source partly outside the image (AC6) | PASS | `[1300,900,200,100]` at `[1200,700]`, zoom 2: accepted (revision 6), nothing rejected |
| Rejected parameters (AC5): zoom 9 and 1, source 6x6, `border_width` 33, a fifth magnify | PASS | final build, ops from files: zoom 9 and zoom 1 -> `invalid_input` `ops[0]: zoom must be in ...`; a 6x6 source -> `ops[0]: source must be at ...`; `border_width` 33 -> `ops[0]: border_width must be ...`; five lenses -> `ops[4]: at most 4 zoom lenses ...`; nothing stored in each case |
| Blur (pixelate over the e-mail line) plus magnify of that area at zoom 2, OCR (AC3) | PASS at zoom 2 | revision 1; the OCR text lacks `Contact: person@example.com`; zoom 4 and 8 and the opposite order not run |
| Pixel comparison with a direct 4x render, edge sharpness (AC1, AC4) | not run | the crate has golden tests; no pixel measurement on this laptop |
| Editor drag preview (AC7), editor and `annotate` render identical (AC8) | not run | editor not driven |
| Op kept by 0.3.3 in `unknown_ops` (AC9) | not run | |
| A thin colour fringe at the loupe's right edge | observation from the earlier report, not investigated | |

## Verdict

PARTIAL
