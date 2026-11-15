# QA 0.3.4 Vietnamese UI on the new surfaces, macOS pass 6 (PARTIAL)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 6). Final dev bundle of main `0bcac6a3`. `appearance.language` is an app-only setting (the CLI refuses with `app_only_setting`), so I wrote `"appearance.language": "vi"` into the isolated `settings.json` with the app stopped and restarted it. Checked by screenshots of the app window (1180 x 792 pt) and accessibility text; no pixel-width measurement, so "no clipping seen" means by eye.

| Surface | Result | Notes |
| --- | --- | --- |
| Sidebar tiles and items | PASS (seen) | Quay…, Vùng, Cửa sổ, Màn hình, Chụp dài, Hướng dẫn; Tất cả bản chụp, Bản ghi, Âm thanh, Ảnh chụp màn hình, Hướng dẫn nhấp, Mô hình AI, Tác nhân & CLI, Cài đặt: nothing cut |
| Recordings / All captures chips | PASS (seen) | Đã nhập, Đã tải lên, Đã đăng, Cần chú ý, Bảng ảnh, Hướng dẫn theo thao tác: the longest chip fits; card pill "Đang chờ đăng nhập để tải lên" fits |
| Library selection bar | PASS (seen) | "Đã chọn 2 | Tạo bảng ảnh | Xuất mục đã chọn… | Xoá | ×" fits. Note "Xoá" (elsewhere also "Xoá") vs the common spelling "Xóa": style only |
| Make a board sheet | PASS (seen) | title "Bảng ảnh mới", "Có thể chỉnh bố cục trước khi chọn "Hoàn tất"", layouts Cặp/Lưới/Dải/Tự do, "TRƯỚC"/"SAU" labels, Hoàn tất bảng ảnh: fits. The helper text says "kéo để đổi thứ tự" (drag to reorder) |
| Export selected sheet | PASS (seen) | "Xuất 2 ảnh chụp màn hình", Kích thước / Định dạng / Hình mờ / Bản phác thảo / Tên tệp; "WebP (không mất dữ liệu)" fits. "Hình mờ" for the watermark can read as "blurred image" (wording proposal) |
| Click Guide editor | PASS (seen) | list rows wrap `Click "Open"` over two lines, header "QA6 guide vi · 500 bước", buttons Sao chép dưới dạng văn bản có định dạng, Xuất…, Sửa ảnh, Chụp lại, Nhân đôi, Xoá, Toàn bộ cửa sổ; caption "nhấp tại 150, 224 · chụp ngay khi nhấn nút chuột" |
| Click Guide bar | NOT SEEN | no bar for the CLI-started guide; the sidebar-started bar was not run in Vietnamese |
| Long Shot toasts | NOT SEEN | no toast appeared for CLI runs |
| Settings (Chung, Chụp & quay) | PASS (seen) | rows, segmented controls, hints fit |
| Editor (Select tool, tool palette panel, view bar) | PASS (seen) | "Chọn", "Ảnh", "Kích thước", "Con trỏ", "Đã lưu mọi thay đổi" |
| **Untranslated strings** | defect | the agent confirm dialog for starting a Click Guide: body "Start a Click Guide: CaptureMann will take a screenshot of each mouse click (clicks only; keystrokes are never recorded)" and button "Start guide" are English while its title is Vietnamese; the guide-start confirmation says "Start guide" |
| Pointer readout | low | in the editor at 10 % zoom "Con trỏ" showed "11730, 3589" for a point outside a 1540 x 8330 image (shows an out-of-image value instead of "-") |

## Verdict

PARTIAL. The new library, board, export and guide-editor surfaces fit in Vietnamese; the guide-start confirmation is not translated.
