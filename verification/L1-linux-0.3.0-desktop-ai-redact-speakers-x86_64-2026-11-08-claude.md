# L1 report: Auto-redact (tesseract helper) and the speaker engine (sherpa-onnx) on x86_64

- **Date:** 2026-11-08
- **Who:** Claude (desktop agent), over ssh; no screen needed (lab lock not required for these runs)
- **Machine:** Lenovo ThinkPad T490, Intel UHD 620, 8 threads, Ubuntu 24.04.5 LTS, x86_64, `tesseract 5.3.4` from the Ubuntu package (`/usr/bin/tesseract`, `eng` data)
- **Build:** debug build (`cargo test`, unoptimised) of branch `track/linux-desktop`; sherpa-onnx v2.0.1 static libraries fetched by `make sherpa` (linux-x64, SHA-256 checked by the script) and the two speaker models it downloads
- **Status:** PARTIAL

| Step | Result | Notes |
| --- | --- | --- |
| `make sherpa` on x86_64 | PASS | libraries and `pyannote-segmentation-3-0.onnx`, `campplus-sv-zh-en-16k-advanced.onnx` with licences unpacked; the Cargo build links them without downloading |
| `cargo test -p cm-render --test ocr_tesseract` (real `tesseract`) | PASS | 2 passed in 8.4 s: a light and a dark synthetic picture give an e-mail address, two phone numbers, a card number with expiry and code through `TesseractRecognizer` and `suggest_redactions`; a blank picture gives no lines. First time on x86_64 |
| `real_diarize` with the real models (`CM_SHERPA_MODELS`, `CM_DIARIZE_AUDIO`) | PASS with a limit | 35.2 s synthetic clip (macOS `say`: voices Samantha, Daniel, two turns each, 16 kHz mono, made on the Mac and pushed to the laptop). With `num_speakers=2`: 2 speakers, 4 turns, 20.0 s in a debug build (RTF 0.57), 17.3 s and 17.9 s per speaker, no turn under 0.5 s; a stop asked from the progress callback at 12.1 s returned after 19.0 s. With automatic speaker count: **1 speaker** (the engine merged the two text-to-speech voices). The clip is synthetic speech, so this shows the engine runs on x86_64 and separates two voices when told how many there are, not its accuracy on people |
| Missing `tesseract` is friendly | NOT RE-RUN | the message ("Auto-redact needs the tesseract program ...", with the `apt install` command, dimmed editor button) is covered by the earlier report and `crates/cm-daemon/tests/suggest_redactions.rs`; not repeated on the laptop (the package is installed there) |
| Auto-redact button in the editor, `ANN-2` on a real desktop | NOT RUN | |
| `redaction_bench` on x86_64 | NOT RUN | |
| aarch64 sherpa build | NOT RUN here (the VM was not used for this part) | |
| `llama-server` in the tarball | not mine (packaging) | |

## Findings

- `diarize` with an unknown speaker count found one speaker on two synthetic voices: not a product bug to fix here (the UI asks for the count or uses "auto"), but worth a look with real recordings in the 0.3.1 AI list.
