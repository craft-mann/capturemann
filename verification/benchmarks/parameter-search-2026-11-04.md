# Parameter search for the Whisper decode (W3), 2026-11-04

Question: which decode parameters (only what we pass to Whisper) lower WER on far-field Vietnamese meetings, lower invented text over quiet stretches, and keep read speech and English? Machine: one Apple M4 Pro, shared with other agents. All numbers are **measured**, one run per cell (see determinism below); none is "verified". Private meeting rows give counts and rates only.

**Result in one line: no parameter set passes the hard gate, so nothing is promoted and no default or setting changes.** The `DecodeParams` plumbing and the search tools are committed (opt-in; default equals the shipped decode).

## Method

- Code: branch `track/mac-params`. Post-merge tables (2 and 3) were measured on commit `767212a` (main `cef55a6` merged in: the quiet-input lift only for really quiet audio). Table 1 was measured on `7ac19a1` plus my changes, with the old RMS-only lift that `mac-ai` identified as the Large v3 regression cause.
- Run = the app's own: `transcribe_pieces` over the quiet-input lift (`Configured` with `Normalize::default()`) over the model, via `crates/cm-ai/examples/param_search.rs` (one model load, many cells). Decode parameters come from `DecodeParams` in `crates/cm-ai/src/whisper.rs` (default = shipped decode; spec keys: best_of, beam, patience, temp, temp_inc, entropy, logprob, no_speech, length_penalty, max_initial_ts, n_ctx, audio_ctx, suppress_blank, suppress_nst, thold_pt, thold_ptsum, max_len, split_on_word). `asr_bench --decode "..."` takes the same spec.
- **WER-exact** (tone marks and the letter d-bar kept, NFC, case and punctuation folded) is computed in `param_search.rs` itself (measure.rs not touched by me); "fold" is the old `measure::word_errors`. Cells show `exact/fold (ins/del/sub)`.
- Sets. T (tuning): pat5 = first 300 s of the private meeting Meeting_with_PAT_TLSG (1,000 reference words); fleurs_vi = FLEURS vi viu_a,b,c pooled; en_syn4 = synthetic English meeting; noise = 9 noise-burst clips (30 s, one 10 s speech span, Vietnamese). H (held-out): viu_h (FLEURS), ami_b (AMI IS1009b), primock3 (PriMock day3 consultation05), hop_ref = private meeting "Hop du an EmeSoft Data Platform", first 600 s decoded, words from 89 to 173 s scored against the 298-word reference the maintainer confirmed (1 point = 3 words; never used alone), nov2_10 and hop_10 = first 10 min of two private meetings, no reference.
- Silence counters (by signals, not phrases): a "silent stretch" is at least 3 s where frame energy is under floor+0.3(p90-floor), Silero (v5.1.2, threshold 0.32) has no speech frame, and no speaker-engine span from the bundle's diarization.json (widened 0.5 s) covers it, trimmed 0.25 s each end. nov2_10 has 4 such stretches (15 s), hop_10 has 10 (50 s), pat5 none. `silent words` = words whose middle is inside one; `units/h` = clusters of 3 or more such words less than 2 s apart, per hour of audio. These stretches are short, so counts are small. `noise outside` = words further than 0.5 s from the speech span of the 9 noise clips (the public silence set that exists today; make_silence_set.py of plan section 5 does not exist yet).
- Determinism: the baseline was run twice on the 14 T sets (separate processes, different load): identical text hash on every set. So single runs are used.
- RTF (`rtf*`) = audio seconds over wall seconds of the whole cell, measured while 3 to 6 other Whisper/llama jobs and builds shared the GPU (load average 15 to 270 in the first table, 3 to 20 later). Only ratios inside one table mean anything, and those are rough. No separate uncontended RTF pass was done because there are no winners.
- Not reachable through whisper-rs 0.16: `carry_initial_prompt`, `suppress_regex` (FullParams.fp is crate private; needs a fork or a newer release; suppress_regex is a phrase mechanism anyway). `audio_ctx` on re-reads only: the pipeline does not tell the transcriber which call is a re-read (pieces.rs, not mine) and audio_ctx on the main path truncates 30 s windows, so P7 was not run. P9 (language from the loudest piece) lives in pieces.rs: not run. Beam only on fallback re-reads is not separable either; `best_of 5` (the sampling analogue) was measured instead.

## Table 1: Turbo, coordinate descent P1 to P8 (old always-on lift, commit 7ac19a1 + params), sets T

