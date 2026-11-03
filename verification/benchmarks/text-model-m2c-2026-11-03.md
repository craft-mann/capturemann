# Reports and translation with the built-in text model: split-and-retry, prompt reuse, rubric (0.2.7, M2c)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, Gemma 4 E2B Q4_0 (the installed file), context 16,384, the app's `llama-server`. Not "verified". Synthetic transcripts only (`make_synthetic_meeting.py`): a short one of 20 s (2 voices, 47 words, 2 lines) and a one-hour one (6 voices, 9,947 words, 299 lines). Floor check = `report::rubric` (times that exist, every speaker named, no invented numbers or names); it is not accuracy.

## 1. Short answer
- **Translation no longer dies on a reply that reaches the output limit.** A reply that hits the limit halves the batch (20, 10, 5, ...) from the same line; finished batches are kept; one line that still cannot be written stays in the source language, flagged. Unit test `output_too_long_halves_the_batch_and_keeps_the_work`. The one-hour English to Vietnamese translation (299 lines) finished: **694 s, 225 calls, 18 lines left untranslated, no failure**.
- **Prompt and KV reuse: not worth building.** Generation dominates: prompt processing is **11.5 % of the one-hour translation** (80 s of 694 s), 15 to 17 % of the short reports, **26 % of the one-hour `summary`** (15 of 57 s) and **5 % of the one-hour `minutes`** (82 of 1,666 s). Even a perfect reuse of the shared prefix (system text and template instructions, about 600 tokens of each prompt) would save a small part of those shares. Not measured: whether `llama-server` already reuses the prefix by itself.
- **A real bug found: the one-hour `minutes` report failed after 28 minutes.** The merge pass sent two big notes (24,250 tokens) to a 16,384-token context and the server answered HTTP 400; the report died with the work of 29 calls and 90,867 generated tokens lost. Fixed: a request that does not fit is handled like a reply that is too long (the notes stay side by side). Unit test `a_merge_request_that_exceeds_the_context_keeps_the_notes_side_by_side`. **The one-hour `minutes` run has not been repeated after the fix** (it takes about 28 minutes).
- The rubric floor passes on every report that finished except the 6-voice `summary` of the 14-minute transcript (5 of 6 speakers named, in the earlier baseline).

## 2. Results (short row = 20 s, one-hour row)

| Task | Transcript | Wall s | Calls | Generated tokens (tok/s) | Prompt tokens (tok/s) | Prompt share | Floor |
|---|---|---|---|---|---|---|---|
| report `summary` | 20 s | 3.2 | 1 | 283 (100.5) | 646 (1,147) | 17 % | pass (5 items, 2 of 2 speakers) |
| report `minutes` | 20 s | 5.5 | 1 | 486 (99.5) | 1,107 (1,274) | 15 % | pass (9 items, 2 of 2) |
| translate to Vietnamese | 20 s, 2 lines | 1.1 | 1 | 101 (103.5) | 266 (900) | 23 % | 0 untranslated |
| report `summary` | 1 h | 56.6 | 5 | 4,028 (96.2) | 19,985 (1,337) | 26 % | pass (15 items, 6 of 6) |
| report `minutes` | 1 h | **failed at 1,666 s** (HTTP 400, merge pass) | 29 | 90,867 (57.4) | 74,997 (914) | 5 % | not scored; fixed, not re-run |
| translate to Vietnamese | 1 h, 299 lines | 694 | 225 | 40,770 (66.6) | 88,779 (1,112) | 11.5 % | 18 lines untranslated |

## 3. What this does not show
Machine-made transcripts: reply length (the hour `minutes` wrote 90 thousand tokens) depends on the transcript, which is list-like and repetitive; the Vietnamese was not read. One run per cell. The prompt share excludes server start. The 1 h `minutes` after the fix, and a larger window for the rubric on real transcripts, are open.
