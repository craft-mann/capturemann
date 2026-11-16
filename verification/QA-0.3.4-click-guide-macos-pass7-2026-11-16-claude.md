# QA 0.3.4 Click Guide, macOS pass 7 (PARTIAL)

- **Date:** 2026-11-16. **Who:** Claude (QA agent, macOS pass 7). **Build:** dev bundle from `main` b2169576 (binaries Nov 16 10:42), private instance (own library, config, socket). Guides started from the CLI (`guide start`), the agent confirm answered with the keyboard (Tab, Return). Clicks were synthetic mouse events on my own fixture page (`QA7 clicks`, 6 buttons) in my own Chrome profile only; nothing else was clicked. Two clicks per fixture hit Chrome's toolbar by my mistake (my coordinates were 100 pt too high); they are in the window guide as steps 1 to 3 and are the toolbar of my own Chrome.
- Not driven: the guide bar and the sidebar tile start (no accessibility tree), countdown on screen, retake, 500-step cap (heavy; pass 6 covered it), rich-text paste.

| Case | Result | Evidence |
| --- | --- | --- |
| Guide of a WINDOW (`--window "QA7 clicks"`, 6 clicks) | PASS (measured, seen) | all 6 steps `focus_crop: false`; every stored step image is the whole window, 1540 x 1720; exports (below) are 800 x 894 for every step: the whole window in every step, no automatic crop (maintainer decision of Nov 16) |
| Guide of a DISPLAY (`--display 1`, 6 clicks inside one 700 x 300 pt area) | PASS (measured, seen) | all 6 steps `focus_crop: true`; stored frames are the whole display (3024 x 1964); every exported step image is the same 800 x 520, so one shared crop; the click badge ("4" in a ring) sits on the clicked button |
| Step titles | PASS | `Click "Button 1"` and so on from the clicked control's accessibility name; toolbar clicks gave `Click "O File"`, `Click here` |
| Export Markdown | PASS | `guide.md` with numbered headings and `images/step-01.png` to `step-06.png` |
| Export HTML | PASS (file written, not opened) | one `.html` file per guide |
| Export PDF | PASS (file written, not opened) | `.pdf` 1.2 MB (display) and 0.28 MB (window) |
| Vietnamese name "Hướng dẫn theo thao tác" | PASS (seen) | chip in the Library ("Hướng dẫn theo thao tác 2"), sidebar entry (cut with an ellipsis: "Hướng dẫn theo thao t…"), tile under Capture (see defect CG-11), guide editor header "QA7 window guide · 6 bước", buttons "Sao chép dưới dạng văn bản có định dạng", "Xuất...", "Sửa ảnh", "Chụp lại", "Nhân đôi", "Xoá", "Bước ghi chú", "Toàn bộ cửa sổ"; note text "nhấp tại 340, 234 · chụp ngay khi nhấn nút chuột" |
| Library card thumbnails | NOTE | the display guide's card showed "No preview" at first and a thumbnail some minutes later; the thumbnail shows the whole uncropped display frame, including other windows' pixels, while the exported images are cropped |
| Big-frame label | NOTE | in the editor the step picture of a plain window guide (1540 x 1720) carries the pill "Ảnh dài · nhấp để xem đầy đủ" ("Long image · click to see full") although it is not a Long Shot |
| Library chips Published / Needs attention | PASS (seen) | both chips present with counts (0, 0) next to Boards and Hướng dẫn theo thao tác |

## Defects

| Id | Severity | Defect |
| --- | --- | --- |
| CG-11 | low | Vietnamese: the tile "Hướng dẫn theo thao tác" wraps to two lines and the second line ("thao tác") is cut at the bottom of the tile |
| CG-12 | low | The "Long image" pill appears on a normal tall window frame |
| CG-13 | low | The display guide's thumbnail is the uncropped frame (more private content than the export) |

Pass 6 items CG-9 (silent 500 cap) and CG-10 (memory) were not rechecked on macOS this pass.

## UX proposals

- Show the crop rectangle on the step picture of a display guide (and let the user set or drop it), because the stored frame is the whole screen but the export is a crop.
- Use the cropped view for the library thumbnail of a guide so it matches the export.
