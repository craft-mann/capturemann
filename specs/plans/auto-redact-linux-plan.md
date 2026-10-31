# Auto-redact on Linux: choosing a text recognizer (RED-1, Linux column)

Status: **decided and built in the early Linux build (Oct 31, 2026)**; the maintainer can still overrule the choice
(see "What the maintainer decides"). Rules that apply: on-device only, no network at redaction time, models only
after the user agrees, no GPL/AGPL in shipped binaries, general techniques only (no phrase lists or one-image fixes),
nothing is called "verified" without a stored report. Builds on `specs/plans/release-0.2.5-plan.md` (the 21 kinds and
the benchmark) and `specs/features/17-redaction.md`.

## 1. What happened on Linux before this change

Auto-redact has three entry points; all go through the `suggest_redactions` action and the `TextRecognizer` trait in
`crates/cm-render/src/ocr/`. Before this change `system_recognizer()` returned the `Unavailable` stub on every
platform except macOS.

| Entry point | Behaviour on Linux before |
|---|---|
| Editor (desktop app) | `redact::available()` was false, so the **Auto-redact button was not drawn at all** (toolbar) and the inspector card (kinds chooser, Accept all, Dismiss) never showed. No explanation anywhere. |
| MCP `suggest_redactions` / CLI `capturemann annotate <id> --suggest-redactions` (and `--kinds`, `--redact-emails`) | The daemon checked `recognizer.is_available()` first and answered **`not_implemented`**: "text recognition is not available on this machine, so redactions cannot be suggested; blur regions yourself with `annotate`". The CLI maps that code to its exit code for `not_implemented`; MCP returns it as the tool error. Nothing was read or written. |
| `redaction_bench run` | Stopped with "the system text recognizer is not available on this platform (Apple Vision exists on macOS only)"; only `--detector-only` (perfect text, no OCR) ran. |
| `get_diagnostics` | No line about text recognition. |

The detector itself (21 kinds, regexes, checksums) is portable and was already tested headless with scripted lines;
only "pixels to text lines with boxes" was missing.

## 2. The choice: the system `tesseract` program as a helper process

`crates/cm-render/src/ocr/tesseract.rs` runs `tesseract stdin stdout -l eng --psm 6 tsv`, feeds it a grayscale PNG
on stdin and reads word boxes from the TSV. It is found like the other helper programs: `CM_TESSERACT` (file or
folder), next to the executable (so a package may ship its own copy), then `PATH`. `CM_TESSDATA` points at a folder
with `eng.traineddata`. Nothing is linked into CaptureMann.

| Option | Licence | Accuracy on small UI text | Weight and build | Models and consent | Verdict |
|---|---|---|---|---|---|
| **`tesseract` binary as helper** | Tesseract Apache-2.0, Leptonica BSD-2-clause, `eng` data Apache-2.0. An external program is not part of our binary: nothing to audit in `cargo deny`, nothing linked. | Measured on the synthetic set (text 9 to 18 px, light and dark): 84.5 % of the items found and 92.7 % precision with the defaults, against 95.3 % and 92.0 % for Apple Vision on the same set; weakest on private-key blocks, MBI, VIN and IBAN (see the benchmark report). | 0 bytes in the app; the distro package is about 30 MB with `eng` data. No build step; works on aarch64 and x86_64 as the distro ships it. | The data comes with the package the user installs (`sudo apt install tesseract-ocr`), so the app **downloads nothing**; the consent rule is met because the user installs it. | **Chosen.** Simplest, safest licence story, testable in the VM, same pattern as `ffmpeg` and `llama-server`. |
| `tesseract-rs` / `leptess` (bindings) | The binding crates are MIT (as recalled, not re-checked); they link `libtesseract` and `libleptonica`. | Same engine. | Needs the C++ libraries and headers at build time on every platform, or a source build of both (cmake, long). CI and the cross builds get heavier; a bundled copy adds about 5 to 10 MB plus data. | `eng.traineddata` still has to be found or downloaded. | Rejected for now: all the cost of a native dependency for the same accuracy as the helper. Revisit if we want to ship OCR inside the Linux tarball. |
| `ocrs` (pure Rust, `rten`) | Code MIT/Apache-2.0 (as recalled, not re-checked). The two model files' licence has to be read before shipping. | Made for screenshots and scans; unmeasured here. Its recognition models are Latin-script, with no per-word confidence of the kind we use. | Small binary, but a new inference runtime next to ONNX Runtime (inside sherpa-onnx) and ggml (whisper.cpp, llama-server). | Two model files (about 10 MB) would need the model store, a consent step and pinned hashes; not built. | The best candidate for a later "ships inside the app" version. Not chosen now because it needs a model store entry, a pinned download and a licence review that we cannot do offline, and the helper already works. |
| PaddleOCR through `ort` | Models Apache-2.0; `ort` is MIT/Apache-2.0 but pulls the ONNX Runtime shared library (tens of MB). | Best of the open options on small text, in general. | A second ONNX Runtime next to the one sherpa-onnx already links statically: duplicate-symbol and size risk. Detection, direction and recognition models, about 15 to 20 MB. | Same model store and consent work as `ocrs`, more of it. | Rejected for 0.2.x: weight and the runtime clash. |
| Linux desktop APIs | There is no OCR service in GNOME, KDE or the portals. | | | | Not an option. |

