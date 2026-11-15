# QA 0.3.4 Blueprint, Retype and the ops file on Windows, pass 7 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent). **Machine:** ThinkPad T490, Windows 11. **Build:** final build from main `011b5000`, installed (`capturemann 0.3.4`).
- **Method:** the shipped `capturemann.exe` against a private library and socket: images made with System.Drawing (Segoe UI 28 px and 30 px), `import`, `annotate --suggest-blueprint --json`, `annotate --replace --ops <file> --out <png>`; the output PNGs were pulled to the Mac and looked at (not only measured). No GUI driving.

| # | Case | Result | Evidence |
| --- | --- | --- | --- |
| 1 | Blueprint suggestions on a Windows-recognizer screenshot (800x500, 4 lines, 28 px) | PASS | `recognizer: windows_ocr`, `line_count: 4`, boxes `[34,34,138,32]`, `[32,105,186,31]`, `[34,173,303,40]`, `[33,243,226,40]` (32 to 40 px high for a 28 px font; pass 5: 17 to 19 px) |
| 2 | Blueprint, bar style | PASS | looked at the PNG: four bars, no letter tops or tails visible above or below any bar, no letter sticking out at the right |
| 3 | Blueprint, box style | PASS | looked at the PNG: four blocks as tall as the lines, nothing of the text visible |
| 4 | Undo of a Blueprint, then a second Blueprint | PASS | `undo` revision 2 returned the original image; the next `annotate` made revision 3 |
| 5 | `annotate --ops <file>` with a UTF-8 BOM | PASS | the file written with a BOM was accepted (pass 5: "expected value at line 1 column 1") |
| 6 | Retype `Ngọc Ấn Đặng Thị Hường` (30 px) in a 36 px high box | PASS | looked at the PNG: every dot-below and stacked mark (ọ, Ấ, Ặ, Ị, ờ) and the descenders are whole; no specks of the old text below the box |
| 7 | Retype in a 56 px box | PASS | same picture quality |

## Notes

- `annotate` right after `import` answered `not_found` for the new id once (the import job was still `queued`); it worked 4 s later. Low: the import answer returns an id that `annotate` cannot use yet.
- The Windows recognizer's boxes are now padded; the pass-5 privacy defect (text tops visible) is not reproduced on this image. Only one image and one font were tried.

## Not run

Blueprint on a busy photo, mixed languages, Retype in the editor window (GUI), Move area and Zoom Lens (not changed since pass 6).

## Verdict

PARTIAL
