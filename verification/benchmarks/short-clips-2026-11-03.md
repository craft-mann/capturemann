# Short clips (about 15 s) next to the long rows (0.2.7, "Audio length targets")

- **Date:** 2026-11-03. **Status:** measured, one run per cell, Apple M4 Pro, GPU on. Not "verified". Covers what was changed or baselined so far; reports, translation and the planned re-read and coverage work will add rows when they land.
- **Data:** four single FLEURS vi_vn utterances (9.1 s, 11.5 s, 9.3 s, 13.0 s; not in the long clips), and two synthetic English clips (`s_en2` 20.0 s with 2 voices, `s_en3` 26.6 s with 3 voices, text-to-speech). Commands as in the long reports.

## Transcription (Large v3 Turbo and PhoWhisper-small, the app's path)

| Clip | Model | Length | WER | Words (reference) | Wall s | Whisper calls | Coverage | U+FFFD / foreign / broken |
|---|---|---|---|---|---|---|---|---|
| s_vi1 | turbo | 9.1 s | 20.0 % | 23 (25) | 0.67 | 1 | 93 % | 0 / 0 / 0 |
| s_vi2 | turbo | 11.5 s | 16.7 % | 24 (24) | 0.67 | 1 | 99 % | 0 / 0 / 0 |
| s_vi3 | turbo | 9.3 s | 15.4 % | 26 (26) | 0.67 | 1 | 97 % | 0 / 0 / 0 |
| s_vi4 | turbo | 13.0 s | 3.0 % | 33 (33) | 0.76 | 1 | 95 % | 0 / 0 / 0 |
| s_vi1..4 | phowhisper-small | 9 to 13 s | 44.0 %, 62.5 %, 30.8 %, 33.3 % | 20 to 27 of 24 to 33 | 0.34 to 0.42 | 1 | | 0 / 0 / 0 |
| s_en2 | turbo | 20.0 s | 4.3 % | 47 (47) | 0.75 | 1 | 100 % | 0 / 0 / 0 |
| s_en3 | turbo | 26.6 s | 8.3 % | 70 (72) | 0.84 | 1 | 100 % | 0 / 0 / 0 |

Compared with the long rows (Turbo, 4-minute clips): Vietnamese WER 20.8 to 24.8 % there against 3.0 to 20.0 % here; no re-reads (1 call), no garbage words, a clip finishes in under a second (14x to 31x real time).

## Suggested pauses (silence rule, `quiet_silence`, speech map known)

| Clip | Speech level | Suggestions | Inside speech | At the clip edges |
|---|---|---|---|---|
| s_en2 | -30 and -45 dBFS, raw and gated | before the edge rule 2, after 1 | 0 | before 1, after 0 |
| s_en3 | -30 and -45 dBFS, raw and gated | before 2, after 1 | 0 | before 1, after 0 |

A silence that touched the start or end of the track (here the 1.8 s and similar tails after the last word) used to be suggested as a pause; it no longer is (`m2g_lead_in_and_tail_silence_are_not_suggested`). The long meeting is affected only at its two ends.

## Speaker finding (the app's automatic path, count unknown)

| Clip | Voices | Found | Turns | Wall s |
|---|---|---|---|---|
| s_en2 | 2 | **1** | 2 | 0.71 |
| s_en3 | 3 | **1** | 3 | 0.93 |

The count is not forced (nothing invents speakers), but on 20 to 27 s clips with 2 and 3 clearly different voices the engine finds one speaker. The long synthetic clips found 2 of 3 and 2 of 4 at the shipped threshold; short clips under-detect more. Not fixed; it belongs with the M2b threshold work.

## What this does not show

Four utterances and two synthetic clips, one run each. Reports and translation on a 15 s transcript were not run here (the report needs 30 words: `TOO_LITTLE_SPEECH` below that, by design). The broken-word repair was exercised only on PhoWhisper output and unit tests; Turbo gave no garbage on any short clip.