### Consequences, stated plainly

- On a Linux machine without `tesseract` Auto-redact is **unavailable until the user installs it**. The desktop app
  shows the button dimmed with the install command as its tooltip; the CLI and MCP answer `not_implemented` with the
  same instruction (not a generic message); `get_diagnostics` has a "text recognizer" line.
- There is **no model download hook** because there is no model for us to download: the user's package manager
  installs the data. If a later release ships `ocrs` or PaddleOCR, it reuses the `cm-ai` model store (user-started,
  resumable, SHA-256 checked) and the consent dialog.
- The same code is used on Windows when `tesseract.exe` is on `PATH` or `CM_TESSERACT` is set. `Windows.Media.Ocr` is
  still the planned recognizer there (RED-1, Windows column); nothing changes on macOS (Apple Vision stays the only
  recognizer; the benchmark numbers in `verification/benchmarks/` are unaffected).
- Accuracy is lower than Apple Vision's. The benchmark harness runs unchanged on Linux and its report is stored next to
  the macOS ones; all of it is synthetic data and says nothing about real screenshots.
- The 100% local rule holds: the helper reads the picture from stdin, writes to stdout, and has no network code. The
  picture is never written to disk by us.

### Preparation steps (general, compared on the dev split of the synthetic set)

- grayscale, 3x Catmull-Rom up-scaling (tesseract is trained for text about 30 px high; UI text is 9 to 14 px),
  reduced automatically so the scaled picture stays under 40 megapixels (a 4K screenshot is read at 2x);
- invert when the mean brightness is below 110/255 (mostly dark pictures), because the LSTM engine reads
  dark-on-light best (the dev results are the same for dark and light pictures);
- page segmentation mode 6 (one block of text), then join words into lines by geometry in our code (tesseract's own
  line numbers split phone numbers and keys at spaces and column edges);
- `OMP_THREAD_LIMIT=1` (threads make it slower on screenshots) and a 180 s timeout that kills the helper.

Eight combinations of scale and mode were run on the `dev` split (211 images, recall any kind / precision):
scale 1: 64.1 % / 86.4 %; scale 2 (modes 11, 3, 6): 78.5 / 90.9, 78.8 / 91.4, 80.9 / 92.3; scale 3 (modes 11, 3, 6):
83.4 / 90.9, 82.5 / 91.5, **84.5 / 92.7**; scale 4 mode 6: 83.8 / 91.3. Scale 3 with mode 6 is the default; it also
costs about 1 s per image (mean 958 ms, p95 1.9 s at about 1000 x 700 px, on a shared 4-CPU VM). The `held_out` split
was not read. `CM_TESSERACT_SCALE` and `CM_TESSERACT_PSM` exist to repeat such a comparison.

## 3. What the maintainer decides or supplies

1. Whether the Linux package depends on (or recommends) the distro `tesseract-ocr` package, or ships its own copy next
   to the executable (bundled build: Apache-2.0 notice, Leptonica notice, `eng.traineddata`, a build script in
   `packaging/`; not done here).
2. Whether to invest in the pure-Rust route (`ocrs`) for an in-app recognizer: needs the licence check of the model files
   and the pinned download URL and hashes.
3. Whether the Windows build should also look for `tesseract.exe` before `Windows.Media.Ocr` lands (it does today, silently).
4. A run of `tests/manual/ANN-2.md` on a real Linux desktop (the VM has no GPU, so the editor was not opened).
