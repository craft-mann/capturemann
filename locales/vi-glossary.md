# Vietnamese glossary (DRAFT for owner review, Oct 23, 2026)

Every Vietnamese string in `vi.po` is written against this table, so one decision here saves many edits later.
Mark a term you want changed and the whole catalogue follows.

## Style (proposal)

- **Tone:** short and neutral, like macOS and the other Mac apps. Buttons and labels are verbs or nouns with no
  pronouns ("Lưu", "Mở thư mục", "Xuất lại").
- **Addressing the user:** only when a sentence needs it, use "bạn" (never "quý khách", "anh/chị"). Prefer
  sentences without a subject: "Chưa có bản ghi nào." rather than "Bạn chưa có bản ghi nào."
- **Errors:** say what happened and what to do, in one sentence: "Không thể lưu — ổ đĩa đã đầy."
  The curly apostrophe forms ("Couldn't …") become "Không thể …".
- **Keep in English** (as the brand/protocol name or because Vietnamese users know it so): CaptureMann, YouTube,
  Google Drive, OneDrive, Dropbox, MCP, CLI, Whisper, FFmpeg, OAuth, API, GIF, PNG, JPEG, TIFF, MP4, WebM, SRT, VTT,
  fps, Wi-Fi, ID, URL, Unlisted/Public/Private as shown by YouTube (translated in a parenthesis only when it helps).
- **Keys and shortcuts:** glyphs stay (⌘ ⌥ ⇧ ⌃), key names stay in English on keycaps (Esc, Enter, Tab).
- **Quotation marks:** curly “…” for quoted UI labels, titles and accounts everywhere.
- **Capitalisation:** sentence case ("Mở thư mục"), not Title Case.
- **Numbers:** decimal comma (2,4 MB), thousands separator is a dot or a space (1.234); dates as "12 thg 10".
- **Plurals:** Vietnamese has one form; `{n} bản ghi` for 1 and for 5.

## Terms

| English | Vietnamese | Note |
|---|---|---|
| recording (the thing) | bản ghi | alternatives: video quay màn hình. "bản ghi" is short and neutral |
| to record / Record | Quay | the Record button; macOS uses "Ghi" but "Quay" is what people say for screen video. Alternative: "Ghi" |
| screenshot | ảnh chụp màn hình | short form "ảnh chụp" when the context is clear |
| capture (the thing, either kind) | bản chụp | "All captures" = "Tất cả bản chụp"; alternative "mục đã ghi" |
| capture (verb, a screenshot) | Chụp | |
| library | Thư viện | |
| screen | màn hình | |
| display (a monitor) | màn hình | "Display 2" = "Màn hình 2" |
| window | cửa sổ | |
| region / area | vùng | "Capture a region" = "Chụp một vùng" |
| full screen | toàn màn hình | |
| camera / webcam | camera / webcam | keep "webcam" (common) |
| bubble (camera) | bong bóng camera | |
| microphone / mic | micrô | "mic" for tight labels |
| system audio | âm thanh hệ thống | |
| cursor | con trỏ | |
| click | nhấp | |
| zoom (auto-zoom) | thu phóng | "Zoom spots" = "Điểm thu phóng" |
| trim | cắt đầu cuối | label "Cắt" |
| cut (remove a section) | xoá đoạn | label "Xoá đoạn" |
| split | tách | |
| speed | tốc độ | |
| playhead | đầu phát | |
| timeline | dòng thời gian | |
| caption(s) | phụ đề | |
| transcript | bản chép lời | the result of transcription (owner review, Oct 24, 2026; was "bản ghi lời nói") |
| to transcribe | chép lời | the action ("chép lời bằng AI") |
| silence | khoảng lặng | |
| track (media track) | rãnh | "rãnh micrô", "rãnh màn hình"; "luồng" only where a stream is meant |
| blocked (state) / Block (button) | Bị chặn / Chặn | a state label uses "Bị chặn", an action "Chặn" |
| client (an app that connects over MCP or the CLI) | ứng dụng khách | "client ID" and "client secret" of OAuth stay in English |
| online copy | bản sao trực tuyến | also "Remote" in the delete dialog; not "bản sao từ xa" |
| telemetry | dữ liệu đo lường | "Không thu thập dữ liệu đo lường" |
| System Settings (macOS) | Cài đặt Hệ thống | Apple's own capitalisation, in prose and paths |
| filler words | từ đệm | |
| annotation(s) | chú thích | |
| arrow / rectangle / ellipse / text / pen | mũi tên / hình chữ nhật / hình elip / văn bản / bút | |
| blur / redact | làm mờ / che | "Auto-redact" = "Tự động che" |
| highlight | tô sáng | |
| export | Xuất | "Export again" = "Xuất lại" |
| publish | Đăng | |
| upload | Tải lên | |
| download | Tải xuống | |
| destination | Điểm lưu | for storage; "Publish destination" = "Nơi đăng" |
| storage / save to | Lưu vào | |
| folder | thư mục | |
| file | tệp | |
| link / copy link | liên kết / Sao chép liên kết | |
| account | tài khoản | |
| channel (YouTube) | kênh | |
| playlist | danh sách phát | |
| privacy: Private / Unlisted / Public | Riêng tư / Không công khai / Công khai | YouTube's own Vietnamese wording |
| title / description / tags | tiêu đề / mô tả / thẻ | |
| thumbnail | ảnh bìa | |
| settings | Cài đặt | |
| permissions | Quyền truy cập | |
| privacy (page) | Quyền riêng tư | |
| shortcuts | Phím tắt | |
| agent(s) | tác nhân AI | "Agents & CLI" = "Tác nhân & CLI" |
| AI models | Mô hình AI | |
| connected / disconnect / reconnect | đã kết nối / Ngắt kết nối / Kết nối lại | |
| sign in | Đăng nhập | |
| allow / deny | Cho phép / Từ chối | |
| ask first | Hỏi trước | agent policy |
| delete / remove | Xoá / Gỡ | delete = erase data; remove = detach (account, model) |
| discard | Bỏ | "Discard take" = "Bỏ bản quay" |
| undo / redo | Hoàn tác / Làm lại | |
| cancel / done / close | Huỷ / Xong / Đóng | |
| save / saved | Lưu / Đã lưu | |
| copy / copied | Sao chép / Đã sao chép | |
| open / reveal in Finder | Mở / Hiện trong Finder | keep "Finder" |
| retry | Thử lại | |
| pause / resume / stop | Tạm dừng / Tiếp tục / Dừng | |
| countdown | đếm ngược | |
| delay | độ trễ | "Before a screenshot" = "Trước khi chụp" |
| quality / size / codec | chất lượng / kích thước / codec | |
| theme: system / light / dark | hệ thống / sáng / tối | |
| language | Ngôn ngữ | |
| coming soon | Sắp có | |
| unavailable / available | không khả dụng / khả dụng | |
| needs attention | cần chú ý | tray tooltip |
| take (one recording attempt) | bản quay | |
| session | phiên | |

## Facts to translate faithfully (owner review required)

The Privacy page facts, the permission explanations, the "no telemetry" line, the AI privacy notices and the
confirmation dialogs state what the app really does. Their Vietnamese is marked `#. FACT` in `vi.po`; please read
every one of them.
