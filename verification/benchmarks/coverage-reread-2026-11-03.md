# Speech without words: read again, or written as a sound (0.2.7, M2d-1 and M2i)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, GPU on. Not "verified".
- **Build:** "before" is the release `asr_bench` built before the change (it already had the repair of broken words); "after" has the gap and sparse-region re-read. Turbo, Vietnamese, FLEURS de-duplicated clips (see the correction in the Vietnamese baseline), made quiet and gated with the models of `level-aware-transcription-2026-11-03.md`.
- **Question:** does reading uncovered speech-like stretches again, as pieces of their own, raise word coverage and lower WER, and what happens to what stays unreadable?

## 1. The change

- `vad::speech_regions` finds stretches of speech-like level (a share of the way from the noise floor to the speech level; dips up to 0.5 s bridged; at least 3 s).
- After each piece (`Job::fill_gaps`): inside such a region, a stretch of 3 s or more with no word within 0.5 s is read again alone (no prompt); a region whose words fill under a quarter of it (a short word standing for seconds of speech) is read again whole. A new reading is kept when it is clean (no broken words, no loop, mean confidence 0.4 or more) and, for a whole region, says more than the old words. At most 20 per piece.
- What stays unreadable is written as a sound entry, `Transcript::sounds` (`[noise]`), not as invented words; the words that are there are never deleted for it. `sounds::classify` upgrades a sound to `[unclear]` wherever speech evidence (the speaker engine) overlaps it; the engine runs outside the piece loop, so this call belongs to the daemon (not wired). Captions, reports, translation and search read `words` only; `sounds::captions_with_sounds` is for the exports.
- Short words that come back after long silences are read again like the other suspects and replaced by what the second reading hears (silence included); never dropped unheard. A test with the same words really spoken shows they survive.

## 2. Results (Turbo, Vietnamese, 4-minute clips; WER % / coverage % / uncovered seconds / Whisper calls)

| Clip | Condition | Before | After |
|---|---|---|---|
| viu_a | -38 dBFS | 12.2 / 98.6 / 0 / 1 | 12.2 / 98.6 / 0 / 1 |
| viu_a | -45 gated | 26.9 / 90.7 / 18 / 1 | **24.0 / 93.1 / 9 / 2** |
| viu_a | -52 | 28.2 / 92.9 / 15 / 1 | 28.2 / 92.9 / 15 / 1 |
| viu_a | -52 gated | 42.9 / 83.3 / 28 / 1 | **38.1 / 88.2 / 8 / 3** |
| viu_b | -38 dBFS | 20.7 / 91.8 / 17 / 1 | 20.7 / 91.8 / 17 / 1 |
| viu_b | -45 gated | 29.8 / 80.3 / 40 / 3 | 29.8 / 80.3 / 40 / 3 |
| viu_b | -52 | 37.3 / 79.6 / 44 / 1 | 37.3 / 79.6 / 44 / 1 |
| viu_b | -52 gated | 42.7 / 69.3 / 69 / 1 | **41.0 / 71.7 / 59 / 2** |
| viu_h (held out) | -38 dBFS | 18.6 / 95.0 / 0 / 1 | 18.6 / 95.0 / 0 / 1 |
| viu_h (held out) | -45 gated | 44.8 / 83.0 / 18 / 1 | **44.0 / 84.0 / 18 / 2** |
| viu_h (held out) | -52 | 30.5 / 84.8 / 22 / 1 | 30.5 / 84.8 / 22 / 1 |
| viu_h (held out) | -52 gated | 30.7 / 87.0 / 25 / 1 | 30.7 / 87.0 / 25 / 1 |

Coverage rose in 4 of 12 cells (by 1.0 to 4.9 points), WER fell in the same 4 (by 0.8 to 4.8 points), the others are unchanged, none got worse; extra Whisper calls: 0 to 2 per clip. Clean (not quiet) clips: coverage 96 to 100 %, unchanged. Short stretches that the re-read does not fix (cells where the uncovered seconds did not change) stay empty; there they are now sound entries.

Not measured here: the maintainer's real case (a quiet full sentence written as one high-confidence word): reproduced only by the unit test `a_short_word_over_a_long_speech_region_is_read_again`, with a scripted engine; Whisper's own safeguards (no-speech threshold, average log-probability threshold, temperature fallback): the shipped call already sets whisper.cpp's defaults, and an experiment through a temporary hook on the early (duplicated-sentence) clips changed nothing; they have not been measured again on the unique clips.

## 3. Medium and short rows
- 1 hour (`viu_1h`, clean, 3,463 s): WER 10.7 %, coverage 97.3 %, 19.0x real time, 2.7 GB (before the change; the gap step adds a few short reads per piece, not re-measured).
- Short clips: unchanged (one call, no region of 3 s).

## 4. What this does not show
Quiet and gated clips are our models; one run each; four clips (one held out). The `[noise]`/`[unclear]` upgrade by the speaker engine and the exports are not wired in the daemon.