Relative table: all cells same code and session.

| cell | pat5 exact/fold (i/d/s) | fleurs_vi exact/fold (i/d/s) | en_syn4 exact/fold (i/d/s) | noise exact/fold (i/d/s) | viu_h exact/fold (i/d/s) | ami_b exact/fold (i/d/s) | primock3 exact/fold (i/d/s) | hop_ref exact/fold (i/d/s) | noise outside | loops | silent words (pat5/nov2/hop) | units/h nov2,hop | calls | rtf* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 8.1 |
| P1_lp-0.7 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 9.1 |
| P1_lp-0.5 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 5.7 |
| P1_ns0.4 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 7.2 |
| P1_lp-0.7_ns0.4 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 10.1 |
| P1_lp-0.5_ns0.4 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 12.9 |
| P2_ti0.1 | 25.2/24.5 (79/42/131) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 12.6 |
| P2_ti0 | 21.5/21.0 (40/50/125) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 17 | 12.1 |
| P2_ent2.0 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 17.2 |
| P2_ti0.1_ent2.0 | 25.2/24.5 (79/42/131) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 11.2 |
| P3_bo5 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 12.5 |
| P3_beam5 | 22.7/22.2 (62/34/131) | 11.6/10.4 (28/15/150) | 6.1/6.1 (0/10/45) | 14.5/14.1 (17/1/16) | - | - | - | - | 15 | 0 | 0/-/- | -/- | 16 | 10.2 |
| P4_nctx64 | 22.9/22.1 (30/70/129) | 12.1/10.9 (25/31/145) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 18 | 13.1 |
| P4_nctx0 | 21.3/21.0 (32/59/122) | 12.1/10.9 (27/18/157) | 6.2/6.2 (1/10/45) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 13.4 |
| P5_mits0.5 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 11.6 |
| P5_lpen0.6 | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 15.2 |
| P8_nst_off | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 13.0 |
| P8_blank_off | 23.4/22.7 (56/42/136) | 11.8/10.5 (26/14/157) | 6.2/6.2 (0/10/46) | 13.2/12.8 (12/1/18) | - | - | - | - | 10 | 0 | 0/-/- | -/- | 15 | 13.1 |

Reading:
- **P1** (logprob -0.7/-0.5, no_speech 0.4, combined), **P2 entropy 2.0** (alone and with temp_inc 0.1), **P3 best_of 5**, **P5** (max_initial_ts 0.5, length_penalty 0.6) and **P8** (suppress_nst off, suppress_blank off): bit-identical text to the baseline on every T set. On this audio no window crosses the thresholds in a way that changes the text, and no model-written non-speech tokens appear. These axes are closed: nothing to gain.
- **P2 temp_inc**: 0.1 is worse on pat5 (+1.8); 0 (no temperature fallback) is better on pat5 (-1.9, insertions 56 to 40, deletions 42 to 50), unchanged on FLEURS, English and noise.
- **P3 beam 5**: pat5 -0.7, FLEURS -0.2, English -0.1, noise set worse (13.2 to 14.5, outside speech 10 to 15 words).
- **P4**: n_ctx 0 pat5 -2.1, FLEURS +0.3, English and noise equal; n_ctx 64 pat5 -0.5 but deletions up (42 to 70).

## Table 2: Turbo, post-merge (767212a), T and H sets

