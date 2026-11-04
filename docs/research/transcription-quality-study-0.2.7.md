# Transcription quality in CaptureMann 0.2.7: a study of one day's measurements

Status: **written Nov 4, 2026 from the stored reports** (`verification/benchmarks/`), the plans (`specs/plans/`) and the code of `crates/cm-ai`, first at commit `6db020f`, then updated the same day at `b22553e` (the final release comparison, the careful-re-read measurement and the Nov 4 decisions; the 0.2.7 version bump is `407acfa`). Every number is "measured" (one machine, mostly one run per cell); nothing here is "verified" in the project's sense (no manual script report stands behind it). Private meeting audio and text never appear: the three private recordings are called Meeting A, B and C and only counts, rates and metrics are given. Where two reports disagree, the later one is used and the disagreement is listed in section 7.3. The rows that were still running in the first version are now filled (E9, E5, E6, E7.4); what remains open is marked **PENDING** in the text and listed as 0.2.8 follow-ups in section 7.4.

Source key (file names are in `verification/benchmarks/` unless stated): `en-syn` = `transcription-baseline-en-synthetic-2026-11-03.md`; `vi-fleurs` = `vietnamese-fleurs-baseline-2026-11-03.md`; `vi-models` = `vietnamese-models-2026-11-03.md`; `level` = `level-aware-transcription-2026-11-03.md`; `short` = `short-clips-2026-11-03.md`; `broken` = `broken-tokens-2026-11-03.md`; `coverage` = `coverage-reread-2026-11-03.md`; `safeguards` = `whisper-safeguards-2026-11-03.md`; `repeats` = `repeated-sentences-2026-11-03.md`; `spk-retune` = `speaker-retune-2026-11-03.md`; `spk-3plus` = `speaker-detection-3plus-synthetic-2026-11-03.md`; `spk-windows` = `speaker-finding-windows-2026-11-03.md`; `cuts` = `suggested-cuts-quiet-speech-2026-11-03.md`; `review-1-3` = `review-steps-1-3-2026-11-03.md`; `review-stack` = `review-stack-2026-11-03.md`; `far-field` = `far-field-model-2026-11-04.md`; `regression` = `large-v3-regression-2026-11-04.md`; `release-cmp` = `release-compare-0.2.6-vs-0.2.7-2026-11-04.md`; `params` = `parameter-search-2026-11-04.md`; `invented` = `invented-text-repro-2026-11-04.md`; `veto` = `invented-veto-signals-2026-11-04.md`; `prep` = `audio-prep-2026-11-04.md`; `norm` = `normalization-2026-11-04.md`; `text-base` = `text-model-report-translation-baseline-2026-11-03.md`; `text-m2c` = `text-model-m2c-2026-11-03.md`; `text-oct30` = `text-model-report-translation-m4pro-2026-10-30.md`; `perf-oct29` = `performance-m4pro-2026-10-29.md`; `import` = `import-0.2.7-2026-11-03.md`; `plan` = `specs/plans/transcription-quality-plan-0.2.7.md`; `brief` = `specs/plans/transcription-pipeline-review-brief.md`; `rel-plan` = `specs/plans/release-0.2.7-plan.md`.

## 0. Abstract

**Question.** Between release 0.2.6 and the 0.2.7 work of Nov 3 and 4, 2026, what changed in CaptureMann's local Whisper transcription (whisper.cpp 1.8.3 through whisper-rs 0.16, Large v3 Turbo by default), by how much, on what data, and what did not change?

**Method.** Public sets with references (FLEURS vi_vn read speech, AMI and PriMock57 English conversations, a synthetic English meeting, a generated silence set) and three private Vietnamese business meetings (counts only; one 1,000-word hand-corrected reference, one 298-word accepted window) were run through the app's own piece pipeline on one Apple M4 Pro, one run per cell (decodes were bit-identical when repeated). Metrics: WER folded and exact (tone marks kept), insertions/deletions/substitutions, speech coverage, loop counters, invented-text units per hour of reference-silent audio, real-time factor, peak memory.

**Main findings.** (1) Coverage re-read is the largest user-visible gain: on an 84 s window of Meeting C, 0.2.6 Turbo left 86.6 % of the reference words out (WER-exact 97.3 %) and 0.2.7 recovers them (29.2 %); on the full 75-row comparison 0.2.7 Turbo is better where 0.2.6 failed (PriMock day3 20.3 to 17.2, AMI IS1009b 31.0 to 30.2, Meeting B loops 3 to 0) and identical to the word on every clean row, worse on one row by one word (AMI TS3003a +0.5) [`release-cmp`]. (2) Turbo word accuracy on Meeting A is unchanged: 23.3 % exact in both releases [`release-cmp`]. (3) The quiet-input lift caused a +5.6 point Large v3 regression on Meeting A (12.8 to 18.4 % exact), found and fixed twice; the final rule lifts only audio whose peak is at least 12 dB under -1 dBFS [`regression`]. (4) Invented text over near-silence is reproduced publicly: Turbo writes 171 (vi) and 109 (en) invented sentence units per hour of silent audio, Large v3 194 (vi); no decode parameter, audio preparation, veto signal or beam re-read removes it without cost (careful re-reads: 78 vs 78 units, +83 words) [`invented`, `params`, `veto`, `prep`, `release-cmp`]. (5) Large v3 beats Turbo only on Meeting A (12.8 vs 23.3 % exact) and by 0.2 to 3.8 points on three English rows; it is worse on FLEURS b and c, the 1 h FLEURS row (17.4 vs 11.7), syn4, PriMock day1 and AMI ES2004b, 3 to 5x slower, 1.9x the memory, and invents more (86 vs 35 words outside speech on the noise set) [`release-cmp`].

**Ships in 0.2.7 (decisions of Nov 4).** Coverage re-read, broken-token repair, repeated-sentence and bare-token rules, sound entries, the headroom-limited lift, the word-time repair (`ai.repair_word_times` on), adaptive pause suggestions, windowed speaker finding, translation split-and-retry, opt-in transcript normalisation without UI, WER-exact, the Large v3 label. **Does not ship, or ships off.** Any decode-parameter change; the hallucination gate; Silero as cutter; the raw-microphone track (`ai.transcribe_raw_mic` off: +6.8 % words on Meeting B through the daemon, but the invented sentence returns over a noise-only clip and no veto was found); careful re-reads (`ai.careful_rereads` off: no gain); per-region lift; silence trimming; piece cuts after speech; PhoWhisper; the Vietnamese Zipformer.

## 1. Background and the pipeline

### 1.1 Why this work

The 0.2.7 plan [`rel-plan` M2d, M2g, M2i, M2j] started from one private 45-minute Vietnamese meeting (Meeting B): 29 gaps over 8 s with no words (546 s, 329 s of it speech by the speaker engine), one word written 7 times with spread-out times, 3 words with U+FFFD and 102 words under 5 % confidence, a laugh written as a word, outro-style text nobody said, and 171 suggested pauses that would have cut 150 transcript words. The maintainer's rule for the fixes: general techniques only, no phrase lists, no one-recording fixes, every change with a before/after report, nothing called "verified" or "accurate" without a stored report.

### 1.2 The pipeline as shipped in 0.2.6 and in 0.2.7

Both releases: FFmpeg decodes the chosen microphone track (the noise-reduced copy by default, the raw file with `original_audio`) to 16 kHz mono; audio up to 600 s is one piece, longer audio is cut at the quietest 100 ms near every 300 s (`pieces.rs`, `PIECE_SECS`, `LONG_AUDIO_SECS`); each piece is one Whisper call (greedy `best_of 1`, whisper.cpp defaults for every threshold: fallback step 0.2, entropy 2.4, log-prob -1.0, no-speech 0.6; `suppress_blank`, `suppress_nst`, token timestamps with DTW; glossary as `initial_prompt`); a looping piece (`is_degenerate`, `find_runs`: 8 identical tokens) is split and re-read, else repeats are collapsed; a bad piece may be read from the other audio track [`brief` §2; `git show v0.2.6:crates/cm-ai/src/pieces.rs`].

```
                     0.2.6                                    0.2.7 (added stages marked +)
 mic track ──► FFmpeg 16 kHz ──► pieces (300 s) ──► Whisper ──► loop check ──► words
                                                                  │
                     ┌────────────────────────────────────────────┘
                     ▼
 0.2.7:  + quiet-input lift (per call; only if RMS < -26 dBFS AND peak <= -13 dBFS)
         ▼
         Whisper (same decode as 0.2.6)
         ▼
         loop check (8-token runs)            [0.2.6]
         + broken-token re-read / clean / drop  (U+FFFD, foreign script < 0.2, runs < 0.1)
         + repeated-sentence rule (3 in a row, or 5 within 60 s): re-read alone, keep, or sound
         + bare-token rule (5+ at > 7 words/s, or 8+ under 0.5 confidence)
         + coverage re-read (energy regions >= 3 s with no word within 0.5 s; sparse < 25 %)
         + sound entries [noise] / [unclear] instead of invented words
         + word-time repair (a stack of 4+ words under 10 ms is spread between its neighbours)
         ▼
         words + sounds ──► glossary ──► captions / speakers / reports / translation
         + opt-in normalisation (text model; punctuation and case; words invariant)
         (off by default: ai.transcribe_raw_mic, ai.careful_rereads, ai.hallucination_gate)
```

Thresholds from the code at `6db020f`: `Normalize { below_db: -26, target_db: -20, max_gain_db: 15, peak_limit_db: -1, min_headroom_db: 12 }` (`pipeline.rs`); `GAP_MIN_SECS 3.0`, `GAP_PAD_SECS 0.5`, `SPARSE_SHARE 0.25`, `MAX_GAP_READS 20`, `GAP_MIN_CONFIDENCE 0.4` (`pieces.rs`); `is_broken`: foreign script and confidence under 0.2 (`failure_kinds.rs`); `impossible_token_runs(min 5, 7 words/s, 0.5)` (`failure_kinds.rs`, `far-field`); repeated sentences 3 in a row or 5 in 60 s, kept when the second reading agrees within one at up to 6 repeats and 7 words/s [`repeats` §2]. Settings with off switches (`pipeline.rs` at `b22553e`): `ai.lift_quiet_input` (on), `ai.repair_word_times` (on; `wordtimes.rs`: `COLLAPSED_SECS 0.010`, `MIN_STACK 4`, `MAX_SECS_PER_CHAR 0.12`), `ai.hallucination_gate` (off), `ai.vad_opinion` (off), `ai.carry_previous_text` (on), `ai.transcribe_raw_mic` (off), `ai.careful_rereads` (off: beam 5 and no temperature fallback on the windows the pipeline reads again). `DecodeParams::default()` equals the 0.2.6 decode (`whisper.rs`, [`params`]).

Size of the change: 85 commits and 12,331 inserted lines in `crates/cm-ai` between `v0.2.6` (`39770b8`) and `6db020f` (`git log --oneline v0.2.6..HEAD -- crates/cm-ai | wc -l`, `git diff --stat`); the raw-mic, word-time and careful-re-read commits came after that count. The plan's earlier count (56 commits, 7,758 lines) was taken at `7ac19a1` [`plan` §0].

## 2. Data sets and metrics

### 2.1 Sets

