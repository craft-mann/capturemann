# QA 0.3.4 Open With and Vietnamese spot check, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd`, isolated library/config/socket under `/tmp/cmq2`; the maintainer's app was not running.

| Item | Result | Evidence |
| --- | --- | --- |
| `open -a CaptureMann-dev.app file.png` with the app running | PASS | the running instance showed the "Import 1 file" sheet with the PNG (900 x 600, PNG, "Ready", "Remove the originals after importing" unticked, Cancel / Import 1 file). Not run: a cold launch by Open With (it would start the bundle without my isolated environment, so it could open the maintainer's library) and importing with several files. Remark O1 (low): "12.0 GB free" in the sheet while `df` showed 39 GB free on the volume |
| Vietnamese: Settings | PASS | General page fits (`macos-pass2-vi-...` not saved; text read: "Cai dat", "Chung", "Ngon ngu", labels fully visible) |
| Vietnamese: library bar | PASS | "Da chon 2 / Tao bang anh / Xuat muc da chon... / Xoa / x" fit in the bar (`macos-pass2-vi-library-bar.png`); chips and sort bar fit |
| Vietnamese: Make-a-board sheet | PASS | all labels fit (`macos-pass2-vi-board-sheet.png`) |
| Vietnamese: sidebar | DEFECT (low) | "Huong dan theo thao tac" (Click Guides) is cut to "Huong dan theo thao..." in the sidebar; the page title and buttons fit |
| Vietnamese: guide editor | PASS | step rows wrap, "Vung de xuat che can xem lai" fits, buttons fit (picture not saved, it shows other windows) |
| Vietnamese: Click Guide bar, Long Shot bar | NOT SEEN | hidden from capture; the bar window is 580 x 58 as in English |

## Verdict

PARTIAL.
