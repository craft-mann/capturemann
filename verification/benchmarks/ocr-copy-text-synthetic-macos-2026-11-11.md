# Copy text (ANN-5): Apple Vision on three synthetic lines of Vietnamese and two of English, 2026-11-11

## Question and short answer

Does Apple Vision, as `cm_render::ocr::system_recognizer` uses it, read Vietnamese with tone marks, and does
setting `automaticallyDetectsLanguage` change anything? Measured on one tiny synthetic image: the plain Vietnamese
sentences were read exactly; the line of stacked tone marks (`ắ ằ ẳ ẵ ặ ế ề ể ễ ệ`) had errors; character error rate
(CER, Levenshtein over characters divided by the expected length) 0.078 for the three Vietnamese lines and 0.000
for two English lines. The setting made **no difference** on this input (same output, same CER).

## Setup

- Date Nov 11, 2026; commit `98fd91b0` plus the working changes of branch `track/ocr`; Apple M4 Pro, macOS 26.6.2.
- Build: `cargo test --locked -p cm-render --test ocr_text_real`; real engine, no download, no screen, camera or microphone.
- Command: `CM_OCR_REAL=1 cargo test --locked -p cm-render --test ocr_text_real -- --ignored --nocapture --test-threads=1`
- Run twice: first with the Vision request left at its defaults, then with `setAutomaticallyDetectsLanguage:` set to true
  (guarded by `respondsToSelector`; `crates/cm-render/src/ocr/apple.rs`).

## Data

Synthetic: text drawn by the project's own renderer (`text` ops, size 34) on a white 1500 px wide image, written for this
check (no real content). Expected text:

```
Save the file before closing / Error 404: page not found
Xin chào, đây là ảnh chụp màn hình
Tiếng Việt có dấu: ắ ằ ẳ ẵ ặ ế ề ể ễ ệ
Người dùng không thể lưu tệp
```

## Results

| Input | Defaults | `automaticallyDetectsLanguage` |
| --- | --- | --- |
| English, 2 lines (paragraph gap kept) | CER 0.000 | CER 0.000 |
| Vietnamese, 3 lines | CER 0.078 | CER 0.078 (identical text) |

Vietnamese output: `Xin chào, đây là ảnh chụp màn hình` / `Tiếng Việt có dấu: ă ằ ắ ẵ ặếê ểếệ` / `Người dùng không thể lưu tệp`.

## What this does not show

One image, one font, one size, one machine, one macOS version (26.6.2). The macOS floor of the app (13) was **not**
run: whether Vision reads Vietnamese there, and whether the selector exists, is unchecked (the selector is guarded, so
an older Mac keeps the defaults). The stacked tone-mark line is an unnatural stress test. Not measured: the redaction
benchmark screens, Windows, Linux `tesseract`, handwriting, photos of text, other languages. Nothing here is an accuracy
claim for real screenshots.

## Decision and follow-up

`automaticallyDetectsLanguage` is kept (guarded, no measured harm, may help other scripts) but is not credited with
any improvement. A larger Vietnamese and English set, and the 13/14 floor, are open.
