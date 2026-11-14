# QA-0.3.4 Retype, Move area, Blueprint, Zoom Lens (ANN-8 to ANN-11) report, macOS (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent)
- **Machine:** Apple M4 Pro, macOS 26.6.2
- **Build:** dev bundle of main `26a05100`, isolated instance (`/tmp/cmq`)
- **Fixture:** a 900x600 dialog drawn with Swift (flat, gradient-button and noise variants), imported; CLI `annotate --ops ... --out`, renders looked at; OCR with `capturemann text`.

| Item | Result | Evidence |
| --- | --- | --- |
| `suggest_blueprint` on the flat dialog | PASS | 7 boxes; "Save" and "Cancel" are now separate boxes with their own fill (`#0088FF`, `#CCCCCC`); this was the merged "SaveCancel" box (D2) of the 2026-11-14 report, so D2 appears fixed |
| Retype (ANN-9) | PASS with remark | "Customer: Bob Smith" became "Customer: Alice Jones" on the flat fill; old text gone. The new text was longer than the box and was clipped ("Alice Jor"), as the spec says, but `annotate` returned `warnings: []` (the panel warning is editor only) |
| Move area (ANN-10) AC1 | PASS | orange icon moved 160 px down, hole filled with the background |
| Move area AC2 (`to: null`) | PASS | area filled, nothing pasted |
| Blueprint (ANN-11), keep 2 lines | PASS | bars on every other line (title bar, three lines, Cancel), "Customer: Bob Smith" and the Save button kept; bars have equal height; the icon untouched (D3 of the earlier report is not reproduced here because the icon was not recognised as text) |
| Zoom Lens (ANN-8) zoom 3, rounded, connector, badge | PASS | lens shows the enlarged line, white border, connector, `x3` badge |
| Retype on gradient/photo, editor drawing of lens, Move area drag, Blueprint keep by click, Select text tool | not run | see `QA-0.3.4-editor-library-2026-11-14-claude.md` for the earlier partial GUI pass |

Pictures were looked at (`retype`, `move`, `blueprint`, `lens` renders); not stored (synthetic, in `/tmp/cmq/out`).

## Verdict

PARTIAL (CLI/render level only).
