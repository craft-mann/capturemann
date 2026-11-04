# Model for far-field meetings, 2026-11-04

Measured on this Mac with `asr_bench` (Metal, greedy, shipped lift stack unless a row says otherwise). Repeated runs gave identical output, so each cell is one number. No private text is in this report; numbers and counts only. "RTF" is audio seconds per wall second (higher is faster).

## meeting-5min (private, 300 s, about 1,000 reference words, language vi)

| Model | Lift | Prompt (made-up terms) | WER % | Ins | Del | Sub | Calls | RTF | Peak MB |
|---|---|---|---|---|---|---|---|---|---|
| Turbo | off | off | 23.1 | 31 | 92 | 108 | 1 | 23.7 | 1995 |
| Turbo | off | on | 25.6 | 24 | 125 | 107 | 1 | 24.4 | 1994 |
| Turbo | on | off | 22.7 | 56 | 42 | 129 | 1 | 22.4 | 2007 |
| Turbo | on | on | 22.4 | 48 | 52 | 124 | 1 | 24.6 | 1999 |
| Large v3 | off | off | 17.9 | 40 | 44 | 95 | 3 | 4.1 | 3806 |
| Large v3 | off | on | 18.2 | 35 | 52 | 95 | 3 | 4.3 | 3815 |
| Large v3 | on | off | 17.1 | 38 | 38 | 95 | 3 | 4.6 | 3812 |
| Large v3 | on | on | 18.7 | 36 | 57 | 94 | 3 | 4.4 | 3813 |
| Medium | off | off | 37.6 | 11 | 201 | 164 | 1 | 13.7 | 2007 |
| Medium | off | on | 32.5 | 24 | 145 | 156 | 1 | 16.6 | 2010 |
| Medium | on | off | 27.3 | 29 | 87 | 157 | 1 | 16.4 | 2008 |
| Medium | on | on | 32.8 | 27 | 126 | 175 | 1 | 16.2 | 2014 |

Large v3 is 5.6 points better than Turbo with the lift (17.1 vs 22.7), fewer substitutions (95 vs 129) and about the same insertions and deletions combined. It is 5 times slower by RTF and uses 1.9 times the memory. Medium is worst. A made-up prompt did not help any model consistently (it changes WER by -0.3 to +2.5 for Turbo/Large, mixed for Medium); it is a different thing from the real glossary.

## Why the stored older Large v3 transcript scores 12.6 % and the new run 17.1 to 17.9 %

Tested one by one on meeting-5min with Large v3:

| Hypothesis | Result |
|---|---|
| New pipeline made it worse (build before the stack vs now) | No. Old binary 17.9 %, new without lift 17.9 % (identical counts 40/44/95); the lift improves it to 17.1 %. |
| Language setting `vi` vs `vi,en` | No change (17.9 and 17.1 either way). |
| Original vs denoised audio | Not a difference: mic.wav and original.m4a are the same samples for this bundle. |
| `n_max_text_ctx` 0 (no carry-over) | Worse: 25.4 % (23 ins, 125 del, 106 sub). |
| Initial prompt of the same word count (made-up) | 18.2 / 18.7 %: no gain. |
| Repeat-rule re-reads | Calls are 3 in all Large rows, identical in old and new builds, so the rules change nothing here. |

Conclusion: no step of the current pipeline makes Large v3 worse. The remaining candidates cannot be tested here: (a) the real glossary used as the prompt in the older run (a made-up glossary of the same size is not the real one, so its null result does not exclude a real one); (b) the stored transcript is probably the seed of the maintainer's corrections, so its 12.6 % is biased toward the reference. It differs by 14.5 % of words from the new run, which is the size expected from (a) plus (b). To tell them apart the maintainer should read the glossary / vocabulary setting that was on when the older transcript was made (Settings, AI page, the transcription glossary or prompt terms list; stored in the app config, not readable from here) and run the same 5 minutes with it on and off, and ideally provide a second reference not seeded from a machine transcript.

## English far-field rows (lift on; Turbo / Large v3 / Medium WER %, Large v3 peak 3.8 GB, Turbo and Medium 2.0 GB)

| Clip | Turbo | Large v3 | Medium |
|---|---|---|---|
| AMI EN2002b | 45.0 | 47.6 | 45.2 |
| AMI ES2004b | 23.8 | 28.6 | 24.2 |
| AMI ES2004c | 27.7 | 29.4 | 27.8 |
| AMI TS3003a | 37.9 | 34.1 | 28.9 |
| PriMock day1 | 21.6 | 25.5 | 26.2 |
| PriMock day2 | 15.4 | 17.4 | 16.9 |
| PriMock day3 | 17.2 | 17.0 | 16.8 |
| PriMock day4 | 15.7 | 16.8 | 15.3 |
| synthetic meeting (syn4) | 6.2 | 12.7 | 6.3 |

(AMI clips are the speaker clips; they have word references, so they were transcribed.) Large v3 is worse than Turbo on 7 of 9 English rows, better on 1 (TS3003a, by 3.8) and equal on 1. Real insertion/deletion counts are in `$scratch/models.jsonl` (not stored in the repo). The 15 s and 1 h rows were not re-run for the models; earlier reports for Turbo hold (short-clips and review-stack reports) and a 1 h Large v3 run would take about 12 minutes of compute: not done.

## Model choice rule: what the data supports

Nothing general. Large v3 wins on one private Vietnamese far-field clip (5.6 points) and loses on English far-field speech and on the synthetic meeting; on FLEURS read speech Turbo was equal or better (earlier report). No measured audio property (level, speech-level-below-peak) was found to separate the clip where Large wins from the clips where it loses with n=1, so no automatic switch is wired, and Turbo stays the default. Proposed, not wired: an AI-page hint under the model list: "Large v3: can be more accurate on meetings with distant microphones and Vietnamese, about 5 times slower, 1.9 times the memory; try it on one recording and compare." That is a desktop UI row and needs the maintainer's go (apps/desktop is outside this track). Confidence: low (one private clip, 1,000 words, one second-language setting). A second reference from the maintainer (another Vietnamese far-field meeting) would show whether the win is about the language, the room or the speaker; if it repeats, a language-based hint ("Vietnamese: Large v3") is the data-supported rule, not an audio-level one.

## Bare-word repeat rule

`failure_kinds::impossible_token_runs`: one folded token written 5 or more times in a row is a suspect when it runs faster than 7 words/s (the existing speech limit), or, from 8 repeats, when its mean confidence is under 0.5. It feeds `Job::repair` as a flag source, the re-read must not contain such a run to replace it, and otherwise the words are removed and written as a sound ([noise]/[unclear]). Tests with made-up tokens (blorp, qux, a Chinese character): a 16-word run at 14 words/s and confidence 0.21 is found; 4 words at a human pace and 8 slow confident ones are not; both the "re-read gives other words" and "re-read loops too" paths end without the token (sound stands for it). On the stored Nov 2 transcript the probe reports 0 bare token runs (the repeated word was already repaired in the new decode: new loop-probe decode has 1 sentence run of 18 words, 0 dense). Grep proof: no phrase, word list or language-specific string appears in the rule or its thresholds; tests use only made-up tokens.
