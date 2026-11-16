# QA 0.3.4 editor, library and misc, macOS pass 7 (PARTIAL, thin)

- **Date:** 2026-11-16. **Who:** Claude (QA agent, macOS pass 7). **Build:** dev bundle from `main` b2169576, private instance, Vietnamese UI for the screenshots. Most of this topic needs clicking inside the editor canvas, which has no accessibility tree; I drove only what the CLI exposes and looked at window captures of my own studio window. **Much of the brief is NOT RUN** (list below).

| Case | Result | Evidence |
| --- | --- | --- |
| Retype (agent op) with Vietnamese "Liên hệ" in a 119 x 34 box on a 2200 x 1720 page | PASS (seen) | `QA-0.3.4-shots/macos-pass7-retype-vietnamese.png`: diacritics complete, bold; a solid fill I passed on a gradient leaves a visible patch (my fill, not a defect) |
| Retype text wider than its box (Vietnamese sentence in a 99 px box) | PASS (as specified for agents) | clipped to the box ("Bảng g"); the editor widens the box, an agent op is stored as sent. Not seen in the editor edit-mode preview: the "preview shows the whole text, wide text near the image edges" check is **NOT RUN** |
| Blueprint: `annotate --suggest-blueprint` on the carchargeos page | PASS (suggest only) | 118 lines found by Apple Vision with fill and colour per box (solid or linear); I did **not** apply a `blueprint` op or look at the result this pass |
| Board of two screenshots (`board a b --layout pair --before-after`) | PASS (job result) | new `Board` capture, 3828 x 1855; picture not looked at |
| Bulk export (`export-images`, 3 shots, longest 800, JPEG, watermark text DRAFT) | PASS (files exist) | 3 JPEG files (109 to 329 KB), job `done`; watermark and size not looked at |
| Vietnamese library and editor strings | PASS (seen) | see the Click Guide report; library: "Tất cả bản chụp", "Hiện", "Đã nhập / Đã tải lên / Đã đăng / Cần chú ý / Bảng ảnh", sort row "Ngày / Tiêu đề / Thời lượng / Kích thước", "Hôm nay", sizes "7,8 MB" with a decimal comma |
| Published and Needs attention chips in Recordings | NOT RUN (chips seen only in All captures, count 0) | |
| Move area, Zoom Lens, Text Auto colour, tile hover/active/disabled, library hover-select and selection bar, Retype edit-mode preview, Blueprint applied | NOT RUN | need the editor canvas or pointer-state looking; no accessibility tree |

## Defects

| Id | Severity | Defect |
| --- | --- | --- |
| LIB-1 | low (carried) | cards of tall Long Shots (1540 x 13418) show a blurred crop of the picture, again seen this pass |
| CG-11 | low | see the Click Guide report (cut tile label) |

## Notes

- The confirm card of an agent request cannot be read by screen capture or the accessibility tree, and Return declines it. UX proposal: expose it to the accessibility tree and make Return the safe action named on the button.
