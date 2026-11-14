# QA 0.3.4 BLK-1 Bulk export on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine / Build / Data:** as in `QA-0.3.4-ann-9-retype-windows-2026-11-15-claude.md` (3 screenshots: 400x200, 1332x951, 880x480 board)
- **Method:** `capturemann export-images <ids> --dest <dir> ... --wait`; files measured with `System.Drawing`

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| `--longest 600 --format jpeg --quality 80 --name-pattern qa-{n} --watermark-text QAMARK` (AC1 in small) | PASS | 3 files `qa-1.jpg` 400x200, `qa-2.jpg` 600x428, `qa-3.jpg` 600x327 (the 400 px source is not enlarged); job `exported 3`, `skipped 0`; the same command without the watermark gave 8,807 / 37,021 / 23,884 bytes |
| The same command again, same folder (AC3) | PASS | `qa-1 (2).jpg`, `qa-2 (2).jpg`, `qa-3 (2).jpg` written, the first three untouched, summary `3 exported`, `skipped 0` |
| `--blueprint --format webp` | PASS | 1 file, 1332x951, 8,390 bytes, lossless WebP |
| Watermark visible | not looked at (the exported pictures show an Edge window; not opened) | the watermark text was passed, the job reports success |
| 50 files (AC1), Cancel at 50 % (AC2) | not run | |
| Library entries unchanged | PASS | the three captures kept their sizes in `library list` afterwards |
| Export selected sheet, Retry skipped | not run | UI not driven |

## Verdict

PARTIAL