| Set | Kind | Size | Licence / status | Reference and its bias | Used in |
|---|---|---|---|---|---|
| syn1/3/4/6 | synthetic English meetings, macOS `say` voices, `make_synthetic_meeting.py` | 240 to 844 s, 710 to 2,481 words | generated, text in repo, audio regenerated | exact generated text; numbers spelled out (style errors inflate WER) | E1, E5, E6, E8 |
| FLEURS vi_vn test | read Vietnamese sentences, one speaker each | 857 utterances, 544 MB; unique-sentence clips viu_a/b/c (243 to 253 s), viu_h held out (249 s), viu_1h (3,463 s, 7,963 words), s_u1..4 (9 to 26 s) | CC BY 4.0, outside the repo | normalised FLEURS transcripts (lower case, no punctuation) | E1 to E7 |
| FLEURS en_us test | read English | 350 sentences | CC BY 4.0 | cased, punctuated text | E8 |
| AMI | English meetings, 4 speakers, array mix | clips of 3 to 5 min; 3 whole meetings of 1,535 to 1,898 s | CC BY 4.0 (card) | turn lists and words | E1, E5, E10 |
| PriMock57 | doctor-patient consultations, 2 speakers | clips of 3 to 5 min | CC BY 4.0 (mirror) | hand-timed utterances | E1, E5, E10 |
| noise set | 9 clips of 30 s, generated pink/brown noise with one FLEURS utterance | 9 x 30 s | generated (`make_noise_burst_set.py`) | the utterance text; silence = outside the span | E5 |
| silence set | 72 clips, 79 min, 36 per language: FLEURS vi or synthetic English speech with generated room tone, hum, clicks, breaths, tones, synthetic RIRs | 27.0 min of reference-silent audio per language | generated (`make_silence_set.py`), not in repo | reference-silent spans from construction | E6, E7 |
| **Meeting A** | private, imported audio, Vietnamese far-field, 23.4 min, 5 pieces | first 300 s: 1,000 (also stated as 1,049) reference words, 2 speakers | private | **hand-corrected, seeded from an older Large v3 machine transcript** (biased toward Large v3) | E1, E2, E5, E6, E8, E9 |
| **Meeting B** | private, recorded in the app, 44.6 min, 9 pieces, raw and noise-reduced mic tracks, 6 participants | no word reference | private | listening labels on short clips only | E3, E6, E7, E10 |
| **Meeting C** | private, recorded, 88 min, 17 pieces | 84 s window (89 to 173 s), 298 words | private | **a stored Large v3 output the maintainer called acceptable** (Large v3 scores 0.7 % on it: not an accuracy number) | E4, E5, E6, E7 |

Rows the reports label `pat5`/`PAT`/`meeting-5min` are Meeting A; `nov2`/`Nov 2` are Meeting B; `hop`/`Data Platform` are Meeting C.

### 2.2 Metrics

- **WER folded** (`measure::word_errors`): case, punctuation and diacritics folded, so a wrong tone mark on the right syllable is not an error. Every Vietnamese WER before Nov 4 is folded and therefore optimistic [`vi-fleurs` §2, `plan` §0].
- **WER exact** (`measure::word_errors_exact`, commit `4f77853`): NFC, case and punctuation folded, tone marks and `đ` kept. Measured gap exact minus folded on the same decode: 0.3 to 1.4 points on Vietnamese rows (Meeting A 0.6 to 0.7; FLEURS pooled 1.3 to 1.4; Meeting C 0.3; viu_h 0.8) and 0 on English [`params` tables 1 and 2, `release-cmp`]; the 15 s FLEURS row is 4.0 points (20.0 to 24.0 %, 25 words) [`release-cmp`]. The informal "about 0.5 point" is the low end of that range.
- **Insertions / deletions / substitutions** from the alignment, always next to the WER in the Nov 4 reports.
- **Coverage**: share of speech seconds (reference spans) with a word within 1 s; "uncovered" = runs of 5 s or more without a word [`vi-fleurs` §4].
- **Loops**: `loop_runs` (a sentence unit 3 or more times in a row), `dense_repeat_words`, bare-token runs (`loop_probe`, `failure_kinds`).
- **Invented text**: words whose middle lies in a reference-silent span, and *units* (such words less than 1 s apart), per hour of reference-silent audio (`silence_probe`); the plan's target is at most 2 units per hour [`invented` §2, `plan` §5].
- **Real-time factor (RTF)** = audio seconds per wall second, model load excluded. **Every Nov 4 RTF was measured while other agents shared the GPU** (load average 3 to 270) and is a rough ratio inside one table only [`params`, `norm`, `release-cmp`]; the Nov 3 RTFs were measured alone.
- **Peak memory**: process high-water mark.
- **Run-to-run determinism**: three repeats of every cell in `review-1-3` were identical; the parameter-search baseline run twice on 14 sets gave the same text hash; the silence-set Turbo grid decoded twice gave 863 words over silence both times [`review-1-3`, `params`, `invented` §2]. So single runs are used on this Mac; this says nothing about other machines.
- **Hardware**: Apple M4 Pro, 14 CPU cores (10 performance, 4 efficiency), 20 GPU cores, 48 GB (`system_profiler`; `hw.memsize` 51,539,607,552 bytes = 48.0 GiB, which `perf-oct29` prints as "52 GB" in decimal), macOS 26.6.2, Metal on.

## 3. Experiments

### E1. Model comparison: read speech against meeting speech

**Question.** Which of Turbo, Large v3 and Medium reads Vietnamese and English best, on read speech and on meetings?

**Setup.** `asr_bench`, the app's pipeline, greedy, one run per cell; FLEURS unique clips, synthetic English, AMI and PriMock clips, Meeting A's first 300 s [`en-syn`, `vi-models`, `far-field`].

| Set | Turbo | Large v3 | Medium | Source |
|---|---|---|---|---|
| synthetic English, 4 clips, mean WER (folded) | 6.0 % (6.05 unrounded) | 11.1 % | 5.9 % | `en-syn` |
| synthetic English, RTF | 19.0 to 28.3 | 2.6 to 4.2 | 17.8 to 20.0 | `en-syn` |
| FLEURS vi, 4 x 4 min, mean WER (folded) | **10.6 %** | 12.3 % | 18.0 % | `vi-models` |
| FLEURS vi, 1 h row, WER / RTF / calls | 10.7 % / 20.3 / 16 | 17.8 % / 3.6 / 50 | 17.3 % / 16.8 / 31 | `vi-models` |
| Peak memory (FLEURS) | 1.9 to 2.7 GB | 3.6 to 4.7 GB | 1.9 to 2.7 GB | `vi-models` |
| Meeting A 300 s, lift on, no prompt, WER (folded) | 22.7 % (56/42/129 ins/del/sub) | **17.1 %** (38/38/95) | 27.3 % (29/87/157) | `far-field` |
| Meeting A, RTF / peak MB | 22.4 / 2,007 | 4.6 / 3,812 | 16.4 / 2,008 | `far-field` |
| English far-field, 8 real clips + syn4 | best on 7 of 9 | better on 1 (TS3003a, by 3.8), worse on 7 | between | `far-field` |

English far-field rows (WER folded, lift on; Large v3 peak 3.8 GB, the others 2.0 GB) [`far-field`]:

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
| syn4 (synthetic) | 6.2 | 12.7 | 6.3 |

Per-clip FLEURS rows (WER folded / coverage / RTF / calls) [`vi-models`]:

| Clip | Length | Turbo | Large v3 | Medium |
|---|---|---|---|---|
| s_u1 | 26 s | 35.9 / 100 / 28.0 / 1 | 28.2 / 100 / 12.8 / 1 | 30.8 / 100 / 23.8 / 1 |
| s_u2 | 9 s | 4.5 / 100 / 10.9 / 1 | 9.1 / 99.4 / 6.4 / 1 | 27.3 / 100 / 10.8 / 1 |
| s_u3 | 17 s | 2.1 / 95.1 / 20.1 / 1 | 2.1 / 97.2 / 8.5 / 1 | 4.2 / 100 / 15.7 / 1 |
| s_u4 | 12 s | 6.1 / 100 / 15.8 / 1 | 6.1 / 100 / 7.0 / 1 | 6.1 / 100 / 12.6 / 1 |
| viu_a | 243 s | 10.3 / 98.5 / 23.8 / 1 | 10.1 / 97.3 / 9.5 / 1 | 12.4 / 98.1 / 19.4 / 1 |
| viu_b | 243 s | 7.4 / 98.6 / 26.9 / 1 | 11.2 / 97.4 / 4.1 / 5 | 21.4 / 96.9 / 17.5 / 1 |
| viu_c | 253 s | 13.9 / 99.9 / 25.1 / 1 | 16.8 / 98.7 / 2.3 / 6 | 21.4 / 94.1 / 16.3 / 6 |
| viu_h (held out) | 249 s | 10.8 / 95.8 / 24.3 / 2 | 11.2 / 97.2 / 5.8 / 3 | 16.8 / 93.9 / 18.3 / 1 |
| viu_1h | 3,463 s | 10.7 / 97.3 / 20.3 / 16 | 17.8 / 97.2 / 3.6 / 50 | 17.3 / 93.1 / 16.8 / 31 |

The 26 s clip is one hard sentence of 39 words: on 9 to 26 s of audio, 4 words move WER by 10 points [`vi-models` §2]. Other models looked at and not adopted: PhoWhisper-small converted to ggml, 55.8 to 66.2 % WER with the shipped decode (mostly deletions; it needs its own decode path) [`vi-fleurs` §7, `short`]; two Vietnamese Zipformer transducers through the linked sherpa-onnx, 10.3 % and 11.2 % mean on the same FLEURS clips at 135 to 181x real time on 4 CPU threads, but one is CC BY-NC-ND 4.0 and the other has no stated training-data terms, and neither gives punctuation or casing [`vi-models` §3].

**Interpretation.** Large v3 wins only on the one Vietnamese far-field clip, by 5.6 points, at 0.21x Turbo's speed (22.4 / 4.6 = 4.87) and 1.9x its memory (3,812 / 2,007). On read Vietnamese and on English it is equal or worse, and it re-reads a lot (50 calls at one hour). The far-field report found no audio property (level, peak-to-speech) that separates the clip where Large v3 wins from those where it loses, so no automatic switch exists; the shipped change is a label, "Recommended for Vietnamese meetings. Slower." (commit `8a46f40`). **Provisional**: one 1,000-word reference, seeded from a Large v3 transcript (section 5).

**Limits.** Synthetic English WER is partly style (numbers as words). The older stored Large v3 transcript of Meeting A scores 12.6 % folded against the reference while a fresh Large v3 run scores 17.1 to 17.9 % on the clip; the far-field report attributed the 4.5-point gap to the real glossary and to reference seeding; the regression report (E2) later showed that a fresh full-file Large v3 decode without the lift scores 12.8 % exact, so the clip-against-file difference, not the glossary, explains most of it.

### E2. The quiet-input lift and the Large v3 regression

**Question.** Does lifting quiet input before Whisper help, and why did Large v3 get worse on Meeting A between 0.2.6 and main?

**Setup, round 0 (the lift is adopted).** The review stack [`review-stack`]: the lift (RMS under -26 dBFS lifted toward -20, at most +15 dB, per call) was the only option that improved Meeting A (no prompt 23.1 to 22.7 %; with a made-up prompt 25.6 to 22.4 %), FLEURS 4-min (10.8 to 10.4 %) and the noise set (8.4 to 7.6 %) while changing no other set. Shipped on by default (commits `f6fe726`, `315550a`, `9bb78e1`). Deletions fell (92 to 42) and insertions rose (31 to 56) on Meeting A.

**Round 1: the regression is found** [`regression`]. The full 23.4-minute file of Meeting A decoded by both daemons, first 300 s scored by timed cut:

| Run (full file, Large v3) | WER folded (sub / del / ins) | Turbo |
|---|---|---|
| v0.2.6 daemon | 12.6 (69 / 33 / 24) | 23.1 |
| main daemon, lift on | 18.2 (92 / 64 / 26) | 23.0 |
| `asr_bench` no lift | 11.9 (70 / 25 / 24) | 23.1 |
| `asr_bench` RMS-only lift, whole file | 16.6 (93 / 52 / 21) | 20.2 |
| `asr_bench` lift with peak limit (first fix) | 11.9 | 23.1 |

Cause: Meeting A has a low average (about -30 dBFS RMS per 5 min) but loud peaks (-4 dBFS; Meeting B reaches 0 dBFS), so a +10 to +12 dB lift pushes the loud voice past full scale. The stages after the first read change nothing on this audio (switching them all off gives the same 996 words). First fix: the gain never raises the largest sample past -1 dBFS (`normalize_rms_capped_peak`, commit `8c842f7`).