| cell | pat5 exact/fold (i/d/s) | fleurs_vi exact/fold (i/d/s) | en_syn4 exact/fold (i/d/s) | noise exact/fold (i/d/s) | viu_h exact/fold (i/d/s) | ami_b exact/fold (i/d/s) | primock3 exact/fold (i/d/s) | hop_ref exact/fold (i/d/s) | noise outside | loops | silent words (pat5/nov2/hop) | units/h nov2,hop | calls | rtf* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 23.7/23.1 (31/92/114) | 11.9/10.5 (26/15/157) | 6.2/6.2 (0/10/46) | 14.1/13.7 (12/1/20) | 11.6/10.8 (6/1/57) | 30.2/30.2 (17/214/66) | 15.8/15.8 (18/51/50) | 21.8/21.5 (25/7/33) | 10 | 0 | 0/2/2 | 0/0 | 97 | 13.0 |
| ti0 | 21.7/21.1 (38/50/129) | 12.9/11.5 (26/33/155) | 6.2/6.2 (0/10/46) | 14.1/13.7 (12/1/20) | 11.2/10.3 (3/3/56) | 29.4/29.4 (15/225/49) | 17.2/17.2 (14/63/52) | 19.1/18.8 (23/6/28) | 10 | 0 | 0/2/16 | 0/6 | 113 | 10.2 |
| nctx0 | 24.9/24.6 (18/110/121) | 11.4/10.2 (26/14/150) | 6.2/6.2 (1/10/45) | 12.4/12.0 (8/1/20) | 11.0/10.1 (2/3/56) | 31.0/31.0 (11/265/29) | 14.6/14.6 (17/41/52) | 18.8/18.8 (12/14/30) | 6 | 0 | 0/0/2 | 0/0 | 36 | 25.7 |
| nctx64 | 29.2/28.1 (20/152/120) | 15.6/14.2 (24/63/172) | 6.2/6.2 (0/10/46) | 14.1/13.7 (12/1/20) | 11.2/10.5 (3/5/54) | 31.2/31.2 (17/259/31) | 17.2/17.2 (10/60/59) | 24.5/24.5 (12/29/32) | 10 | 1 | 0/2/12 | 0/6 | 80 | 18.6 |
| beam5 | 21.4/20.8 (50/45/119) | 12.6/11.5 (30/16/164) | 6.1/6.1 (0/10/45) | 8.5/8.1 (2/1/17) | 15.7/14.6 (26/3/58) | 29.1/29.1 (23/213/50) | 13.4/13.4 (22/20/59) | 17.4/17.4 (13/13/26) | 0 | 0 | 0/0/1 | 0/0 | 77 | 4.7 |

Beam variants (post-merge, same columns):

| cell | pat5 exact/fold (i/d/s) | fleurs_vi exact/fold (i/d/s) | en_syn4 exact/fold (i/d/s) | noise exact/fold (i/d/s) | viu_h exact/fold (i/d/s) | ami_b exact/fold (i/d/s) | primock3 exact/fold (i/d/s) | hop_ref exact/fold (i/d/s) | noise outside | loops | silent words (pat5/nov2/hop) | units/h nov2,hop | calls | rtf* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| beam3 | 23.6/22.8 (46/52/138) | 13.0/11.5 (44/17/155) | 6.4/6.4 (2/10/46) | 9.4/9.0 (2/1/19) | 17.1/16.2 (9/4/82) | 27.7/27.7 (23/210/39) | 16.1/16.1 (18/49/54) | 24.5/24.2 (20/24/29) | 0 | 2 | 0/0/4 | 0/0 | 113 | 5.1 |
| beam2 | 27.1/25.9 (53/79/139) | 12.4/11.2 (25/14/167) | 6.2/6.2 (0/10/46) | 32.5/32.1 (54/1/21) | 11.6/10.8 (5/4/55) | 27.2/27.2 (25/190/52) | 14.4/14.4 (16/37/55) | 29.5/29.2 (51/8/29) | 0 | 1 | 0/1/6 | 0/6 | 61 | 7.4 |
| beam5_lpen0.6 | 23.9/22.8 (43/56/140) | 11.7/10.4 (28/16/150) | 6.2/6.2 (0/10/46) | 14.5/14.1 (4/1/29) | 15.3/14.3 (19/3/63) | 29.1/29.1 (23/213/50) | 13.4/13.4 (22/20/59) | 33.6/33.2 (18/34/48) | 0 | 0 | 0/0/46 | 0/6 | 87 | 6.0 |
| beam5_ti0 | 21.4/20.8 (50/45/119) | 12.6/11.5 (30/16/164) | 6.1/6.1 (0/10/45) | 8.5/8.1 (2/1/17) | 12.6/11.7 (10/6/54) | 29.1/29.1 (23/213/50) | 13.4/13.4 (22/20/59) | 17.4/17.4 (13/13/26) | 0 | 0 | 0/0/0 | 0/0 | 82 | 5.4 |
| beam5_nctx0 | 23.8/22.9 (35/72/131) | 12.4/11.2 (40/16/150) | 6.4/6.4 (1/10/47) | 8.5/8.1 (2/1/17) | 11.9/11.2 (2/4/60) | 29.5/29.5 (19/225/46) | 12.2/12.2 (17/22/53) | 21.1/21.1 (12/15/36) | 0 | 0 | 0/0/16 | 0/6 | 58 | 8.5 |

## Table 3: Large v3, post-merge (767212a), three cells

fleurs_vi here = viu_a,b only (no viu_c); primock3 not run.

