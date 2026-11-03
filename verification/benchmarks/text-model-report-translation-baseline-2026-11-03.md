# Built-in text model, reports and translation, speed and floor check (0.2.7, M2c baseline)

- **Date:** 2026-11-03. **Status:** measured, one run per cell (the two `minutes` cells twice), one machine, synthetic transcripts. Not "verified".
- **Machine:** Apple M4 Pro, 14 cores, 48 GB, macOS 26.6.2.
- **Build:** branch `track/mac-ai`, release build of `crates/cm-ai/examples/text_bench.rs` (commit `6ba37ac` plus the rubric commits that follow it; the report and translation code is the shipped 0.2.6 code). Engine: the `llama-server` inside the installed `CaptureMann.app` (read-only use), model Gemma 4 E2B Instruct Q4_0 (the installed file), context 16,384, one slot, all layers on the GPU (the app's own `engine_args`).
- **Question:** how fast (tokens per second, wall time) are the report and translation paths of the current text model on a fixed transcript set, and does a stored report pass a floor check (times exist, speakers present, nothing invented)?

## 1. Short answer

- **Speed (measured on this Mac):** generation **72 to 98 tokens/s**, prompt processing **800 to 1,360 tokens/s** (the server's own log). A `summary` of a 4-minute (739 words) transcript took **9.0 s**, of a 14-minute (2,481 words) transcript **12.9 s**. A `minutes` report took **44.7 s** (4 min) and **268 to 309 s** (14 min, 7 model calls, 21,527 generated tokens, 182 items): the longer template wrote about 21 thousand tokens for a 14-minute meeting.
- **Translation English to Vietnamese:** 27 caption lines in **16.6 s** (2 calls); 75 lines in **104.2 s** (41 calls; 3 lines left untranslated). Neither stopped with `output_too_long`.
- **Floor check (`report::rubric`):** 3 of 4 report cells pass; `summary` of the 6-voice transcript named **5 of 6 speakers**. The `minutes` report of that transcript had one capitalised word (`Updates`) that the transcript does not contain. The check measures nothing about whether the content is right.

## 2. Setup and how to repeat it

```sh
export SHERPA_ONNX_LIB_DIR=<packaging/sherpa/out/lib>
cargo build --release -p cm-ai --example text_bench
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn3 --speakers 3 --minutes 4  --seed 2
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name syn6 --speakers 6 --minutes 14 --seed 4
target/release/examples/text_bench --task report --template summary|minutes --lines <dir>/syn3.lines.json \
  --model <gemma-4-E2B-it-Q4_0.gguf> --model-id gemma-4-e2b --llama-server <llama-server> --label syn3 --content-out <file>
target/release/examples/text_bench --task translate --target Vietnamese --lines <dir>/syn3.lines.json --model ... (same flags)
target/release/examples/text_bench --task report --template minutes --lines ... --rescore <stored content.json>   # rubric only, no model
```

Time is wall clock around `generate` / `translate_with_text_model` (the server was started and warmed by a first tiny request, 1.3 to 6.7 s, reported apart in the JSON). Tokens and tokens/s are the totals of the server's own `prompt eval time` and `eval time` log lines for that process (the warm-up request adds a few tokens). `calls` counts requests to the model. The rubric (`crates/cm-ai/src/report/rubric.rs`, nine unit tests) scores the stored report content: every model-section item has an `at` inside a transcript line (tolerance 1 s); every speaker of the transcript appears in the report text; every number the report states (digits, or spelled numbers starting with a ten, hundred or thousand; ordinals and "one/two" are ignored) is a number of the transcript (digits or spelled out); every capitalised word inside a sentence (not headings: short or Title Case strings) is a word of the transcript, a speaker or an attendee, or appears in lower case elsewhere in the report.

## 3. Data

Two synthetic English transcripts from `tests/tools/make_synthetic_meeting.py` (the same clips as the transcription and speaker baselines): `syn3` 27 caption lines, 739 words, 3 speakers, 244 s; `syn6` 75 lines, 2,481 words, 6 speakers, 844 s. Speakers are labelled "Speaker 1" and so on. The text is machine-made from word lists (repetitive in shape, many numbers and first names): it measures speed and the floor check, never summary quality. No private recording or text. The 2026-10-30 report remains the reference for hour-long stand-in transcripts and memory.

## 4. Results

| Task | Transcript | Wall s | Model calls | Generated tokens (tok/s) | Prompt tokens (tok/s) | Items | Floor |
|---|---|---|---|---|---|---|---|
| report `summary` | syn3 | 9.0 | 1 | 743 (98.2) | 1,715 (797) | 14 | pass |
| report `minutes` | syn3 | 44.7 and 45.2 | 1 | 3,782 (86 to 87) | 2,176 (1,358 to 1,362) | 66 | pass |
| report `summary` | syn6 | 12.9 | 1 | 859 (89.9) | 4,251 (1,200) | 15 | fail: 5 of 6 speakers named |
| report `minutes` | syn6 | 309.3 and 267.9 | 7 | 21,527 (72.3 and 83.4) | 12,478 (1,072 and 1,249) | 182 | fail: 1 invented capitalised word (`Updates`) |
| translate to Vietnamese | syn3 | 16.6 | 2 | 1,206 (76.6) | 1,381 (1,236) | 27 lines, 0 untranslated | |
| translate to Vietnamese | syn6 | 104.2 | 41 | 7,397 (81.9) | 16,401 (1,189) | 75 lines, 3 untranslated | |

All report cells: `dropped_uncited` 0, `reassigned_names` 0 (the product's own safety net never fired), 0 items without a valid time, 0 invented numbers. A first run of the rubric (before it skipped headings) flagged 8 capitalised heading words per `minutes` report; those were false alarms of the checker, fixed and rescored from the stored content (a unit test pins the rule).

## 5. What this does not show

- **Synthetic transcripts.** Generation speed depends on the output, which here is long and list-like. A real meeting may give shorter or longer replies. No summary quality is measured; the floor check cannot tell a right claim from a wrong one, and a report that names every speaker can still be wrong.
- One run per cell (two for `minutes`: wall time differed by 15 % between the two runs of the 14-minute cell with identical output, so single-run differences under that are noise).
- Translation was English to Vietnamese; the quality of the Vietnamese was not read or scored. The 41 calls for 75 lines include retries of batches whose line count did not match; their cause was not examined.
- Memory of `llama-server` was not sampled in this run (the 2026-10-30 report has it: 3.2 to 5.0 GB).
- The `output_too_long` failure of performance.md item 5 did not occur on these inputs, so the planned fix (split and retry) cannot be judged on this set; it needs the stand-in 1 h and 2.5 h transcripts of the earlier report.
- Intel: not measured. The installed model is the Q4_0 file the app pins; no other quantisation was run (O2).

## 6. Decision and follow-up

- Baseline stored. Candidate speed-ups (prompt reuse across pieces, smaller context for short pieces) are judged on: wall time and tokens/s above, with the rubric not getting worse.
- Open: re-run the hour-long stand-ins for the translation split-and-retry fix; add 2 or 3 more fixed transcripts (a 2-speaker one, one with an attendee list in the context) before week 2's comparison.
