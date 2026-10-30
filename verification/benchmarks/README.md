# Benchmarks

Measured comparisons that other people can read, repeat and build on. They differ from the
reports one level up (`verification/*.md`), which record that a manual script was run and passed:
a benchmark answers "which option is better, by how much, on what data". Both are stored so a
status can point at them; neither is a promise about every recording.

A benchmark report has these parts, in this order:

1. **Question and short answer.** What was compared, and the result in a few lines.
2. **Setup.** Date, git commit, machine, build, tools and the exact commands to repeat it.
3. **Data.** Where each reference came from, its licence, how clips were cut, and which clips were
   used for choosing settings and which were held back. Public data only: never audio, text or
   turn lists from a person's own recordings (numbers from such a run may be quoted, the files
   stay with their owner).
4. **Results.** Full tables, including options that were rejected and why.
5. **What this does not show.** Sample size, language, recording conditions, anything chosen by
   looking at the same data it is judged on.
6. **Decision and follow-up.** What the project does with it, and what is still open.

Wording rule (see `CLAUDE.md`): say "measured" with the numbers here; "verified" only for a
manual script with a report in `verification/`.

| Report | Question |
| --- | --- |
| [`speaker-detection-2026-10-29.md`](speaker-detection-2026-10-29.md) | How well does speaker detection work today, and do other voice models and thresholds do better? (English, 14 clips) |
| [`performance-m4pro-2026-10-29.md`](performance-m4pro-2026-10-29.md) | How fast are transcription and speaker finding by model, thread count and length, and what did the two PERF-1 fixes change? (Apple M4 Pro) |
| [`text-model-report-translation-m4pro-2026-10-30.md`](text-model-report-translation-m4pro-2026-10-30.md) | How long do reports and translation take with the built-in text model from 11 minutes to 2.5 hours, and where does it break? (Apple M4 Pro, stand-in long transcripts) |

Tools used: `crates/cm-ai/src/measure.rs` (word and diarization error rates, with tests),
`crates/cm-ai/examples/measure.rs` and `baseline.rs`, `tests/tools/make_reference_clips.py`, `tests/tools/perf_matrix.py` and `crates/cm-ai/examples/perf.rs` (speech and speakers), `tests/tools/text_bench_tile.py` (long stand-in transcripts for reports and translation).