**Round 2: the first fix was not enough** [`regression` §"Second round"]. The daemon lifts *per call* (each 300 s piece), not once over the file. The whole-file `--gain-only` runs of round 1 had not lifted at all (the file's peak is near full scale), which is why they showed 11.9 %; a per-call run with the peak-limited lift still gave 16.2 % exact (95 / 43 / 24), matching the daemon (16.0 %). A small lift is a perturbation, and Large v3 changes words under any gain change on loud-peak audio. Second fix: lift only when the largest sample is at least 12 dB under the -1 dBFS limit, that is a really quiet recording (`min_headroom_db 12`, commit `2740f3c`).

| Model, first 300 s, exact (folded) | v0.2.6 daemon | main before fix | first fix | headroom rule |
|---|---|---|---|---|
| Large v3 | 12.8 (12.3), 70/33/25 | 18.4 (17.5), 92/65/27 | 16.0 | **12.8 (12.3)**, 70/33/25 |
| Turbo | 23.3 (22.7), 111/100/22 | 23.2 (22.5), 133/49/50 | 24.3 | **23.3 (22.7)**, 111/100/22 |

Regression size: +5.6 exact (18.4 - 12.8), +5.2 folded. After the headroom rule both models equal the 0.2.6 counts word for word in count. Other sets with the fix (Turbo): viu_a 10.1 vs 9.9, viu_b 7.9 vs 7.9, viu_c 13.7 vs 13.5, syn4 6.2 vs 6.2 (one-word differences). Where Turbo's lift insertions landed: of 49 inserted words with the old lift, 38 were at or above the median level, 2 under floor+6 dB, so the lift made Turbo write more words in loud speech (deletions 100 to 49, insertions 21 to 49), not over silence [`regression`].

**What was given up.** The lift's possible gain for Turbo on Meeting A (up to about 3 points, rough) and the 0.4-point FLEURS gain; the lift still applies to really quiet audio (the quiet-level sets have low peaks and are unaffected).

**The lesson, plainly.** The first diagnosis was wrong twice. (a) The far-field report compared the 300 s *clip* with the old binary and concluded "no step of the current pipeline makes Large v3 worse" [`far-field`]; the clip is one piece and decodes differently from the first piece of the full file (17.9 vs 12.6 %), so the comparison could not see the regression. (b) The round-1 whole-file `--gain-only` check did not exercise the daemon's per-call path, so the first fix looked complete when it was not. A regression check has to go through the exact call path of the product (`asr_bench --lift` now wraps the daemon's `Configured` per call) on the full file. Also: a "lift off" daemon run by the coordinator gave the same words as lift on, so that setting did not reach the decoder; the cause was not found in the code (`PipelineSettings::from_settings` reads `ai.lift_quiet_input`) [`regression`]. The release comparison then ran the 0.2.7 side through `asr_bench --lift` (the daemon's per-call wrapper) and cross-checked the 0.2.6 side against the coordinator's v0.2.6 daemon run (equal counts); a separate daemon run of Meeting A with the headroom build is not reported and stays a 0.2.8 follow-up [`release-cmp` §Setup].

### E3. Repetition loops: the structural repeated-sentence rule and the bare-token rule

**Question.** Can loops of a short sentence over real speech be removed without a phrase list, and do Whisper's own thresholds do it?

**Setup.** `loop_probe` on Meeting B's stored transcript (an earlier build), then a fresh decode with the rule (commit `d9c8bf2`); FLEURS and synthetic rows as the no-change check [`repeats`].

| Meeting B | Runs of 3+ in a row | Words in them | Words in 5+ within a minute | Sounds |
|---|---|---|---|---|
| stored transcript (before) | 4 | 78 | 60 | 1 (4.9 s) |
| decoded again with the rule | 1 | 18 | 0 | 0 |

The four stored runs [`repeats` §1]:

| Where | Units in a row | Words | Span | Words per second | Raw mic level | Speaker-engine speech under it |
|---|---|---|---|---|---|---|
| 3:21 | 16 (1 word each) | 16 | 26.6 s | 0.6 | -37 dBFS | 15.2 s |
| 3:53 | 9 (1 word each) | 9 | 14.5 s | 0.6 | -29 dBFS | 9.5 s |
| 13:23 | 7 (5 words each) | 35 | 9.0 s | 3.9 | -37 dBFS | 5.7 s |
| 16:57 | 3 (6 words each) | 18 | 0.9 s | **19.8** | -29 dBFS | 0.9 s |

The audio under them is not silent and the speaker engine hears 0.9 to 15.2 s of speech, so level alone cannot tell a loop from a repeat; the last run is physically impossible (18 words in 0.9 s). The one run left is a 6-word sentence 3 times at 5 words/s that both readings agree on [`repeats` §1, §3]. The 0.2.6 checks never see these (8 identical tokens needed; a quarter of a piece or 40 words for `is_degenerate`).

Whisper's own safeguards on the four stretches read alone (80 to 120 s each): defaults 0 / 1 run (24 words) / 1 (18) / 0; fallback off made the dense repeat worse (156 to 264 words) and added 36 dense words; log-prob -0.5 removed one run of four; none removed the 156-word dense repeat [`repeats` §3]. On quiet gated FLEURS the same thresholds changed nothing except log-prob -0.5 (31.1 to 29.1 % WER, within noise) [`safeguards`].

**Bare-token rule** (commit `56c6197`, [`far-field`]): one folded token 5 or more times in a row at over 7 words/s, or 8 or more under mean confidence 0.5, goes through the same re-read-or-sound path. On the stored Meeting B transcript the probe reports 0 bare runs after the new decode.

**No change on public rows**: FLEURS s_u1..4 and viu_a/b/c/h and syn4/syn6 found nothing to re-read (0 repairs, 0 words dropped) [`repeats` §3]; loop counters were 0 in every cell of every later run [`review-1-3`, `review-stack`, `params` except n_ctx 64 (1 loop) and beam 2/3 (1 to 2)].

**Limits.** The fresh decode has 7,749 words against 8,212 stored and is not the same run (different language list and build); with the rule off the same audio gave only 2 runs and 8 dense words, so the drop from 4 runs to 1 cannot be attributed to the rule alone [`repeats` §3, `brief` P2]. No one read the replacement text.

### E4. Coverage re-read: recovering skipped speech

**Question.** Does reading speech-like stretches that got no words again, as pieces of their own, recover them?

**Setup.** `vad::speech_regions` (energy, dips of 0.5 s bridged, 3 s minimum) and `Job::fill_gaps` (commits `4087eb5`, `fd8ad8f`, `c0cfd4e`): a stretch of 3 s or more with no word within 0.5 s, or a region whose words fill under a quarter of it, is read again without a prompt; kept when clean, not looping and mean confidence 0.4 or more; at most 20 per piece; what stays unreadable becomes a `[noise]` sound (upgraded to `[unclear]` where the speaker engine heard speech, commit `b6d93c0`) [`coverage` §1].

Quiet and gated FLEURS (Turbo, WER / coverage / uncovered s / calls) [`coverage` §2]: coverage rose in 4 of 12 cells by 1.0 to 4.9 points and WER fell in the same 4 by 0.8 to 4.8 points (viu_a -45 gated 26.9 to 24.0 %; viu_a -52 gated 42.9 to 38.1 %; viu_b -52 gated 42.7 to 41.0 %; viu_h -45 gated 44.8 to 44.0 %), the other 8 cells unchanged, none worse, 0 to 2 extra calls per clip. Clean clips: coverage 96 to 100 %, unchanged.

**The real case** [`release-cmp`]: Meeting C, window 89 to 173 s of a 330 s clip, 298 reference words:

| Row | 0.2.6 Turbo | 0.2.7 Turbo | 0.2.7 Large v3 |
|---|---|---|---|
| WER folded / exact (sub/del/ins exact) | 96.3 / 97.3 (32 / 258 / 0) | **28.9 / 29.2** (47 / 31 / 9) | 0.7 / 0.7 (0 / 1 / 1), reference-biased |
| words per 30 s, first three minutes of the clip | 46, 61, 15, 15, 15, 28 (180) | 46, 62, 80, 108, 107, 72 (475) | |

0.2.6 Turbo left 258 of 298 words out (86.6 %) and wrote 15 words per 30 s for about 100 s where 0.2.7 writes 80 to 108: 2.64x the words over the first three minutes. The Large v3 row scores 0.7 % because the reference *is* an older Large v3 output; it ranks nothing.

Pipeline counters on the current build (Turbo) [`invented` §5]: Meeting B 9 pieces, 46 gap re-reads (44 filled), 44 broken re-reads (43 kept); Meeting C 17 pieces, 3 loop splits, 55 gap re-reads (51 filled), 47 broken (44 kept); Meeting A 5 pieces, 1 split, 6 gap re-reads (6 filled), 10 broken (4 kept). Most gap re-reads end with words: this is where both quiet speech and invented text come from.

**Limits.** One private stretch; the public quiet cells are synthetic gain and gating; the 1 h FLEURS row was not re-run after the gap step [`coverage` §3].

### E5. Whisper safeguards and the decode-parameter search

**Question.** Which decode parameters (only what is passed to Whisper) lower meeting WER or invented text while keeping read speech and English?

**Setup.** `DecodeParams` (commit `ce2c677`, default = shipped decode), `param_search` with one model load; T sets: Meeting A 300 s, FLEURS viu_a/b/c pooled, syn4, noise set; H sets: viu_h, AMI IS1009b, PriMock day3, Meeting C window, first 10 min of Meetings B and C (silent-word counts only). Gate: held-out exact WER within 0.5 point on FLEURS and English, silence insertions and loops not up, RTF at least 0.8x; promoted only if it also helps the meeting rows [`params`].

**Inert axes** (bit-identical text on every T set, Turbo): log-prob -0.7 / -0.5, no-speech 0.4, their combinations, entropy 2.0, `best_of 5`, `max_initial_ts 0.5`, `length_penalty 0.6`, `suppress_nst` off, `suppress_blank` off [`params` table 1]. On this audio no window crosses the thresholds in a way that changes text; whisper.cpp applies `no_speech_thold` only AND-ed with `logprob_thold`, so a confident invention never trips it [`plan` §0].

Post-merge Turbo cells (exact WER; i/d/s in the report) [`params` table 2]:

| Cell | Meeting A | FLEURS pooled | syn4 | noise | viu_h | AMI b | PriMock 3 | Meeting C window | noise outside words | silent words B/C (10 min) | calls | rtf* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 23.7 | 11.9 | 6.2 | 14.1 | 11.6 | 30.2 | 15.8 | 21.8 | 10 | 2 / 2 | 97 | 13.0 |
| temp_inc 0 | 21.7 | 12.9 | 6.2 | 14.1 | 11.2 | 29.4 | 17.2 | 19.1 | 10 | 2 / 16 | 113 | 10.2 |
| n_ctx 0 | 24.9 | 11.4 | 6.2 | 12.4 | 11.0 | 31.0 | 14.6 | 18.8 | 6 | 0 / 2 | 36 | 25.7 |
| n_ctx 64 | 29.2 | 15.6 | 6.2 | 14.1 | 11.2 | 31.2 | 17.2 | 24.5 | 10 | 2 / 12 | 80 | 18.6 |
| beam 5 | **21.4** | 12.6 | 6.1 | **8.5** | 15.7 | 29.1 | 13.4 | **17.4** | **0** | 0 / 1 | 77 | 4.7 |
| beam 5, temp_inc 0 | 21.4 | 12.6 | 6.1 | 8.5 | 12.6 | 29.1 | 13.4 | 17.4 | 0 | 0 / 0 | 82 | 5.4 |

Large v3 (three cells) [`params` table 3]: base Meeting A 15.8, FLEURS (a, b) 11.8, syn4 12.7, noise 18.8 (24 outside words), Meeting C window 2.7; `n_ctx 0` Meeting A 25.1 (+9.3), Meeting C window 21.8 (+19.1), FLEURS 9.7 (-2.1), syn4 6.2 (-6.5), noise 12.8 (10 outside).

**Verdicts** [`params` §"Decision"]: temp_inc 0 fails FLEURS (+1.0, deletions 15 to 33) and PriMock (+1.4) and raises silent words on Meeting C 2 to 16; n_ctx 0 disagrees between meetings (A +1.2, C -3.0) and fails AMI (+0.8); n_ctx 64 rejected (FLEURS +3.7, A +5.5, one loop); beam 5 helps A (-2.3), C (-4.4), AMI (-1.1), PriMock (-2.4) and removes the noise-set insertions (10 to 0) but fails viu_h (+4.1, insertions 6 to 26), FLEURS (+0.7) and runs at 0.36x the speed (4.7 / 13.0); beam 5 with temp_inc 0 is the closest candidate (viu_h +1.0, FLEURS +0.7, 0.42x speed) and still fails. **Nothing promoted.** Large v3's `n_ctx 0` split is real but it is read/English (better) against meetings (much worse).

**Other findings.** Large v3 invents *more* than Turbo on the noise set (24 vs 10 outside words), against the plan's H1 [`params` finding 2]. The made-up glossary prompt hurt English badly (16.0 vs 6.2 % on syn4) and `n_ctx 0` only "fixed" it by dropping the prompt [`review-1-3`, `review-stack`]. Earlier non-parameter options: flash attention 77 invented words on the noise set; Silero regions as decode unit 13.8 to 13.9 % on FLEURS vs 10.8 and 72 calls vs 4; high-pass hurt short FLEURS (12.1 to 12.9); the hallucination gate cost 2 to 4 points of real speech [`review-1-3`, `review-stack`]. Beam 3 / 5 on FLEURS in the first baseline: -0.4 to +5.4 points, 10 to 40 % slower [`vi-fleurs` §7].

**Not reachable or not run.** `carry_initial_prompt`, `suppress_regex` (whisper-rs 0.16 keeps `FullParams.fp` crate-private); beam or `audio_ctx` on re-reads only (the Transcriber boundary does not know which call is a re-read: needs a hook in `pieces.rs`); language from the loudest piece; Medium; Large v3 `n_ctx 32/64/128`; the real glossary (P6) [`params` §"Missing"].

**Careful re-reads (beam on re-reads only): measured, no gain, ships off.** The hook was built (`ai.careful_rereads`, commit `df98084`: beam 5 and no temperature fallback for the windows the pipeline reads again; `asr_bench` and `silence_probe --careful-rereads`, `19dfae6`) and measured with Turbo through the app's path [`release-cmp` §"Careful re-reads"]:

| Check | Baseline | Careful re-reads |
|---|---|---|
| public silence set (36 Vietnamese clips, 1,619 s of reference silence): units over silence (per hour) | 78 (173) | 78 (173) |
| same set, words over silence | 896 | 979 (+83) |
| Meeting C window, exact WER (sub/del/ins), 15 decode calls both | 29.2 (47/31/9) | 30.5 (52/24/15) (+1.3) |
| Meeting A 300 s clip, exact | 23.7 | 23.7 (no re-read happened, 1 call) |
| FLEURS viu_a / b / c, syn4 | 11.8 / 9.1 / 14.8, 6.2 | identical (no re-reads) |
| decode time, Meeting C clip | 31 s | 55 s (1.77x; machine noise is of the same order: 12 vs 30 s on Meeting A with no re-read, under contention) |

The beam search that cleaned the noise set in the parameter search does not change the units that the gap re-reads make here (the invented units sit *inside* the re-read windows and beam 5 writes them too). Not measured: the silence set for Turbo English and Large v3 Vietnamese, the two labelled raw-track instances through the daemon, English far-field WER with the flag, decode time per hour on an idle machine (0.2.8).

### E6. Audio preparation

**Question.** Which preparation of the audio lowers invented text without raising WER?

**Setup.** `prep.rs` (commit `741457a`, not wired) through `prep_probe`; Turbo, WER-exact by `wer_exact.py`; the noise-reduced copy of Meeting A made with the recorder's own RNNoise from its raw track; Meetings B and C have both tracks from the recorder [`prep`, `level`, `invented` §7, §11].

| Candidate | Result | Decision |
|---|---|---|
| (a) raw microphone instead of the noise-reduced copy | Meeting A: **23.4 % raw vs 36.4 % copy** (13.0 points; substitutions 132 to 230). `prep_probe` runs: Meeting B 8,878 vs 7,037 words (+26.2 %), word-free gaps of 5 s+ 14 vs 42; Meeting C 19,079 vs 15,633 (+22.0 %), gaps 17 vs 41. **The real daemon check on Meeting B** (Turbo, `vi`, both stored tracks, `ai.transcribe_raw_mic` on): raw 8,254 words, 0 loop runs, 1 sound, 689 caption lines; copy 7,726 words, 1 loop run, 1 sound, 651 lines: **+528 words, +6.8 %** (the report phrases it as 6.8 % fewer on the copy; 528 / 7,726 = 6.8 %, 528 / 8,254 = 6.4 %) [`release-cmp` §"Real daemon check"]. The `prep_probe` +26 % counted a different run and is superseded. The copy's room level is 13 to 45 dB under the raw track's; where the maintainer hears faint speech the copy has nothing. **But** on labelled silence (58 s) Turbo raw writes 56 words in 5 units vs 18 in 4 on the copy, and on a pure-background-noise clip of Meeting B the raw track brings back the outro-style sentence that the copy does not write [`prep` row a, `invented` §11] | **Decided Nov 4: ships OFF** (`ai.transcribe_raw_mic`, commit `4a82616`; the setting and `--original-audio` / `--denoised-audio` per run exist). The noise-reduced copy stays the default as in 0.2.6, because the invented sentence returns on the raw track and none of the veto signals (perturbation, cross-model, silence signature, flatness, Silero, speaker engine) meets the bar [`veto`] |
| (b) lift per speech region instead of per call | Meeting A 23.4 to 22.4 (-1.0); public pooled 10.52 to 11.44 (+0.92); viu_b 9.8 to 11.6, viu_h 11.6 to 15.5, s_u4 21.2 to 24.2; silence set no gain | rejected |
| (c) trim silences of 8 s+ to 2 s with an offset map | public speech rows identical; silence set 413 to 82 words (-80.1 %), 35 to 9 units (-74.3 %); **on raw meeting tracks it removes 58 s (B) and 196 s (C), overlapping 4 of 11 stretches the maintainer heard as speech** and 286 transcript words; on the copy it removes nothing | rejected as a flatness rule (quiet far-field speech is as flat as the room, about 0 dB SNR); kept for use behind a real speech detector |
| (d) pieces end just after speech | Meeting C words over labelled silence 17 to 2, Meeting B 1 to 15: the 15-word unit moves from one file to the other | neutral, not adopted |
| normalisation before Whisper (peak, loudness) on synthetic quiet clips | changes of -4.8 to +3.0 points, within the run-to-run spread of a gain change | not shipped [`level`] |
| VAD as cutter (Silero regions) | worse (E5) | not shipped |

Quiet input itself costs accuracy on Vietnamese (Turbo, folded WER / coverage, two FLEURS clips of the first duplicated-sentence cut, one synthetic English clip, 63 runs) [`level`]:

| Language | Speech level | Reading | No normalisation | Peak normalisation | Loudness normalisation |
|---|---|---|---|---|---|
| vi | clean | raw | 22.8 / 88.2 | 20.5 / 88.3 | 25.8 / 85.3 |
| vi | -38 dBFS | raw | 28.3 / 87.1 | 27.0 / 88.1 | 27.9 / 88.3 |
| vi | -38 dBFS | gated | 31.4 / 82.7 | 28.6 / 86.4 | 28.5 / 86.4 |
| vi | -45 dBFS | raw | 32.1 / 83.9 | 35.1 / 81.7 | 27.3 / 85.2 |
| vi | -45 dBFS | gated | 33.4 / 80.5 | 29.8 / 81.1 | 27.8 / 82.3 |
| vi | -52 dBFS | raw | 42.9 / 75.3 | 38.9 / 71.2 | 39.3 / 72.6 |
| vi | -52 dBFS | gated | 44.4 / 67.9 | 44.0 / 70.0 | 44.4 / 69.7 |
| en | clean to -52 dBFS | raw | 5.9 to 7.7 / 99 to 100 | 6.1 to 6.4 / 100 | 6.1 to 6.4 / 100 |
| en | -38 to -52 dBFS | gated | 6.0 to 8.7 / 98 to 100 | 6.2 to 11.8 / 90 to 100 | 6.1 to 18.0 / 92 to 100 |

The level trend (22.8 to 42.9 %, 20.1 points) is the only effect larger than the run-to-run spread of a gain change (several points). The modelled gating denoiser was worse than raw by 1.3 to 3.1 points and 3.4 to 7.4 coverage points on Vietnamese, which candidate (a) confirmed on real RNNoise output. The "-52 dBFS over -60 dBFS" cells mix low level with 8 dB of signal-to-noise ratio, which no gain can fix.

WER-exact per clip for candidates (b) and (c) (sub / del / ins) [`prep` §3]:

| Clip | base | region lift | trim | trim + region lift |
|---|---|---|---|---|
| Meeting A 300 s | 23.4 (132/44/58) | 22.4 (128/51/45) | - | - |
| FLEURS viu_a | 11.4 (45/4/10) | 11.2 (43/5/10) | 11.4 | 11.2 |
| FLEURS viu_b | 9.8 (49/3/4) | 11.6 (47/3/16) | 9.8 | 11.6 |
| FLEURS viu_c | 14.2 (63/7/12) | 13.9 (60/7/13) | 14.2 | 13.9 |
| FLEURS viu_h | 11.6 (58/2/4) | 15.5 (68/9/9) | 11.6 | 15.5 |
| FLEURS s_u1 / s_u2 / s_u3 / s_u4 | 43.6 / 4.5 / 2.1 / 21.2 | 43.6 / 4.5 / 2.1 / 24.2 | same as base | same as region lift |
| syn4 English | 6.2 (46/10/0) | 6.2 | 6.2 | 6.2 |
| public pooled (word-weighted) | 10.52 | 11.44 | 10.52 | 11.44 |

Silence set with the trim (Turbo vi, words / units over reference-silent audio) [`prep` §6]: long room tone -55 dBFS 75/9 to 17/1 (77.0 s removed); far field RT60 0.9 s 103/9 to 11/2 (52.5 s); quiet speaker 70/5 to 0/0 (49.5 s); gaps 25/2 to 18/2 (43.0 s); clicks and breaths 112/8 to 34/2 (37.5 s); far field RT60 0.5 s 28/2 to 2/2 (45.0 s); sum 413/35 to 82/9; music-like filler 0 s removed (not flat).

**Interpretation.** The raw track is where the quiet speech is, and also where the room tone is that the decoder turns into text. (d) showed that an invented unit's position depends on the gain of the whole piece (the lift is decided per call), so it is a chaotic event of the decoder, about one per labelled 50 s of quiet stretches, that cuts cannot remove [`invented` §10].

**Limits.** One 1,000-word reference (1 point = 10 words); the Meeting B/C evidence is word counts and listening labels, not WER; Turbo only (Large v3 cells not run); the detectors that labelled "silence" for the listening clips ran on the noise-reduced copy, which hides faint speech (a control called silent held a voice) [`invented` §6, §7].

### E7. Invented text

**Question.** Can the outro-style text over quiet audio be reproduced publicly, how often does it happen, and can any general signal veto it without deleting quiet speech?

**E7.1 Public reproduction** (`make_silence_set.py`, `silence_probe`, commit `4413405`) [`invented` §1, §4]. Units per hour of reference-silent audio, clips hit / total:

| Family | Silent min | Turbo vi | Turbo vi, lift off | Turbo en | Large v3 en | Large v3 vi |
|---|---|---|---|---|---|---|
| tail (30 s window, 10 to 20 s speech then silence) | 4.1 | 29 (1/14) | 29 | 0 | 0 | 29 |
| end31 (speech past 30 s, 4 s silence) | 0.1 | 0 | 429 (1/2) | 0 | 0 | 0 |
| lead (20 s silence before speech) | 1.4 | 249 (2/2) | 83 | 38 | 75 | 124 |
| gaps (speech, gap, speech, gap; 90 s) | 5.9 | 41 (3/6) | 41 | 56 | 75 | 51 |
| long (15 s speech, 105 s silence) | 8.3 | 231 (5/5) | 231 | 206 | 135 | 333 |
| far (synthetic room RT60 0.5 / 0.9 s, -35 dBFS) | 4.5 | 240 (4/4) | 253 | 110 | 147 | 213 |
| quiet (-45 dBFS over -55 room tone, 24 to 30 s gaps) | 2.6 | 345 (3/3) | 369 | 132 | 242 | 345 |
| **all** | 27.0 | **171** (18/36) | 169 | **109** (16/36) | **111** (18/36) | **194** (18/36) |

By silence kind (Turbo, vi / en): room tone -45 dBFS 157 / 113; -55 dBFS 174 / 104; **-60 dBFS 0 / 0**; room tone with clicks and breaths -50 dBFS 153 / 104; music-like tones -45 dBFS 82 / 163; plus or minus one LSB noise 82 / 103; **digital zero 0 / 0**. So it needs something faintly audible and a long enough stretch; exact silence and very quiet tone do not trigger it; the lift is not the cause (169 vs 171); Large v3 does not fix it (194 vs 171 on Vietnamese). Each Vietnamese insertion is a whole sentence with all words on one time stamp. The 9-clip noise set of Nov 3 had not reproduced it (0 words over silence) because its windows were 30 s with the speech in the middle [`review-1-3`, `invented` §1].

**E7.2 The real instances** [`invented` §5, §6, §11]. Units with two of three evidence shares (energy, Silero, speaker engine) under 0.25 (2 s+, 4 words+): Meeting B stored 3 of 468 units, Turbo now 0 of 521, Large v3 now 0 of 567; Meeting C stored 0 of 892, Turbo now 0 of 982; Meeting A 0 in all three. The three instances the maintainer named in the stored Meeting B transcript do not reappear in the current build's Turbo decode (0, 0, 0 words at those times; Large v3 1, 3, 0). Their positions relative to the 300 s pieces are at the chance rate (H2, H5: piece ends and padding, not supported). Listening labels from 32 clips (three batches): labelled silence 58 s (B 24.6 s in 5 spans, C 33.4 s in 4); confirmed invented units on the current build: **2** (a cough window of 6 words at confidence 0.52; a background-noise clip of 7 words at 0.67), against **19** real quiet-speech units. Over the 53 s of batch 1 the current Turbo build wrote 18 words in 4 units (271.7 per hour from essentially one 15-word instance; 67.9 per hour without two edge words); the 95 % interval for one event in 53 s is roughly 2 to 380 per hour [`invented` §6].

**E7.3 Signal separation, 2 invented vs 19 real** (values over the words' span, AUC with invented higher, 2 x 19 pairs) [`invented` §11, `veto` §3]:

| Signal | Invented (two values) | Real min / median / max | AUC |
|---|---|---|---|
| energy share (copy) | 0.42, 0.58 | 0.21 / 0.42 / 1.00 | 0.59 |
| Silero share (copy) | 0.00, 0.00 | 0.00 / 0.16 / 0.32 | 0.08 |
| speaker-engine share (copy) | 0.32, 0.53 | 0.00 / 0.36 / 0.64 | 0.62 |
| RNNoise voice probability (raw) | 0.35, 0.58 | 0.10 / 0.38 / 0.65 | 0.66 |
| spectral flatness (raw) | 0.022, 0.0014 | 0.0003 / 0.0013 / 0.0033 | 0.82 |
| voiced share (raw) | 0.66, 0.53 | 0.37 / 0.43 / 0.67 | 0.84 |
| words per second | 3.3, 3.7 | 1.7 / 4.2 / 7.6 | 0.21 |
| confidence | 0.52, 0.67 | 0.40 / 0.74 / 0.91 | 0.16 |
| level above local floor (dB) | 5.7, 10.2 | 2.6 / 6.2 / 10.3 | 0.66 |
| (A) perturbation agreement, mean of -6 dB / +150 ms / +noise | 0.77, 0.67 | 0.13 to 0.85 | 0.70 (stable, not unstable) |
| (A) perturbation agreement, minimum | 0.32, 0.00 | 0.00 to 0.74 | 0.29 |
| (B) Turbo vs Large v3 agreement | 0.32, 0.00 | 0.00 to 0.87 | 0.18 |
| (B) Turbo raw vs Turbo copy agreement | 0.32, 1.00 | 0.00 to 0.30 | 1.00 |
| (C) overlap with the recording's quietest 30 s | 0.95, 1.00 | 0.00 to 1.00 | 0.96 (poisoned: the quietest 30 s of B holds 78 words of speech) |

Best single thresholds that catch both invented units: Silero 0 (flags 3 real), confidence under 0.668 (4 real), Silero under 0.04 (4 real). The flatness lead from batch 1 (one positive) failed on the second instance (0.0014, inside the real range). Fourteen two-signal rectangles catch both with 0 of 19 real in-sample, but **leave-one-out kills them**: of 219 and 464 rules fitted on one invented unit, only 13 and 4 catch the other [`invented` §11]. Rules from the perturbation and cross-model probes [`veto` §3]: models disagree (cross <= 0.35) catches 2 / 2 and flags 6 / 19 real; minimum perturbation agreement <= 0.35 catches 2 / 2, flags 9 / 19; both: 2 / 2, 5 / 19; the only 0-of-19 rule (stable under perturbation *and* models disagree) was found by search after seeing the 21 points, has a thin margin and fails leave-one-out.

**Why the public set does not transfer.** On the synthetic set (82 invented vs 186 real units) every signal separates: RNNoise AUC 0.06, voiced share 0.06, level 0.02, flatness 0.95, words per second 0.97, perturbation agreement 0.12 to 0.14, cross-model 0.00 vs 0.89 [`invented` §11, `veto` §3]. The synthetic silence is not voice-like; the two real instances sit on faint noise that two models both turn into speech-like text, and to the detectors they are as voice-like as the faintest real speech. So the public set measures whether a cure *removes* the failure it can make, not whether a detector can tell the real one from faint speech.

**Decision.** No veto implemented (`invented.rs` not written): the acceptance condition (both invented, 0 of 19 real, not fitted after the fact) is not met [`veto` §5]. The cheapest honest mitigation on offer, a product decision: on the raw track, mark a unit `[unclear]` (not delete) when the two models disagree, at the cost of 6 of 19 faint-speech units. **What would settle it**: a third and fourth labelled invented instance of the noise kind (the raw-track units with high RNNoise or Silero that the maintainer calls noise; batch 3, clips 111 to 118, is prepared), and real consented room tone for the public set [`invented` §9, `veto` §5, `plan` §5].

**E7.4 Side finding: collapsed word times on real speech** [`invented` §6]. Runs of 4+ words each 10 ms or shorter stacked on one instant are an alignment failure on *real* speech (the two collapsed clips the maintainer heard, 26 and 24 words, hold speech): Meeting B Turbo 24 runs (32.3 per hour, 2.5 % of words, largest 26), Meeting B Large v3 25 (33.6 per hour, 4.0 %, largest 65), Meeting C Turbo 20 (13.6 per hour, 1.0 %), Meeting A 6 (15.4 per hour) for both models. Not tied to piece ends (5 of 24 within 30 s of an edge, chance 0.22); a segment's words all get the segment end. Timing collapse is therefore not an invented-text signal. **The general fallback was built and ships on** (`ai.repair_word_times`, `wordtimes.rs`, commit `66654c6`): a stack of 4 or more words under 10 ms is spread between its neighbours in proportion to word length, at most 0.12 s per character, structure only. Counts reported by the coordinator on Nov 4 (not in a stored benchmark report): stacked words Meeting A 48 to 32 (-33.3 %), Meeting B 140 to 77 (-45.0 %), Meeting C 269 to 111 (-58.7 %); zero-length caption lines 0 before and 0 after, because `build_captions` already repaired them. The remaining 32 to 111 stacked words per meeting have no room between their neighbours and need a rule that may shift a neighbour (0.2.8) [`release-cmp` §"Left for 0.2.8"].

### E8. Post-ASR normalisation (AI-10)

**Question.** Can the local text model add punctuation, case and breaks without changing a word, and is Gemma 4 E2B enough?

**Setup.** Opt-in action `normalize_transcript` with job, CLI, MCP tool and sidecar (merge `4d8e7bb`); invariant: folded with tone marks kept, 0 substitutions and 0 deletions, insertions only punctuation; a failing turn is bisected down to 12 words and what still fails stays raw; `normalize_bench` with a real `llama-server`, temperature 0, JSON schema; **all timings under GPU contention** (load average 4 to 130) [`norm`].

| Run | Model | Words | Pieces | Rejected pieces | Rejected words | Period F1 | Comma F1 | Case acc. (no pass) |
|---|---|---|---|---|---|---|---|---|
| FLEURS vi, 1 sentence per turn | Gemma E2B | 1,872 | 74 | 2 (2.7 %) | 1.1 % | 0.822 | 0.729 | 0.953 (0.886) |
| FLEURS vi, 3 unrelated sentences | Gemma E2B | 2,739 | 67 | 5 (7.5 %) | 1.9 % | 0.660 | 0.608 | 0.943 (0.892) |
| FLEURS vi, 3 sentences | Qwen3.5 4B | 2,739 | 54 | 2 (3.7 %) | 0.8 % | 0.742 | 0.625 | 0.951 (0.892) |
| FLEURS en, 1 sentence | Gemma E2B | 1,298 | 69 | 3 (4.3 %) | 2.0 % | 0.919 | 0.777 | 0.968 (0.866) |
| FLEURS en, 3 sentences | Gemma E2B | 2,000 | 48 | 4 (8.3 %) | 1.4 % | 0.814 | 0.724 | 0.960 (0.866) |
| FLEURS en, 3 sentences | Qwen3.5 4B | 2,000 | 41 | 1 (2.4 %) | 0.5 % | 0.876 | 0.752 | 0.969 (0.866) |
| syn4 (reference over-commas) | Gemma E2B | 900 | 30 | 0 | 0 % | 0.945 | 0.479 | 0.993 (0.906) |
| private vi meeting, first 5 min | Gemma E2B | 962 | 32 | 4 (12.5 %) | 4.4 % | | | |
| private vi meeting, first 15 min (two meetings) | Gemma E2B | 2,508 / 2,612 | 76 / 104 | 7 (9.2 %) / 9 (8.7 %) | 2.1 % / 2.5 % | | | |

Tone-mark damage in accepted text: 0 words in every run. Without the bisect step the FLEURS vi rejections were 16.7 % of turns (1 sentence) and 48 % (3 sentences), nearly all the model "helping" (digits as words, typo fixes); the bisect and a "do not fix spelling, keep digits" rule took word loss to 1 to 2.5 % [`norm`]. The 5 % target is met by words, not by pieces, on Vietnamese meetings.

**Speed** (contended) [`norm`]:

| Audio | Model | Wall (contended) | Per audio minute |
|---|---|---|---|
| 5 min vi (private) | Gemma E2B | 66.8 s | 13.4 s |
| 15 min vi (private; the report's own label "A", not this study's Meeting A) | Gemma E2B | 115.8 s | 7.7 s |
| 15 min vi (private; the report's own label "B", not this study's Meeting B) | Gemma E2B | 153.9 s | 10.3 s |
| 8.7 min en (FLEURS, 1 sentence per turn) | Gemma E2B | 52.3 s | 6.0 s |

Server log for the 15-minute row: 38,432 prompt tokens at 1,231 tok/s and 7,579 generated tokens at 90 tok/s for 2,508 words, so 3.0 generated tokens per Vietnamese word (2.5 English) and about 15 prompt tokens per word (the instructions are re-sent in each of the 96 calls). That is 2 to 2.5x the 1-minute budget for 15 minutes; 1 h projected about 8 min (115.8 / 15 x 60 = 7.7 min; **PENDING**, not measured); 15 s about 2 s (projected). **Gemma vs Qwen**: Qwen3.5 4B better by 0.02 to 0.08 F1 and rejects about half as many words at roughly 25 to 30 % more time. Verdict: enough for English as an opt-in action; borderline for Vietnamese [`norm`].

**Limits.** The 3-sentence FLEURS turns are unrelated sentences (harder than real turns); only one question mark per set; the 50-turn human spot-check (`tests/manual/NORM-1.md`), paragraph breaks and the 1 h timing are not done.

### E9. Release comparison, 0.2.6 vs 0.2.7

**Setup.** `bench026` built in a worktree of tag `v0.2.6` (calls that tag's `transcribe_pieces`), `asr_bench --lift` for 0.2.7 (the daemon's per-call wrapper), `compare_releases.py` and `rescore`; full files decoded as the app does, scored by word time; no glossary; one run per cell; 25 clips x 3 metric rows = 75 table rows [`release-cmp`, final version, commit `1865d37`]. Cross-check: `bench026` on Meeting A equals the coordinator's v0.2.6 daemon run (Large v3 70/33/25, Turbo 111/100/22).

Exact WER % (sub/del/ins), folded after the semicolon; private rows are counts only; the 1 h FLEURS and Meeting C 88 min rows were run for 0.2.7 only:

| Row | 0.2.6 Turbo | 0.2.7 Turbo | 0.2.7 Large v3 | Large v3 vs 0.2.7 Turbo |
|---|---|---|---|---|
| Meeting A, 300 s, 1,000 ref words (seeded from Large v3) | 23.3 (111/100/22); 22.7 | 23.3 (111/100/22); 22.7 | 12.8 (70/33/25); 12.3 | -10.5 (reference-biased) |
| Meeting C window, 84 s, 298 words (reference = older Large v3 output) | 97.3 (32/258/0); 96.3 | 29.2 (47/31/9); 28.9 | 0.7 (0/1/1); 0.7 | meaningless |
| FLEURS vi 15 s (s_vi1, 25 words) | 24.0 (4/2/0); 20.0 | 24.0; 20.0 | 24.0; 20.0 | 0 |
| synthetic English 20 s (s_en2) | 4.3 (2/0/0) | 4.3 | 4.3 | 0 |
| FLEURS viu_a | 11.8 (46/5/10); 10.3 | 11.8; 10.3 | 10.8 (35/7/14); 10.1 | -1.0 |
| FLEURS viu_b | 9.1 (46/3/3); 7.4 | 9.1; 7.4 | 12.6 (56/12/4); 11.2 | +3.5 |
| FLEURS viu_c | 14.8 (65/7/13); 13.9 | 14.8; 13.9 | 17.4 (54/6/40); 16.8 | +2.6 |
| AMI ES2004b | 24.7 (55/186/20) | 24.7 | 27.0 (74/181/30) | +2.3 |
| AMI TS3003a | 41.2 (42/31/14); loops 2, dense 12 | 41.7 (41/31/16); loops 1, dense 10, sounds 2 | 37.9 (34/31/15); loops 1 | -3.8 |
| AMI IS1009b | 31.0 (51/239/15); 759 w | 30.2 (66/214/17); 786 w | 29.6 (36/242/13); dense 5 | -0.6 |
| PriMock day1 | 21.6 (58/120/19) | 21.6 | 25.5 (74/124/34) | +3.9 |
| PriMock day3 | 20.3 (46/54/40); loops 1, dense 25 | 17.2 (40/61/18); dense 7 | 17.0 (46/54/17); dense 5 | -0.2 |
| syn4 | 6.2 (46/10/0) | 6.2 | 12.7 (58/11/45) | +6.5 |
| noise set, 9 clips: words outside speech (sum) | 40 | 35 | 86 | +51 |
| Meeting B, 44.6 min: words / loops / dense / sounds | 8,378 / 3 / 35 / 0 | 8,695 / 0 / 0 / 1 | 9,138 / 0 / 0 / 0 | |
| FLEURS viu_1h (3,463 s, 7,963 ref words) | not run | 11.7 (646/146/136); 10.7 | 17.4 (712/136/539); 16.4 | +5.7 |
| Meeting C, 88 min: words / loops / dense / bare / sounds | not run | 18,765 / 0 / 0 / 1 / 1 | 18,828 / 0 / 0 / 0 / 0 | |

Per noise clip, words outside speech (0.2.6 Turbo / 0.2.7 Turbo / Large v3): pink -40 8 / 6 / 1; pink -45 7 / 0 / 15; pink -50 12 / 12 / 12; brown -40 0 / 0 / 14; brown -45 0 / 0 / 12; brown -50 6 / 0 / 14; pink -40 (06) 0 / 0 / 16; pink -45 (07) 7 / 17 / 0; pink -50 (08) 0 / 0 / 2. Speed under contention (rough): Turbo 4 to 35x, Large v3 1 to 12x; peak memory Turbo 1.9 to 2.6 GB, Large v3 3.7 to 4.2 GB [`release-cmp`].

**What the table shows** [`release-cmp` §"What the table shows"].
- **0.2.6 and 0.2.7 Turbo give the same text on every clean row** (FLEURS a/b/c, syn4, PriMock day1, AMI ES2004b, the two short clips, Meeting A): the decode is unchanged there. The only public row worse on 0.2.7 is AMI TS3003a by 0.5 point (one word).
- **0.2.7 Turbo is better where 0.2.6 failed**: PriMock day3 20.3 to 17.2 (-3.1; dense repeats 25 to 7, loops 1 to 0), AMI IS1009b 31.0 to 30.2 (-0.8; 786 vs 759 words, 27 more; the report's text says 25), the Meeting C window 97.3 to 29.2 (skipped speech recovered), Meeting B loops 3 to 0 and dense repeat words 35 to 0 (+317 words, +3.8 %).
- **Large v3 vs Turbo**: better only on Meeting A (-10.5, reference-biased) and slightly on AMI TS3003a (-3.8), IS1009b (-0.6) and PriMock day3 (-0.2); worse on FLEURS viu_b (+3.5), viu_c (+2.6), the 1 h FLEURS row (+5.7), syn4 (+6.5), PriMock day1 (+3.9) and AMI ES2004b (+2.3); 3 to 5x slower, 1.9x the memory; and it writes more words outside speech on the noise set (86 vs 35; 0.2.6 Turbo 40). The lift change moves single noise clips both ways (pink -45: 7 to 0; clip 07: 7 to 17).
- **Sounds**: 0.2.7 writes `[noise]`/`[unclear]` where it would otherwise invent words: 2 on AMI TS3003a, 2 on brown -40, 1 each on Meetings B and C.

**Interpretation.** 0.2.7 is a pipeline that reads what 0.2.6 skipped or looped on and leaves the rest untouched; it does not read better where 0.2.6 already read. The Turbo-vs-Large v3 gaps on both private rows are biased toward Large v3 by the references; on public data Large v3 is a worse general choice and a better one only on a minority of English far-field clips. Not done (0.2.8): speaker counts per release for the private bundles (only old stored diarization exists), insertion-over-silence on private audio (needs marked silent spans), a full-file route for the Meeting C window, a Large v3 row with an independent reference.

### E10. Word-time collapse, speaker-count retune, text side

**Word-time collapse**: see E7.4 (13.6 to 33.6 runs per hour on real speech, 1.0 to 4.0 % of words).

**Speaker finding** [`spk-retune`, `spk-windows`, `spk-3plus`]. Structural changes shipped (commit `1f89ba9`): no second engine run for a maximum (fold to the nearest kept speaker), and audio over 20 min runs in 10-minute windows linked by voice prints (Cancel noticed between windows). Synthetic checks [`spk-windows`]:

| Clip, setting | Fold (new) | Second run (0.2.6) |
|---|---|---|
| syn6 (6 voices, 844 s), maximum 2 | DER 62.1 %, 38.1 s | DER 62.2 %, 79.1 s |
| syn6, maximum 3 | 49.1 %, 53.5 s | 49.1 %, 90.6 s |
| syn4 (4 voices), maximum 3 | 44.0 %, 17.3 s (no fold needed) | 44.0 %, 15.8 s |

| syn60 (1 h, 6 voices) | Wall s | Found | Peak MB | DER |
|---|---|---|---|---|
| whole run (0.2.6) | 267.8 | 5 | 599 | 34.1 % |
| windows of 600 s, link 0.5 | 183.8 | 5 | 594 | 34.1 % |
| windows of 300 s | 185.7 | 4 | 568 | 48.2 % |

Same turns and DER in 0.69x the time; no memory gain shown at one hour on that clip (the 1.2 GB at one hour of `perf-oct29` was a real recording). Retune of distance and shift on real conversations:

| Distance, shift | Tuning DER (8 clips) | Held-out DER (8) | Long AMI windowed DER (3) | Meeting B found (6 people) | Meeting B whole-vs-window disagreement | Time (B) |
|---|---|---|---|---|---|---|
| 1.0, 0.1 (shipped) | **23.6 %** | 19.7 % | **11.5 %** | 7 / 7 | 17.2 % | 119 s |
| 1.0, 0.2 | 28.7 % | 18.4 % | 15.4 % | **6 / 6** | **10.5 %** | 60 s |
| 0.8, 0.1 | 22.5 % | 14.7 % | 15.1 % | 8 / 8 | 58.5 % | 119 s |
| 0.8, 0.2 | 21.8 % | **12.9 %** | 17.5 % | 7 / 8 | 36.3 % | 60 s |

**Defaults kept (1.0, 0.1)**: no pair meets the criterion (do not worsen the real-meeting count, hold DER on held-out and long meetings). (0.8, 0.2) had briefly shipped (commit `261b905`) and was reverted (`dfeaf10`); (1.0, 0.2) is offered as a speed choice (0.50x the time, right count on B, 5.1 points worse on tuning, 3.9 on long AMI). Single clips swing by tens of points, so differences of a few points on 3 meetings are weak [`spk-retune`]. Synthetic 3 to 6 voices at the shipped threshold: DER 26.7 / 44.0 / 36.7 %, right count 0 of 3; 0.8 gives 6.8 % mean there, which is why synthetic voices must not set the threshold [`spk-3plus`]. Short clips (20 to 27 s, 2 and 3 voices) find 1 speaker [`short`].

**Suggested pauses** (adaptive rule, commit `ea01efc`): at -45 dBFS the 0.2.6 rule put 236 of 251 and 706 of 770 speech seconds inside suggested cuts; the adaptive threshold (40 % of the way from the noise floor to the speech level) brings that to 0.0 to 0.5 s, all 17 and 16 wanted pauses still found; the call site now reads the raw track and vetoes pauses with transcript words and speaker turns [`cuts`].

**Reports and translation** (text model, briefly) [`text-base`, `text-m2c`, `text-oct30`]: generation 72 to 98 tok/s, prompt 800 to 1,360 tok/s; `summary` 9.0 s (4 min) and 12.9 s (14 min); `minutes` 44.7 s and 268 to 309 s; translation split-and-retry fixed `output_too_long` (1 h, 299 lines: 694 s, 225 calls, 18 lines untranslated, no failure); a merge request over the 16,384-token context killed the 1 h `minutes` report after 1,666 s and is fixed (commit `318f387`), re-run **PENDING**; prompt processing is 5 to 26 % of wall time, so prompt reuse was not built; the rubric floor passes 3 of 4 cells (the 6-voice summary named 5 of 6 speakers). **Import**: 9.1 s per GB for a usable video, 1.6 s per audio hour for AAC, 0.3 s for a 15 s memo [`import`].

## 4. Synthesis

### 4.1 What changed for users in 0.2.7, with evidence strength

| Change | Evidence | Strength |
|---|---|---|
| Speech that got no words is read again; a long skipped stretch comes back | Meeting C window 97.3 to 29.2 % exact, 180 to 475 words in 3 min; quiet FLEURS 4 of 12 cells better, none worse | **moderate** (one private stretch with a biased reference; synthetic quiet cells) |
| Sentence loops over real speech are re-read and, if they persist, written as `[noise]`/`[unclear]` | Meeting B 4 runs / 138 loop words to 1 run / 18; 0 loops on every public row | moderate (fresh decode is not the stored run; rule's own share not separable) |
| Broken characters never reach captions | U+FFFD 2 to 0 and broken-and-doubtful 2 to 0 on PhoWhisper output; 3 U+FFFD in Meeting B's stored transcript | strong for the character rule, weak for the re-read's value |
| Transcript marks unreadable stretches as sounds instead of guessing | design; counters in job results | n/a (behavioural) |
| Suggest cuts no longer cuts quiet speech | 236 and 706 speech seconds inside cuts to under 0.5 s on synthetic quiet clips | strong on synthetic, unmeasured on the real meeting |
| Speaker finding: no second engine run, windows above 20 min, Cancel between windows | same DER, 0.48x and 0.69x the time on synthetic; windows better than whole on 3 real long meetings for the shipped pair | moderate |
| Translation no longer dies on a long reply; big reports no longer fail at the merge | 1 h translation finished; bug reproduced and unit-tested | strong (functional) |
| WER-exact reported next to folded | code and tables | strong |
| Opt-in "tidy" normalisation | 0 tone-mark damage; 1 to 4.4 % of words left raw; period F1 0.82 vi / 0.92 en on single sentences | moderate; speed 2 to 2.5x over budget |
| Collapsed word times are spread between neighbours | stacked words Meeting A 48 to 32, B 140 to 77, C 269 to 111 (coordinator's counts, no stored report); caption lines already 0 zero-length | moderate for the count, unmeasured for caption placement |
| Large v3 labelled "Recommended for Vietnamese meetings. Slower." | one private clip, 10.5 exact points in the final comparison; worse on 6 of 11 public rows | weak |
| Loops and dense repeats on real English conversations | PriMock day3 loops 1 to 0, dense 25 to 7; AMI TS3003a dense 12 to 10; Meeting B loops 3 to 0, dense 35 to 0 | moderate (3 clips, one private file) |

### 4.2 What did not improve, frankly

- **Turbo word accuracy on Meeting A is unchanged**: 23.3 % exact in 0.2.6 and 0.2.7, the same 111/100/22 counts [`release-cmp`]. Where the old pipeline already read the speech, nothing in 0.2.7 reads it better.
- **Invented text over near-silence** is reproduced (171 / 109 units per hour) and not cured; the plan's target of 2 per hour is not approached by any shipped or measured option [`invented`, `params`, `prep`, `veto`].
- **No decode parameter survived** the gate [`params`], and **beam search on re-reads only gives nothing**: 78 vs 78 invented units, +83 words, +1.3 points on the Meeting C window, 1.77x the decode time on that clip [`release-cmp`].
- **The raw microphone track**, which recovers +6.8 % words on Meeting B and 13 points on Meeting A against a denoised copy, cannot be the default because it brings the invented sentence back over noise; it is a per-run option.
- **Far-field Vietnamese WER** stays around 23 % (Turbo) to 13 to 17 % (Large v3) on one reference; read speech was already at about 10 %.
- **Speaker count** on the real meeting stays 7 for 6 people at the shipped settings [`spk-retune`].
- **Collapsed word times** on real speech (1 to 4 % of words) are only partly repaired: the spread fixes stacks that have room between their neighbours (48 to 32, 140 to 77, 269 to 111 stacked words on A, B, C); 32 to 111 per meeting remain [`invented` §6, `release-cmp` §"Left for 0.2.8"].

### 4.3 Decisions taken (maintainer, Nov 4, 2026)

1. **Quiet-input lift on, limited to really quiet audio** (headroom 12 dB); the possible Turbo gain on Meeting A (about 3 points, one file) is given up; a per-region lift is the 0.2.8 way to get it back [`regression`, `release-cmp` §"Left for 0.2.8"].
2. **Raw microphone is not the default** (`ai.transcribe_raw_mic` off, commit `4a82616`, after `66654c6` and spec `f498994` had briefly made it the default): +6.8 % words on Meeting B through the daemon, 13 points on Meeting A against a denoised copy, but the invented sentence returns over a noise-only clip and no veto signal (perturbation, cross-model, silence signature, flatness, Silero, speaker engine) meets the bar [`veto`, `release-cmp`]. The noise-reduced copy stays the default as in 0.2.6; `--original-audio` / `--denoised-audio` choose per run.
3. **Careful re-reads off** (`ai.careful_rereads`): measured, no gain [`release-cmp`].
4. **Word-time repair on** (`ai.repair_word_times`): stacks of 4+ words under 10 ms are spread.
5. Hallucination gate, voice-model opinion and no-carry-over are settings, off [`review-stack`].
6. No decode-parameter change; `DecodeParams` plumbing opt-in only [`params`].
7. Speaker defaults 1.0 / 0.1 kept; windows and folding on [`spk-retune`].
8. No normalisation before Whisper; no Silero as cutter; no flash attention [`level`, `review-stack`].
9. **Normalisation (AI-10) ships as an opt-in action without UI** (`capturemann ai normalize`, MCP tool, sidecar file) [`norm`, CHANGELOG 0.2.7].
10. Large v3 Turbo stays the default model; Large v3 carries the "Recommended for Vietnamese meetings. Slower." label (commit `8a46f40`).
11. PhoWhisper not offered (60.9 % WER with the shipped decode; needs its own decode path); Vietnamese Zipformer not adopted (CC BY-NC-ND, or unknown data terms) [`vi-fleurs`, `vi-models`].

### 4.4 Decisions still open (0.2.8)

| Decision | Evidence for | Evidence against | Needs |
|---|---|---|---|
| **Raw microphone as the default track** (decided off for 0.2.7) | +13.0 points on Meeting A against a synthetic denoised copy; +6.8 % words on Meeting B through the daemon; word-free gaps 42 to 14 (B) and 41 to 17 (C) in the probe runs | the invented sentence returns over a background-noise clip; 56 vs 18 words over 58 s of labelled silence | a second consented recording with both tracks and a reference; Large v3 on the denoised/raw pair; a veto (third labelled batch, clips 111 to 118, not labelled yet) or acceptance of `[unclear]` on model disagreement (6 of 19 faint units lost) |
| **Careful re-reads** (decided off) | beam 5 removed the synthetic noise-set insertions in the parameter search | no gain on the silence set, +1.3 on the Meeting C window, 1.77x time | a narrower trigger (only windows the gate marks), the English and Large v3 silence rows, timing on an idle machine |
| **Large v3 recommended for Vietnamese meetings** (label shipped, provisional) | 10.5 exact points on Meeting A | both private references are Large-v3-biased; worse on FLEURS b, c and 1 h, syn4, PriMock day1, AMI ES2004b; invents more (86 vs 35 noise words; 194 vs 171 units/h); 3 to 5x slower, 1.9x memory | a second Vietnamese reference not seeded from a machine transcript (W10); rule: >= 3 exact points on both references with FLEURS-h within 2 [`plan` §2] |

## 5. Threats to validity

1. **Reference seeding bias.** Meeting A's reference was hand-corrected from an older Large v3 transcript; Meeting C's window reference *is* an older Large v3 output (Large v3 scores 0.7 % on it). Every Turbo-vs-Large v3 gap on private rows is biased toward Large v3 by an unknown amount; the plan calls it "a few points" [`plan` §7, `release-cmp`].
2. **Tiny n.** 2 confirmed invented units against 19 real labelled ones; one 298-word window (1 point = 3 words); one 1,000-word reference (1 point = 10 words); one meeting per language domain. The 95 % interval for one event in 53 s spans 2 to 380 per hour [`invented` §6].
3. **Single machine, contended GPU.** All numbers come from one M4 Pro; every Nov 4 RTF was measured with 3 to 6 other GPU jobs running (load average up to 270) [`params`, `norm`]. Determinism was shown on this Mac only.
4. **Synthetic noise against real rooms.** The silence set uses generated room tone, hum, clicks and synthetic impulse responses; its invented units are not voice-like, the real ones are; detectors that separate the synthetic ones do not transfer [`veto` §3]. The quiet FLEURS cells are gain scaling plus a modelled gating denoiser, not RNNoise [`level`, `coverage`].
5. **One private language domain.** Vietnamese business meetings with English terms, far-field laptop microphone; no public far-field Vietnamese set exists; FLEURS is read speech.
6. **Human labelling by one person.** All listening labels (32 clips) are the maintainer's; no second annotator.
7. **Detector-on-denoised-copy flaw.** The clips offered for listening were chosen by detectors run on the noise-reduced copy, which is 13 to 45 dB below the raw track; a control the detectors called silent held speech. The 19 "real" units are hard negatives selected for weak evidence, so false-positive rates against clear speech are not measured [`invented` §6, §7, `veto` §4].
8. **Tool mismatches found during the day.** `asr_bench --gain-only` lifted the whole file once while the daemon lifts per call (E2); `asr_bench` scored the 300 s clip while the daemon decodes five pieces; the first FLEURS clips held duplicated sentences (E1 correction); a coordinator's "lift off" run was not off [`regression`, `vi-fleurs` §7].
9. **Folded WER before Nov 4** hides tone-mark errors (0.3 to 1.4 points on these rows, 4.0 on a 25-word clip).
10. **Metrics that do not measure accuracy.** Coverage counts any word near speech (PhoWhisper had 100 % coverage at 60 % WER); the report rubric is a floor check; normalisation F1 on unrelated concatenated sentences is harder than real turns.

## 6. Open questions and next experiments (ranked)

| Rank | Experiment | Data needed | Decides |
|---|---|---|---|
| 1 | Third and fourth labelled invented instances of the noise kind (batch 3, clips 111 to 118, prepared, not labelled) and a re-run of the 2-vs-19 separation at n >= 4 | the maintainer's answers (keyed file) | whether any two-signal veto exists; the raw-track default |
| 2 | A second consented recording with both microphone tracks and a reference; Large v3 on the denoised/raw pair | a recording (W6) | the raw-track default |
| 3 | A second Vietnamese far-field reference not seeded from a machine transcript, with silent spans marked | about 3 maintainer hours (W10) | Large v3 for Vietnamese; the Turbo decision rule of the plan |
| 4 | Careful re-reads with a narrower trigger (only windows the gate marks); the English and Large v3 silence rows; timing on an idle machine | none new | whether beam can cut invented text at a bearable cost |
| 5 | Consented real room tone (10 x 60 s) published CC0, mixed into the silence set | the maintainer records | whether the public rate transfers to real rooms |
| 6 | The real glossary on and off on Meeting A (P6) | the glossary export (counts only) | H4; the 12.6 vs 17.1 % residue |
| 7 | Remaining word-time stacks (32 to 111 words per meeting with no room between neighbours): a rule that may shift a neighbour; measure caption placement | Meetings A, B, C counts | completeness of the timing repair |
| 8 | Per-region lift (W6) to recover the Turbo gain the headroom rule gave up (about 3 points on Meeting A, one file) | none new | the lift's second half |
| 9 | Normalisation: shorter instructions and prompt caching; the 1 h row; the 50-turn spot-check; a UI | none new | default-on and UI in 0.2.8 or not |
| 10 | Speaker counts per release on the private bundles; insertion-over-silence on private audio (marked silent spans); full-file route for the Meeting C window; a Large v3 row with an independent reference | marked spans; a reference | the rows the comparison could not fill |
| 11 | Large v3 on the audio-prep and silence-trim cells; Medium in the parameter search | GPU time | completeness of E5/E6 |
| 12 | Two-stream decode (mic and system separately); PhoWhisper's own decode path; WPE prototype | a recording with both tracks; converted weights | 0.2.8 items W13 |

## 7. Reproducibility appendix

### 7.1 Commits (main, `git log --oneline`)

| Commit | What |
|---|---|
| `39770b8` | tag `v0.2.6` |
| `4918021` | failure-kind counters and reproductions (M2d) |
| `b408b06` | broken-token re-read / clean / drop |
| `4087eb5`, `fd8ad8f`, `c0cfd4e` | coverage re-read: gaps, sparse regions, short words after long silence |
| `1d9180e`, `b6d93c0` | sound entries `[noise]` / `[unclear]`; daemon upgrade and exports |
| `d9c8bf2`, `040d984` | repeated-sentence rule; `loop_probe` |
| `56c6197` | bare-token impossible-run rule |
| `ea01efc`, `8722d0b` | adaptive pause threshold and speech veto |
| `1f89ba9`, `261b905`, `dfeaf10`, `ed5c380` | speaker windows and folding; retune shipped then reverted; 2x2 report |
| `a339302`, `d6674d7`, `918d3ee`, `f6fe726`, `315550a`, `9bb78e1` | segment stats; review options; Silero regions; gate and `Configured` lift; settings; the shipped stack |
| `8a46f40` | Large v3 label |
| `7ac19a1` | the quality plan |
| `4f77853` | WER-exact, `rescore`, `compare_releases.py` |
| `8c842f7`, `2740f3c`, `cef55a6` | lift peak limit; headroom rule; merge |
| `ce2c677`, `627b9f9`, `338c940` | `DecodeParams`, `param_search`; report; merge |
| `4413405`, `741457a`, `04b6d96`, `1bbbc86`, `c1da28b`, `54cd0bc`, `8d3b54c`, `2463d74` | silence set and probe; `prep.rs`; invented-text and audio-prep reports |
| `321798e`, `6db020f` | `invented_probe`, veto-signal report; merge (first HEAD of this study) |
| `66654c6`, `f498994` | raw microphone as the transcription default (later reverted); word-time repair (`wordtimes.rs`, `ai.repair_word_times`) |
| `df98084`, `19dfae6`, `13b32ed` | careful re-reads (beam 5, no fallback, on re-read windows; `ai.careful_rereads` off); example flags; merge |
| `d96e7e1` | this study, first version |
| `4a82616` | raw microphone is NOT the default (`ai.transcribe_raw_mic` off); CHANGELOG [0.2.7] |
| `407acfa` | 0.2.7 version bump |
| `1865d37`, `dbf74e3`, `b22553e` | final release comparison (75 rows), careful re-reads measured, 0.2.8 list; merge (second HEAD of this study) |
| `47d913e`, `14fa9a2`, `1a21881`, `c7f7991`, `4d8e7bb` | normalisation engine, action, bisect, report, merge |
| `318f387`, `aae8cd2` | report merge-request fix; M2c report |

### 7.2 Commands (all from the reports; models from the app's models folder, nothing downloaded)

```sh
export SHERPA_ONNX_LIB_DIR=packaging/sherpa/out/lib
cargo build --release -p cm-ai --examples
# sets
python3 -I tests/tools/make_synthetic_meeting.py --out D --name syn4 --speakers 4 --minutes 5 --seed 3
python3 -I tests/tools/concat_utterances.py --tsv unique.tsv --out D --name viu_a --secs 240 --skip 0
python3 -I tests/tools/make_noise_burst_set.py ...
v/bin/python -I tests/tools/make_silence_set.py --out S --utterances list.tsv --lang vi --en-wav syn4.wav --en-lines syn4.lines.json
python3 -I tests/tools/make_reference_clips.py ami|primock --out D ...
# transcription rows
target/release/examples/asr_bench --audio X.wav --reference X.txt --speech X.speech.json \
  --model ggml-large-v3-turbo.bin --model-id large-v3-turbo --language vi --label X [--lift] [--decode "beam=5,temp_inc=0"]
target/release/examples/param_search --model M --model-id large-v3-turbo --sets sets.json --cells cells.tsv --vad ggml-silero-v5.1.2.bin --out r.jsonl
python3 tests/tools/param_search_table.py r.jsonl
target/release/examples/silence_probe --dir S --model M --model-id large-v3-turbo --language vi --only _vi_ --lift on --out t.jsonl
target/release/examples/prep_probe --audio X --model M --model-id large-v3-turbo --language vi --save out.json [--trim|--region-lift|--after-speech]
python3 -I tests/tools/wer_exact.py --reference ref.txt --transcript out.json [--secs 300]
target/release/examples/invented_probe --raw mic.wav --copy mic.denoised.wav --spans '[[a,b,label],...]' --turbo M1 --large M2 --language vi
target/release/examples/loop_probe ...; target/release/examples/rescore ...
# release comparison (v0.2.6 worktree: cargo build --release -p cm-ai --example bench026)
python3 tests/tools/compare_releases.py run --out DIR --rows ROWS.json; ... table; ... rescore
# speakers, pauses, text
target/release/examples/perf speakers X.wav --sherpa packaging/sherpa/out/models [--window-secs 600] [--shift 0.2]
target/release/examples/measure der ref.rttm.txt turns.txt
target/release/examples/quiet_silence --audio X.wav --speech X.speech.json --levels -30,-38,-45,-52 --rule adaptive
target/release/examples/text_bench --task report|translate ... ; cargo run --release -p cm-ai --example normalize_bench
```

**Rule for private data.** Audio, text and turn lists of a person's recordings never enter the repository or a report; only counts, rates and times do (`verification/benchmarks/README.md`). Listening answers are kept in a private keyed file read by `apply_silent_answers.py --keyed`. Public sets stay outside the repository and are regenerated or re-downloaded by the commands above.

### 7.3 Corrections: claims made during the day that later turned out wrong

| Claim | Where it was made | What is true now | Source of the correction |
|---|---|---|---|
| Turbo's re-read factor 2.0 to 3.2x and coverage 85 to 90 % on FLEURS show skipped speech | `vi-fleurs` §1, §6 | artifacts of duplicated sentences in the first clips; withdrawn; unique clips give 1 call, 96 to 100 % coverage | `vi-fleurs` §7 |
| Turbo Vietnamese read-speech WER is 22.8 % | `vi-fleurs` §1, `rel-plan` M2j | 10.6 % on unique clips (folded) | `vi-fleurs` §7, `vi-models` |
| "Invented text does not reproduce" on public data (0 words over 9 noise clips) | `review-1-3`, `safeguards`, `plan` §0 | 171 (vi) / 109 (en) units per hour on the silence set; the noise set's windows were too short | `invented` §1 |
| Large v3 has fewer insertions and is likely more robust over silence (H1) | `plan` §1 | Large v3 invents more: 24 vs 10 noise words, 194 vs 171 units per hour | `params` finding 2, `invented` §4 |
| "No step of the current pipeline makes Large v3 worse"; the old binary gives 17.9 % | `far-field` | the lift did: 12.6 to 18.2 % on the full file; "17.9 %" was the 300 s clip alone, not v0.2.6 | `regression` |
| The peak-limited lift fixes the regression (11.9 %) | `regression` round 1 | the whole-file run had not lifted at all; the per-call path still gave 16.0 to 16.2 %; the headroom rule fixes it | `regression` round 2 |
| A coordinator's daemon run with "lift off" | daemon run of Nov 4 | it gave the lift-on words: the setting did not reach the decoder; cause not found | `regression` |
| The outside review's advice to set `no_context` / `condition_on_previous_text=false` | review brief feedback | `no_context` is already true in whisper.cpp 1.8; the carry-over switch is `n_max_text_ctx`, which hurts the noise set and Large v3 | `review-1-3` §1, `plan` §0 |
| Folded WER is "the" WER for Vietnamese | all Nov 3 tables | it drops tone marks; exact is 0.3 to 1.4 points higher here | `plan` §0, `params`, `release-cmp` |
| The speaker engine separates the one known invented unit from quiet speech (27 % vs 71 %) | `review-1-3` §4, `plan` §5 | AUC 0.62 over 2 vs 19; overlaps | `invented` §11 |
| Spectral flatness separates invented text (one cough) | `invented` §6 | the second instance is inside the real range (0.0014) | `invented` §11 |
| Piece ends / zero padding / re-read spans cause the instances (H2, H5) | `plan` §1 | positions at the chance rate; cuts move the unit, they do not remove it | `invented` §5, §10 |
| Perturbation changes prior-driven text (plan signal 2) | `plan` §5 | the real invented units are stable under perturbation (0.77, 0.67); only the synthetic ones change | `veto` §3 |
| Timing collapse marks invented text | `plan` H8, gate design | collapse happens on real speech at 1 to 4 % of words | `invented` §6 |
| Speaker retune 0.8 / 0.2 is better (shipped `261b905`) | `brief` P5 | worse on the real meeting (7 to 8 speakers, 36 to 58 % disagreement); reverted | `spk-retune`, `dfeaf10` |
| "52 GB" of memory | `perf-oct29` | 48 GiB (51.5 GB decimal) | `system_profiler`, `hw.memsize` |
| 56 commits / 7,758 lines in `cm-ai` since 0.2.6 | `plan` §0 (at `7ac19a1`) | 85 commits / 12,331 lines at `6db020f` | `git` |
| The one-hour `minutes` report works | `text-oct30` (stand-ins) | it failed at the merge pass on a 1 h synthetic transcript (HTTP 400); fixed, not re-run | `text-m2c` |
| The raw track gives +26 % words on Meeting B (8,878 vs 7,037) and +22 % on Meeting C | `prep` row (a), the first version of this study (E6, abstract, 4.4) | those were `prep_probe` runs; the real daemon run on Meeting B gives 8,254 vs 7,726 words, **+6.8 %** (0 vs 1 loop runs); the Meeting C pair has no daemon run | `release-cmp` §"Real daemon check" |
| The raw microphone is the transcription default | commit `66654c6`, spec `f498994`, the plan's rank-1 acceptance ("adopt raw if it wins by >= 1 point on two recordings") | reverted the same day: `ai.transcribe_raw_mic` ships off because the invented sentence returns over noise and no veto exists | commit `4a82616`, `veto` §5 |
| Beam search on re-reads only would remove invented text at a fraction of beam's cost (4.4 of the first version; `params` finding 1) | `params`, the first version of this study | measured: 78 vs 78 units on the silence set, +83 words, Meeting C window +1.3, 1.77x time; stays off | `release-cmp` §"Careful re-reads" |
| Collapsed word times are "untouched" in 0.2.7 (4.2 of the first version) | the first version of this study | the repair ships on (`66654c6`); stacks 48 to 32, 140 to 77, 269 to 111 per the coordinator | commit `66654c6`, CHANGELOG 0.2.7 |

### 7.4 Pending results: filled on Nov 4, and what is left for 0.2.8

Filled in this version (the study's sections name the numbers):

| Item | Result | Section |
|---|---|---|
| Release comparison rows: FLEURS 4-min, AMI x 3, PriMock x 2, syn4, noise set, Meeting B 45 min, 1 h FLEURS, Meeting C 88 min | 75 rows, `release-cmp` final (`1865d37`) | E9 |
| Careful re-reads (beam on re-reads only) | no gain: 78 vs 78 units, +83 words, Meeting C window +1.3, 1.77x time; ships off | E5, 4.3 |
| Raw-microphone default | daemon check on Meeting B +6.8 % words; decided off for 0.2.7 | E6, 4.3 |
| Word-time repair | ships on; stacks 48 to 32 / 140 to 77 / 269 to 111 (coordinator's counts) | E7.4, 4.3 |
| Normalisation | ships as an opt-in action without UI | 4.3 |

Still pending (0.2.8 follow-ups, from `release-cmp` §"Left for 0.2.8" and the open items above):

| Item | Owner | Where it goes | Why it matters |
|---|---|---|---|
| Third listening batch (clips 111 to 118, prepared, **not labelled yet**) and the n >= 4 separation | maintainer, then mac-synth | `invented` / `veto` | any veto; the raw-track default (E7, 4.4) |
| Raw-microphone default: a second consented recording with both tracks and a reference; Large v3 on the denoised/raw pair | maintainer, mac-ai | new report | 4.4 |
| Careful re-reads: silence set for Turbo English and Large v3 Vietnamese; the two labelled raw-track instances through the daemon; timing on an idle machine; a narrower trigger | mac-ai | `release-cmp` successor | 4.4 |
| Remaining word-time stacks (32 to 111 words per meeting, no room between neighbours) | mac-ai | new report | E7.4 |
| Speaker counts per release for the three private meetings; insertion-over-silence on private audio (marked silent spans); full-file route for the Meeting C window; a Large v3 row with an independent reference | mac-ai, maintainer | `release-cmp` successor | E9 |
| Per-region lift (W6) to recover the Turbo gain given up by the headroom rule | mac-ai | new report | E2 |
| Daemon run of Meeting A with the headroom build (the comparison used the per-call harness, not the daemon) | mac-ai | `regression` | closes E2 |
| Second Vietnamese reference not seeded from a machine transcript; silent spans on Meeting A | maintainer | `plan` W10 | Large v3 for Vietnamese (4.4) |
| Real glossary on / off on Meeting A | maintainer export, mac-ai | `far-field` | H4; 12.6 vs 17.1 residue |
| Normalisation 1 h timing, 50-turn spot-check, paragraph breaks, UI | mac-norm, desktop owner, maintainer | `norm`, `NORM-1` | default-on decision for 0.2.8 (E8) |
| 1 h `minutes` report after the merge fix | mac-ai | `text-m2c` | functional check (E10) |
| Large v3 on the audio-prep, trim and silence-set cells; Medium in the parameter search; Large v3 `n_ctx` 32 / 64 / 128 | mac-ai | `prep`, `params` | completeness (E5, E6) |
| Uncontended RTF pass for any adopted change | mac-ai | the adopting report | every Nov 4 RTF is a rough ratio |
| Real daemon re-run of the broken-token repair on Meeting B (counts) | mac-ai | `broken` | whether the re-read replaces the garbage there (E3) |
