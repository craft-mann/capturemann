# QA 0.3.4 ANN-9 Retype on Windows (PARTIAL)

- **Date:** 2026-11-15 (laptop clock Nov 14, 23:05 to 23:40)
- **Who:** Claude (agent); nobody watched
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %
- **Build:** main `26a05100` release package built on the laptop (hashes in `QA-0.3.4-cap-6-long-shot-windows-2026-11-15-claude.md`); the final-build re-run lines are marked below
- **Data:** a screenshot of an Edge window showing a local page `text.html` ("Total amount due is 1200 dollars", flat light background), 1332x951 physical px; isolated library; deleted at the end. The picture itself was not stored (Edge's toolbar shows the profile picture).
- **Method:** `capturemann annotate <id> --ops <file>` (JSON from a file: PowerShell 5.1 strips the quotes of inline JSON), text read back with `capturemann text` (Windows OCR, which reads the rendered image), a pixel diff of the render against `original.png` with `System.Drawing`.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `retype` of the line at rect `[83,342,432,25]`, solid white fill, text "Total amount due is 1500 dollars" (AC1) | PASS | revision 1; OCR of the render: `Total amount due is 1500 dollars` and no `1200`; pixel diff outside the rect against the original: **0 pixels** |
| Undo (AC6 partly) | PASS | after six undo calls (retype, blueprint, move, move to null, magnify twice) the OCR text is the original again (`1200`, all lines present) |
| Base edits first (AC4): an `arrow` listed before the `retype` | PASS | final build, ops from a file: `invalid_input`, `ops[1]: \`retype\` is a base edit ...` and nothing stored |
| Gradient fill (AC2) | not run | the test page has a flat background only |
| Photo background warning (AC3) | not run (editor flow) | |
| Revert to original byte-identical `original.png` (AC6) | not run | the undo walk above restores the text, `original.png` was not hashed before and after |
| Older app keeps the op in `unknown_ops` (AC7) | not run | needs a 0.3.3 build |
| Editor tool (Retype in the app window) | not run | not driven |

## Acceptance criteria

AC1 PASS. AC2 not run. AC3 not run. AC4 PASS. AC5 (agent op equals editor op) not run (editor not driven). AC6 partly (undo restores the text). AC7 not run.

## c2 re-run on the final build

Run on the final build (zip SHA-256 `2A3717F6B1C6B2955130874B1FC651128DE39F71388F7F2EDFF760AE98AA8F13`, commit `ac730b17` plus the Windows tree of `26a05100`): the AC4 line above. The first-build run is the one in the Steps table.

## Verdict

PARTIAL