| cell | pat5 exact/fold (i/d/s) | fleurs_vi exact/fold (i/d/s) | en_syn4 exact/fold (i/d/s) | noise exact/fold (i/d/s) | viu_h exact/fold (i/d/s) | ami_b exact/fold (i/d/s) | primock3 exact/fold (i/d/s) | hop_ref exact/fold (i/d/s) | noise outside | loops | silent words (pat5/nov2/hop) | units/h nov2,hop | calls | rtf* |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 15.8/15.5 (32/42/84) | 11.8/10.7 (18/19/91) | 12.7/12.7 (45/11/58) | 18.8/17.9 (24/1/19) | 12.3/11.2 (13/5/50) | 29.6/29.6 (13/242/36) | - | 2.7/2.7 (1/5/2) | 24 | 0 | 0/3/25 | 0/12 | 78 | 2.2 |
| ti0 | 16.5/15.9 (32/53/80) | 11.8/10.7 (18/19/91) | 9.9/9.9 (26/10/53) | 18.8/17.9 (24/1/19) | 13.0/12.1 (16/2/54) | 29.6/29.6 (13/242/36) | - | 3.4/3.4 (5/3/2) | 24 | 0 | 0/3/10 | 0/6 | 77 | 1.9 |
| nctx0 | 25.1/24.1 (30/111/110) | 9.7/8.5 (13/9/84) | 6.2/6.2 (1/10/45) | 12.8/12.0 (10/1/19) | 11.2/10.1 (7/4/51) | 30.2/30.2 (14/254/29) | - | 21.8/21.8 (4/33/28) | 10 | 0 | 0/0/0 | 0/0 | 38 | 6.0 |

## Decision per the hard gate

Gate: held-out WER-exact within 0.5 point of the baseline (or better) on FLEURS vi AND the English sets; silence insertions not up on any row; loops not up; RTF at least 0.8 of baseline; promoted to default only if it also improves the meeting rows or removes invented text.

| Candidate | Fails on | Good on | Verdict |
|---|---|---|---|
| Turbo temp_inc=0 (no fallback) | FLEURS +1.0 (deletions 15 to 33), primock3 +1.4, hop_10 silent words 2 to 16 | pat5 -2.0, hop_ref -2.7, ami_b -0.8, viu_h -0.4 | not kept |
| Turbo n_ctx=0 | ami_b +0.8; pat5 +1.2 post-merge (it was -2.1 under the old lift) | hop_ref -3.0, primock3 -1.2, noise WER 14.1 to 12.4, noise outside words 10 to 6, silent words 0 | not kept (meeting rows disagree, ami_b over the line) |
| Turbo n_ctx=64 | FLEURS +3.7, pat5 +5.5, one loop | none | rejected |
| Turbo beam=5 | viu_h +4.1 (insertions 6 to 26), FLEURS +0.7, RTF ratio about 0.4 to 0.5 | pat5 -2.3, hop_ref -4.4, ami_b -1.1, primock3 -2.4, noise WER 14.1 to 8.5, noise outside words 10 to 0, silent words not up | not kept (RTF, Vietnamese read speech) |
| Turbo beam=5, temp_inc=0 | FLEURS +0.7, viu_h +1.0, RTF about 0.4 | pat5 -2.3, hop_ref -4.4, ami_b -1.1, primock3 -2.4, noise outside words 0, silent words 0/0/0 | closest candidate; fails the FLEURS 0.5 line and the RTF line |
| Turbo beam=5, n_ctx=0 | hop_10 silent words 2 to 16 | primock3 -3.6, noise outside 0, FLEURS +0.5 | not kept |
| Turbo beam=2,3; beam5 length_penalty 0.6 | worse than beam 5 on pat5/hop_ref (hop_ref up to +12), loops 1 to 2 | | rejected |
| Large v3 temp_inc=0 | pat5 +0.7, hop_ref +0.7, viu_h +0.7 | syn4 12.7 to 9.9, hop silent words 25 to 10 | not kept |
| Large v3 n_ctx=0 | pat5 +9.3, hop_ref +19 (deletions 5 to 33) | FLEURS 11.8 to 9.7, syn4 12.7 to 6.2, noise 18.8 to 12.8, noise outside 24 to 10, silent words 25 to 0 | the model split is real, but it is read/English (better) against meetings (much worse); not usable as a meeting default |

