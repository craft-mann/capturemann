# Transcript normalisation (AI-10, W8): measured 2026-11-04

Measured, not verified: one run per cell, one M4 Pro, and **every timing was taken while other
agents ran Whisper and llama.cpp on the same GPU** (load average 4 to 130 during the runs), so
seconds are inflated and noisy; token counts, rejection rates and scores are not affected.
Tool: `cargo run --release -p cm-ai --example normalize_bench` (real `llama-server`, text model
files from the app's models folder, temperature 0, JSON schema reply). Code: branch `track/mac-norm`.

## What was run

* Public references with punctuation: FLEURS test `vi_vn` (347 sentences) and `en_us` (350).
  Punctuation and case were removed from the words (tone marks kept), the words were grouped into
  "turns" of 1 or 3 consecutive sentences (`g1`, `g3`; in `g3` the sentences are unrelated, so
  sentence boundaries have no context: a harder, not a typical, test), the pass ran, and the
  result was scored against the original. `syn4` is the synthetic English meeting of the earlier
  benchmarks (LLM-written text with punctuation: its commas are over-used, so its comma F1 is not
  informative).
* Private Vietnamese meeting transcripts (three bundles, copied to a scratch folder): **counts and
  times only**; no text appears here.
* Models: Gemma 4 E2B Q4_0 (the pinned default) and Qwen3.5 4B Q4_K_M (installed, in the catalogue).
* Rows marked `v1` in the notes were run before the bisect step (below) and are superseded.

## Invariant and rejected turns (target under 5 % of turns)

"Rejected" counts pieces (a turn that failed twice is split in two and each half asked again, down
to 12 words, so a piece is a turn or part of one). Word rate = raw words kept / all words.
Tone-mark damage in accepted text, counted by a separate code path: **0 words in every run**
(also 0 substitutions, deletions and insertions by construction).

| Run | Model | Words | Pieces | Rejected pieces | Rejected words |
| --- | --- | --- | --- | --- | --- |
| FLEURS vi, 1 sentence per turn | Gemma E2B | 1,872 | 74 | 2 (2.7 %) | 1.1 % |
| FLEURS vi, 3 sentences per turn | Gemma E2B | 2,739 | 67 | 5 (7.5 %) | 1.9 % |
| FLEURS vi, 3 sentences per turn | Qwen3.5 4B | 2,739 | 54 | 2 (3.7 %) | 0.8 % |
| FLEURS en, 1 sentence | Gemma E2B | 1,298 | 69 | 3 (4.3 %) | 2.0 % |
| FLEURS en, 3 sentences | Gemma E2B | 2,000 | 48 | 4 (8.3 %) | 1.4 % |
| FLEURS en, 3 sentences | Qwen3.5 4B | 2,000 | 41 | 1 (2.4 %) | 0.5 % |
| synthetic English meeting | Gemma E2B | 900 | 30 | 0 | 0 % |
| private vi meeting, first 5 min | Gemma E2B | 962 | 32 | 4 (12.5 %) | 4.4 % |
| private vi meeting A, first 15 min | Gemma E2B | 2,508 | 76 | 7 (9.2 %) | 2.1 % |
| private vi meeting B, first 15 min | Gemma E2B | 2,612 | 104 | 9 (8.7 %) | 2.5 % |

Without the bisect step the same FLEURS vi 1-sentence run rejected 16.7 % of turns (18.8 % of
words) and the 3-sentence run 48 %: nearly all failures were the model "helping" (writing digits as
words, fixing a typo, merging a garbled word). The bisect step and the rule "do not fix spelling;
keep digits as digits" took word loss to 1 to 2.5 %. On the private meetings (Whisper output with
disfluencies) the piece rate is 9 to 12 % and the word rate 2 to 4 %: **the 5 % target is met by
words, not by pieces, on Vietnamese meetings** (many of the failed pieces are short bisected leaves).
First-answer errors on the private meetings were mostly substitutions, insertions and deletions
at the same order of size; after the retry no change reaches the output.

## Punctuation and case (all pieces counted; a rejected piece counts as unpunctuated)

F1 per mark, case accuracy over words that start with a letter. "No pass" case accuracy is the
lower-case input.

| Run | Comma F1 | Period F1 | Sentence boundary F1 | Question F1 | Case acc. | No pass case acc. |
| --- | --- | --- | --- | --- | --- | --- |
| vi, 1 sentence, Gemma | 0.729 | 0.822 | 0.824 | 1.0 (1 mark) | 0.953 | 0.886 |
| vi, 3 sentences, Gemma | 0.608 | 0.660 | 0.664 | 1.0 (1 mark) | 0.943 | 0.892 |
| vi, 3 sentences, Qwen 4B | 0.625 | 0.742 | 0.744 | 1.0 (1 mark) | 0.951 | 0.892 |
| en, 1 sentence, Gemma | 0.777 | 0.919 | 0.920 | 1.0 (1 mark) | 0.968 | 0.866 |
| en, 3 sentences, Gemma | 0.724 | 0.814 | 0.816 | 1.0 (1 mark) | 0.960 | 0.866 |
| en, 3 sentences, Qwen 4B | 0.752 | 0.876 | 0.877 | 1.0 (1 mark) | 0.969 | 0.866 |
| synthetic English meeting, Gemma | 0.479 (reference over-comma) | 0.945 | 0.945 | n/a | 0.993 | 0.906 |

Only one question mark exists in each FLEURS set, so the question score says nothing. The Vietnamese
paragraph breaks were not scored (no reference has them).

## Speed

Server numbers (llama.cpp's own log) on the private 15-minute meeting A: 38,432 prompt tokens at
1,231 tok/s and 7,579 generated tokens at 90 tok/s, for 2,508 words: **3.0 generated tokens per
Vietnamese word** (2.5 for English) and about 15 prompt tokens per word (the fixed instructions
are sent with every call; 96 calls). Wall time:

| Audio | Run | Wall (contended) | Per audio minute |
| --- | --- | --- | --- |
| 5 min vi (private) | Gemma E2B | 66.8 s | 13.4 s |
| 15 min vi (private A) | Gemma E2B | 115.8 s | 7.7 s |
| 15 min vi (private B) | Gemma E2B | 153.9 s | 10.3 s |
| 8.7 min en (FLEURS, 1 sentence) | Gemma E2B | 52.3 s | 6.0 s |

Budget was about 1 min for 15 minutes: **measured 2 to 2.5 times over** (116 to 154 s) even with
the generation rate at 90 tok/s, because Vietnamese needs 3 tokens per word and the instructions
are re-read per call. At the same rate 1 hour is about 8 minutes (projected, **not measured**: the
60-minute run was queued behind other agents' GPU runs and did not start before this report). A
15-second clip (about 40 words) is about 120 generated tokens, so about 2 s (projected, not
measured). A cheaper prompt (shorter instructions, prompt caching across calls) is the first fix.

## Is Gemma 4 E2B enough?

For an opt-in tidy action: yes for English (period F1 0.92 on single sentences, case 0.97), borderline
for Vietnamese (period F1 0.82 on single sentences, 0.66 with unrelated sentences; 2 to 4 % of
words stay raw on real meetings). Qwen3.5 4B, already in the catalogue, was better by 0.02 to 0.08 F1
and rejected about half as many words, at roughly 25 to 30 % more time. The 26B model (Macs with 32
GB) was not run. Not measured: the 50-turn human spot-check (`tests/manual/NORM-1.md`), paragraph
breaks, 1-hour timing, the private 5-minute corrected reference (the file was not present), and
LibriSpeech/AMI/PriMock (not present locally).
