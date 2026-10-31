# L1 report: Auto-redact on Linux (tesseract helper)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04 aarch64, 4 CPUs shared with other agents, 8 GB. No GPU, no desktop session used. x86_64 not available.
- **Engine:** `tesseract 5.3.4` / leptonica 1.82.0 from the Ubuntu package `tesseract-ocr` (installed with `sudo apt install tesseract-ocr`; `eng` and `osd` data)
- **Build:** commit `5adb8b7` plus the later docs commits; `cargo build/test/clippy -p cm-render --no-default-features`, debug for tests, release for the benchmark and the `ocr_file` example, `CARGO_INCREMENTAL=0`, target dir `/home/dev/cm-ocr-target`
- **Decision note:** `specs/plans/auto-redact-linux-plan.md`. Benchmark: `verification/benchmarks/redaction-0.2.5-linux-tesseract-dev-2026-10-31.md`.
- **Data:** all synthetic: the project's seeded benchmark set (dev split only; `held_out` not read) and nine pictures made with ImageMagick `convert` (DejaVu Sans and Mono, 9 to 14 px, light, dark and mixed backgrounds, invented data). Nothing was downloaded other than the `tesseract-ocr` apt package.

## Before (step 1): what Linux did

Read from the code at `c4dab9b`: the editor did not draw the Auto-redact button (`redact::available()` false) and the inspector card never showed; `suggest_redactions` (MCP) and `capturemann annotate --suggest-redactions` (CLI) answered `not_implemented` ("text recognition is not available on this machine ..."); `redaction_bench run` stopped with "Apple Vision exists on macOS only"; no diagnostics line. Details in the plan, section 1. This was read from the code, not run on Linux before the change.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Unit tests of the new module on Linux (`cargo test -p cm-render --lib`) | PASS | 92 passed, 1 ignored (the 5K blur timing test); includes TSV parsing, geometric line joining, reading order on 300 staggered lines (no panic), lookup order of `CM_TESSERACT` / exe folder / `PATH`, missing engine says what to install, dark pictures inverted and scaled |
| 2 Integration test `tests/ocr_tesseract.rs` with the engine installed | PASS | 2 passed in 1.8 s: a light and a dark picture each give an email, two phone numbers, a card and its expiry and code through `TesseractRecognizer` + `suggest_redactions`; a blank picture gives no lines |
| 3 Same test without the engine | PASS (SKIPPED) | on macOS (no `tesseract` installed) the two tests print `SKIPPED: text recognition needs the tesseract program ...` and pass; on Linux the engine was moved away for step 6 only |
| 4 `cargo clippy -p cm-render --all-targets -- -D warnings` on Linux and on macOS; `cargo clippy -p cm-daemon`; `cargo check -p cm-desktop --no-default-features` on macOS | PASS | `cm-desktop` was not compiled on Linux (disk and time on the shared VM); the code added there has no `cfg(target_os)` and compiled on macOS. `python3 scripts/check_target_os.py` ok (no new `target_os` lines outside the existing `system_recognizer`) |
| 5 Full pipeline on 9 ImageMagick pictures (`examples/ocr_file`) | PASS with limits | see the table below |
| 6 Engine missing, three ways | PASS | `CM_TESSERACT=/nonexistent`: "CM_TESSERACT points to `/nonexistent`, which is not a file; fix or unset it", exit 1. `tesseract` moved away from `PATH`: the diagnostics line and the error say `sudo apt install tesseract-ocr` (and the Fedora and Arch commands) and `CM_TESSERACT`; exit 1. Engine restored afterwards (checked) |
| 7 Diagnostics line (`cm_render::ocr::describe_system`, used by `get_diagnostics`) | PASS (the function; the daemon call was not run on Linux) | "text recognizer: tesseract at /usr/bin/tesseract (tesseract 5.3.4)" or "text recognizer: not available: <instruction>" |
| 8 Daemon error path: `suggest_redactions` with a missing helper | PASS (tested with a recognizer that has no engine) | `crates/cm-daemon/tests/suggest_redactions.rs::a_missing_helper_program_names_what_to_install`: `not_implemented`, the message contains `apt install tesseract-ocr` and "annotate"; ran on macOS |
| 9 Benchmark harness on Linux with the real recognizer (`redaction_bench run --recognizer system --split dev`) | PASS (runs) | 211 images in 346 s (shared CPUs); found 84.5 %, precision 92.7 %, 0.431 false alarms per image, leak rate 4.4 %; mean 956 ms and p95 1.9 s per image. The macOS path and the harness code are unchanged (only the error message changed) |
| 10 Editor button state on Linux | not run | no desktop session; the dimmed button (`editor-auto-redact-unavailable`, disabled, tooltip = the install sentence, translatable, Vietnamese added) was only compiled on macOS, where the recognizer exists so the branch is not taken |
| 11 `tests/manual/ANN-2.md` on a real Linux desktop | not run | step 9 of the script now describes the Linux cases |
| 12 x86_64, other distributions, a `tesseract` 4.x or 5.5 | not run | only 5.3.4 on aarch64 |
| 13 `cargo run -p cm-i18n -- extract` and `check` | PASS | 2649 messages; one new message (`Auto-redact needs the tesseract program ...`), Vietnamese added (marked fuzzy like the rest of the catalogue until reviewed) |