Findings that matter beyond the table:
1. **Beam search is the only thing that removes the invented text over quiet audio** (noise outside-speech words 10 to 0 on Turbo; silent words on the two meetings equal or lower), with no phrase list. It costs about half the speed and adds insertions on one FLEURS clip (viu_h). A narrower variant (beam only for suspicious windows) cannot be built from outside the decoder: whisper.cpp has no per-window strategy switch and re-reads are not distinguishable at the Transcriber boundary; it needs a hook in pieces.rs.
2. **Large v3 invents more than Turbo on the noise set** (outside-speech words 24 vs 10; WER 18.8 vs 14.1), against the plan's H1 assumption. n_ctx=0 halves it but wrecks meetings.
3. **Large v3 regression cause**: found by `mac-ai`, confirmed independently here: the RMS-only quiet-input lift, not a decode parameter. My no-lift Large v3 run on the pre-merge branch: viu_h 12.3 % (13 insertions) vs 34.3 % with the lift (137 insertions), viu_a 10.8 vs 14.9, pat5 18.2 vs 15.6 (the old lift helped the PAT clip). After the merge viu_h is 12.3 again.
4. The fallback thresholds are inert here (identical text); the temperature step is the only fallback control that changes anything, and fallback both helps (FLEURS, primock3, hop_10 silence) and hurts (pat5, hop_ref). A fixed no-fallback default is a trade, not a win.
5. The outro-style text the maintainer reports was not reproduced: in the first 10 minutes, Turbo baseline has 2 words in 15 s of signal-silent audio (nov2_10) and 2 words in 50 s (hop_10), 0 units per hour on both; Large v3 has 25 words and 2 units on hop_10. Removal could not be tested on the meetings with Turbo; the noise set is the only place invented text shows.

## What is wired, what is not

- Committed and wired (opt-in, nothing changes unless a spec is passed): `DecodeParams`/`Strategy` in `crates/cm-ai/src/whisper.rs` (serde, `parse`, `WhisperTranscriber::with_decode/set_decode/decode`; the existing `with_n_max_text_ctx` hook kept; tests: default is the shipped decode, spec parsing, JSON round trip), `asr_bench --decode`, `examples/param_search.rs`, `tests/tools/param_search_table.py`. No daemon setting added: **no survivor, so no `ai.*` key**. loop_probe unchanged.
- Not reachable: carry_initial_prompt, suppress_regex (whisper-rs fork). Not isolable from outside: re-read-only audio_ctx or beam, language from the loudest piece (pieces.rs).

## Missing / not done

- Large v3: n_ctx 32/64/128 started but stopped for the time box (no rows); only base, temp_inc=0, n_ctx=0 done; fleurs_vi has 2 clips, primock3 not run; P1, P3, P5, P8 not run on Large (inert on Turbo).
- Medium: not run. Turbo cells of Table 1 not re-run after the merge (inert ones) and temp_inc=0.1, entropy, best_of, max_initial_ts, length_penalty, suppress_* not on H sets.
- temp_inc=0 with n_ctx=0 post-merge: not run; pre-merge it equals n_ctx=0 on T and on H except hop_10 silent words (31 vs 5).
- No uncontended RTF pass; no second machine; the 1 h row and short clips were not used; the no-reference meetings were scored on their first 10 minutes only; make_silence_set.py (real room tone) does not exist, so the silence evidence is the 9 synthetic noise clips plus 65 s of signal-silent meeting audio.

## Reproduce

```
export SHERPA_ONNX_LIB_DIR=packaging/sherpa/out/lib
cargo build --release -p cm-ai --examples
# sets.json: [{"label","audio","language","reference"?,"speech"?,"rttm"?,"diar"?,"signals"?,"window"?}]; cells.tsv: name<TAB>spec
/tmp/cm-gpu nice -n 10 target/release/examples/param_search --model <ggml> --model-id large-v3-turbo \
  --sets sets.json --cells cells.tsv --vad <ggml-silero-v5.1.2.bin> [--only-sets a,b] [--no-lift] --out results.jsonl
python3 tests/tools/param_search_table.py results.jsonl
target/release/examples/asr_bench ... --decode "beam=5,temp_inc=0"
```
Scratch (not in the repo): sets, cells and JSONL in the session scratchpad folder mac-params/. Cells: base; logprob=-0.7|-0.5; no_speech=0.4; both; temp_inc=0.1|0; entropy=2.0; best_of=5; beam=2|3|5; n_ctx=0|64; max_initial_ts=0.5; length_penalty=0.6; suppress_nst=false; suppress_blank=false; beam=5 with temp_inc=0 | n_ctx=0 | length_penalty=0.6.
