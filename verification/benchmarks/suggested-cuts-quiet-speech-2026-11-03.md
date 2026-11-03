# Suggested pauses on quiet speech (0.2.7, M2g)

- **Date:** 2026-11-03. **Status:** measured on synthetic recordings, deterministic, one machine (Apple M4 Pro; the numbers do not depend on it). Not "verified".
- **Build:** branch `track/mac-ai`, `crates/cm-ai/examples/quiet_silence.rs`, the silence rule of 0.2.6 (`suggest/silence.rs`, default options: -40 dBFS, 1.2 s minimum, 0.4 s kept).
- **Question:** does Suggest cuts (pauses) stay out of speech when the recording is quiet, and when a gating denoiser has pushed the quiet stretches towards digital silence (the shape seen on the maintainer's meeting, Nov 3, 2026)?

## 1. Before the fix (this section is the baseline)

**Short answer.** At speech levels of -45 dBFS RMS and below the 0.2.6 rule suggests cutting almost the whole recording. At -45 dBFS every suggestion (21 of 21, and 66 of 66 on a 14-minute clip) lies inside speech, and 94 % (236 of 251 s) and 92 % (706 of 770 s) of the speech seconds fall inside a suggested cut. At -52 dBFS it suggests one cut of the entire recording. At -30 dBFS (a normal level) it is fine (0 or 1 suggestions in speech, 0.4 s). The denoised variant behaves exactly like the raw one, because the noise-floor estimate falls (-75 dBFS here, about -83 on the real file) and the adaptive rule never raises the threshold above the fixed -40 dBFS.

### Setup and how to repeat it

```sh
python3 -I tests/tools/make_synthetic_meeting.py --out <dir> --name synp3 --speakers 3 --minutes 5 --seed 11 --max-gap 4.0   # also syn6 (see the transcription report)
cargo build --release -p cm-ai --example quiet_silence
target/release/examples/quiet_silence --audio <dir>/synp3.wav --speech <dir>/synp3.speech.json --levels -30,-38,-45,-52 [--noise -60] [--write <dir>]
```

The example scales the clean speech so the RMS over the speech spans is the stated level, adds uniform room noise at -60 dBFS RMS (`raw`), and builds `gated` by passing that through a gating denoiser model: frames below -50 dBFS are attenuated 1.5 dB per dB under the gate, up to 40 dB (noise floor about -75 dBFS, quiet syllables lowered by 4 to 9 dB). This is a model of the shape found, not RNNoise. A suggestion is *inside speech* when it overlaps a speech span (the generator's exact turn map) by more than 0.1 s. "Wanted" pauses are the gaps of at least 1.4 s between turns.

### Data

Synthetic only (macOS `say` voices; `synp3`: 305 s, 3 voices, 27 turns, gaps 0.3 to 4.0 s, 17 gaps of 1.4 s or more; `syn6`: 844 s, 6 voices, 16 such gaps). No private recording was used; the statistics quoted in the plan (171 pauses, 464 s, 150 words inside) come from the maintainer's own read-only analysis and are not reproduced here.

### Results (default options, current rule)

| Clip | Speech level | Variant | Median / p10 level | Suggestions | Inside speech | Speech s inside suggestions | Suggested s of recording |
|---|---|---|---|---|---|---|---|
| synp3 | -30 | raw / gated | -33.4 / -60.0 (-75.0) | 20 | 1 | 0.4 of 251 | 45 of 305 |
| synp3 | -38 | raw / gated | -41.4 / -60.0 (-75.1) | 20 | 7 | 1.6 of 251 | 47 |
| synp3 | -45 | raw / gated | -48.2 / -60.0 (-75.1) | 21 | **21** | **235.8 of 251** | **289** |
| synp3 | -52 | raw / gated | -54.2 (-60.4) / -60.1 (-75.2) | 1 | **1** | **250.4 of 251** | **304** |
| syn6 | -30 | raw / gated | -31.5 / -59.8 (-74.6) | 34 | 0 | 0.1 of 770 | 37 of 844 |
| syn6 | -38 | raw / gated | -39.4 / -59.9 (-74.7) | 38 | 5 | 1.5 of 770 | 43 |
| syn6 | -45 | raw / gated | -46.3 / -59.9 (-74.8) | 66 | **66** | **706.3 of 770** | **779** |
| syn6 | -52 | raw / gated | -52.6 (-56.6) / -60.0 (-74.9) | 1 | **1** | **770.1 of 770** | **843** |

Raw and gated give identical suggestions in every row (the figures in brackets are the gated values).

### What this does not show

Synthetic text-to-speech, a steady noise, one level per recording (real speakers vary within a meeting), no overlap. The gating model is ours. The real recording's level distribution is not reproduced, only its kind of failure.

## 2. After the fix (same clips, same example, `--rule adaptive`, the new default)

**What changed** (`crates/cm-ai/src/suggest/silence.rs`, `crates/cm-daemon/src/ai_engine.rs`; commits after the baseline):

1. The threshold follows the recording: 40 % of the way from the noise floor (10th percentile of the 20 ms frame levels) to the speech level (90th percentile), at least 6 dB above the floor, at most -20 dBFS. When the two are less than 8 dB apart the fixed 0.2.6 rule applies. `SilenceOptions.adaptive` (default on) turns it off. The 40 % is a choice made before the measurement (the middle of the usual gap between a noise floor and speech); it was not tuned on these clips and its sensitivity was not measured.
2. A veto: a suggested pause is cut back where the transcript's words or the speaker engine's turns (padded 0.15 s) say someone speaks; what is left stays only if it is still a pause of the minimum length.
3. The call site analyses `raw_file` of the microphone track when it exists (not the noise-reduced copy), and applies the veto with the transcript and `speaker_segments` when they exist. The veto is covered by unit tests only; the example below measures the level rule alone.

**Results** (default options, the new rule; the "before" column repeats section 1):

| Clip | Speech level | Variant | Suggestions (before) | Inside speech (before) | Speech s inside suggestions (before) | Suggested s (before) |
|---|---|---|---|---|---|---|
| synp3 | -30 | raw / gated | 20 (20) | 0 (1) | 0.0 (0.4) of 251 | 44 / 43 (45) of 305 |
| synp3 | -38 | raw / gated | 20 (20) | 0 (7) | 0.1 / 0.0 (1.6) | 44 (47) |
| synp3 | -45 | raw | 20 (21) | 3 (21) | 0.5 (**235.8**) | 45 (**289**) |
| synp3 | -45 | gated | 20 (21) | 0 (21) | 0.2 (**235.8**) | 45 (**289**) |
| synp3 | -52 | raw | 20 (1) | 6 (1) | 1.3 (**250.4**) | 47 (**304**) |
| synp3 | -52 | gated | 20 (1) | 4 (1) | 0.8 (**250.4**) | 46 (**304**) |
| syn6 | -30 | raw / gated | 31 / 29 (34) | 0 (0) | 0.0 (0.1) of 770 | 33 / 31 (37) of 844 |
| syn6 | -38 | raw / gated | 33 / 31 (38) | 0 (5) | 0.0 (1.5) | 35 / 33 (43) |
| syn6 | -45 | raw / gated | 34 (66) | 0 (66) | 0.2 / 0.0 (**706.3**) | 37 (**779**) |
| syn6 | -52 | raw | 37 (1) | 5 (1) | 1.3 (**770.1**) | 42 (**843**) |
| syn6 | -52 | gated | 35 (1) | 2 (1) | 0.5 (**770.1**) | 39 (**843**) |

At -45 dBFS the speech seconds inside suggested cuts fall from 236 and 706 to 0.0 to 0.5 (below 0.1 % of the speech); at -52 dBFS from the whole recording to 0.5 to 1.3 s (at most 0.2 %). The suggestions that still touch speech (2 to 6 per clip, at -45 and -52) overlap it by a few tenths of a second at a turn edge (the tail of a word at the generator's turn boundary), which the word and speaker veto of the real call site would remove. All 17 and 16 wanted pauses (gaps of 1.4 s or more) are still found (as before: the rule finds a pause when any suggestion overlaps it).

Unit tests (`suggest/tests.rs`): `m2g_quiet_speech_is_not_cut_raw_or_denoised` (two real pauses found, under 0.3 s inside speech, raw and gated), `m2g_the_fixed_rule_cuts_quiet_speech` (the baseline kept as a test), `m2g_a_loud_recording_keeps_its_pauses`, `m2g_speech_evidence_vetoes_a_pause_inside_it`; all 25 earlier suggest tests pass unchanged.

**Not shown.** Real recordings: the maintainer's meeting was not re-analysed, so nothing here says what the new rule suggests on it; the synthetic noise is steady and the speaker level constant. A recording whose speech and noise are less than 8 dB apart still uses the fixed rule and can still be cut wrongly. Level-aware transcription (plan M2g item 3) is not part of this report.
