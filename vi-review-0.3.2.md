# Rà soát bản dịch tiếng Việt của CaptureMann 0.3.2

Bảng có 11 chuỗi, với 6 đề xuất sửa ở các dòng 3, 4, 8, 9, 10 và 11. Cột Fuzzy ghi lại trạng thái của từng chuỗi trong danh sách gốc.

## Sao chép văn bản (ANN-5)

| # | Ngữ cảnh | Tiếng Anh | Tiếng Việt đề xuất | Fuzzy |
| --- | --- | --- | --- | --- |
| 1 | - | Copied {n} word / Copied {n} words (one form in Vietnamese) | Đã sao chép {n} từ | yes |
| 2 | - | Copy text | Sao chép văn bản | yes |
| 3 | - | Copy text needs a Windows text recognition language. Add one in Settings > Time & language > Language & region (it installs the text recognition part), then restart CaptureMann. It reads only the languages you install there. | Để sao chép văn bản, hãy thêm một ngôn ngữ có hỗ trợ nhận dạng văn bản trong Windows. Vào Settings > Time & language > Language & region để thêm ngôn ngữ và cài kèm tính năng nhận dạng văn bản, rồi khởi động lại CaptureMann. CaptureMann chỉ nhận dạng được văn bản bằng các ngôn ngữ đã cài tại đó. | yes |
| 4 | - | Copy text needs the tesseract program. Install it (Debian and Ubuntu: sudo apt install tesseract-ocr), then restart CaptureMann. It reads English; other languages need their tesseract language data. | Để sao chép văn bản, hãy cài Tesseract rồi khởi động lại CaptureMann. Trên Debian và Ubuntu, dùng lệnh `sudo apt install tesseract-ocr`. Tesseract nhận dạng được văn bản tiếng Anh. Với các ngôn ngữ khác, hãy cài thêm gói dữ liệu tương ứng. | yes |
| 5 | - | Couldn’t read the text | Không đọc được văn bản | yes |
| 6 | - | Found text, but could not copy it to the clipboard | Đã tìm thấy văn bản nhưng không sao chép được vào bộ nhớ tạm | yes |
| 7 | - | No text found | Không tìm thấy văn bản | yes |
| 8 | - | Reading text… | Đang nhận dạng văn bản… | yes |
| 9 | tool | Select text | Chọn vùng văn bản | yes |
| 10 | - | Read the text in a screenshot | Nhận dạng văn bản trong ảnh chụp màn hình | yes |

## Danh sách ứng dụng khách (AGT-2.AC15)

| # | Ngữ cảnh | Tiếng Anh | Tiếng Việt đề xuất | Fuzzy |
| --- | --- | --- | --- | --- |
| 11 | - | No clients yet. A client appears here as soon as it connects, or when you add it below. | Chưa có ứng dụng khách nào. Ứng dụng khách sẽ xuất hiện trong danh sách ngay khi kết nối hoặc khi bạn thêm ứng dụng đó ở bên dưới. | no |

## Ghi chú rà soát

- **#3** Giữ tên các mục cài đặt Windows bằng tiếng Anh theo bản gốc.
- **#4** Giữ nguyên lệnh cài đặt và dùng tên Tesseract nhất quán.
- **#8–10** Dùng “nhận dạng văn bản” cho thao tác đọc chữ trong ảnh. “Chọn vùng văn bản” mô tả thao tác khoanh một vùng trên ảnh.
- **#11** Dùng “ứng dụng khách” theo các nhãn hiện có trong phần cài đặt.
