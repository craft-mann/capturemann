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