### Step 5: the nine ImageMagick pictures (synthetic; 9 items in each of 8 pictures, none in the ninth)

Each picture holds: an email, two phone numbers, a card number with expiry and CVV (3 items), an SSN, an IPv4 address and an API key (9 items). Pictures: light 14, 11 and 9 px; dark 14 and 11 px; dark 9 px mono; light 12 px mono; light 11 px with a dark sidebar (mixed); and a no-secret picture of ordinary sentences.

| Result | Count |
| --- | --- |
| Items with a proposal on them | 70 of 72 |
| Items fully covered (the benchmark's 90 % rule) | 66 of 72: the API key was split by the recogniser into two lines in 3 pictures (only the first part boxed) and cut short in the 9 px mono picture |
| Misses | the email and the expiry date in the dark 9 px mono picture (recognised as `jane.doegexampLe.` / `com` and `64/29`) |
| Proposals not on an item | 0 of 70 |
| Proposals on the no-secret picture | 0 |
| Card number whose spaces the recogniser partly dropped (`4111 111111111111`) | still found |

This is a handful of pictures made by the author of the code: it shows the chain works end to end (pixels, tesseract, line joining, detector, boxes in original pixels) and where it breaks (a key the recogniser splits is only partly covered, which leaves its tail readable). It is not an accuracy claim; the benchmark above is the larger synthetic figure, and neither says anything about real screenshots.

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| ANN-2.AC4 (Auto-redact proposes, applies nothing) on Linux | PARTIAL | proposals through the real recognizer: steps 2, 5, 9. Daemon, CLI and MCP paths are the shared ones tested with fakes (macOS); the editor was not opened on Linux |
| RED-1 (benchmark on the platform) | PARTIAL | dev split only; no held-out; no real screenshots (`tests/manual/RED-1.md` not run) |
| 100 % local | PASS by construction, not by a network trace | the helper reads stdin and writes stdout; no sockets in `tesseract.rs`; not checked with a packet capture |

## Problems found

- A comparison in the first version of the line ordering was not a total order, so `sort_by` could panic on some pictures ("user-provided comparison function does not correctly implement a total order"). Found by the benchmark, fixed in `1c15aa7` with a regression test.
- A key that tesseract splits at a wrong space is boxed only in its first part. General limit of recognising text, not special-cased.
- Recall is clearly below Apple Vision's on the same synthetic set (84.5 % against 95.3 %), most on MBI, VIN, passport, email and card expiry/code.
- Disk on the shared VM dropped to 4.4 GB free at one point because of other agents' target directories (`cm-pkg2-target` 6.7 GB, `cm-ed-target` 3.2 GB); this work used about 1.5 GB (`cm-ocr-target`, the 95 MB set), removed at the end.

## Verdict

PARTIAL. The recognizer, discovery, error messages, diagnostics line and benchmark run work on aarch64 Ubuntu 24.04 with the distro `tesseract`. Not done: the editor on a real desktop, the manual script, x86_64, packaging or bundling the engine, a recognizer that ships inside the app. Not a compliance tool; synthetic data only.
