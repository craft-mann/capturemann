# Transcription follow-ups after 0.2.7 (macOS and shared `cm-ai`)

Status: **draft** (Nov 4, 2026). Written from the 0.2.7 study (`docs/research/transcription-quality-study-0.2.7.md`) and its stored reports. The 0.2.8 plan is Windows ([`release-0.2.8-plan.md`](release-0.2.8-plan.md), its own session); this file lists the transcription work that rides along in `cm-ai`, to be run by a macOS agent in parallel so Windows is not delayed. Rules as in the 0.2.7 plan: general techniques only (no phrase lists, no channel names, no per-language blocklists, no one-recording fixes), local only, nothing is "verified" without a stored report, private meeting audio never enters the repository (counts only).

## 1. The problem to fix first: invented text over quiet or noisy stretches

The maintainer saw it again on the 0.2.7 release candidate (Nov 4, 2026): Turbo writes an outro-style sentence (an invitation to subscribe to a channel, thanks for watching) over stretches with no speech or with faint speech, confidently (word confidence 0.5 to 0.9). It is promotional text nobody said, so it is the most damaging transcript fault we know.

What 0.2.7 established (reports in `verification/benchmarks/`):

- It reproduces on **public, generated audio**: 171 invented units per hour of reference-silent audio for Turbo in Vietnamese, 109 in English, 194 for Large v3 in Vietnamese (`invented-text-repro-2026-11-04.md`). It needs something faintly audible (room tone at -45 to -55 dBFS, hum, clicks, music-like tones) over a long enough stretch; exact digital silence and -60 dBFS room tone give 0.
- **No single signal separates it from faint real speech** (two confirmed invented units, a cough and a noise-only stretch, against 19 real quiet-speech units): energy, Silero, the speaker engine, RNNoise, confidence, words per second, spectral flatness, perturbation agreement, cross-model agreement and a runtime silence signature all overlap or fail leave-one-out (`invented-veto-signals-2026-11-04.md`).
- Beam search is the only decode change that removed it on the public set, but it is 2 times slower and hurts clean read speech; using it only for re-reads gave no gain (`parameter-search-2026-11-04.md`, careful re-reads in the release comparison report).
- Reading the raw microphone brings the sentence back where the noise-reduced copy writes nothing, which is why `ai.transcribe_raw_mic` ships off.

## 2. Ordered work

| # | Work | Why | Gate |
|---|---|---|---|
| T1 | **PhoWhisper on the invented-text set.** Finish the ggml upload to `craftmann/phowhisper-ggml` (file ready, sha256 recorded in the 0.2.7 notes), pin the three `pending_pin` rows, and measure Vietnamese PhoWhisper small and medium on the public silence set and the labelled clips. It was trained on Vietnamese speech, not on web video with outro captions, so its prior may not contain the sentence. | The cheapest test of whether the fault is the model's prior. | Units per hour on the silence set (baseline 171) and WER-exact on the three references and FLEURS vi. |
| T2 | **An audio-tagging model as the independent signal** (an AudioSet-style classifier under an open licence, a few MB; the same model gives the planned `[laughter]` and `[whispering]` marks). A unit whose stretch has a low Speech probability and a high Cough, Noise or Music probability becomes `[noise]` (never deleted text, only a marker, like the 0.2.7 sounds). It is the one kind of evidence we have not tried: everything so far is a speech/non-speech detector or a Whisper opinion. | Cough and noise are exactly the two confirmed cases. | Removes both confirmed invented units, flags none of the 19 real quiet-speech units, then does not raise WER on FLEURS, the English sets, the 5-minute reference and the 84 s window, and cuts the public set's units per hour. |
| T3 | **More labels.** The third batch (clips 111 to 118 in `~/Movies/CaptureMann-review/silent-clips-3/`) is not labelled yet. Batches 4 and up only if a signal needs them (noise-like units with a high RNNoise or Silero value). | n = 2 invented units is too few to choose a rule. | At least 6 labelled noise-type invented units before any rule is fitted. |
| T4 | **A second consented recording with both microphone tracks and a corrected reference** (not seeded from a machine transcript). Decides the raw-microphone default and the Vietnamese default model. | One 1,000-word reference seeded from an older transcript carries every Large v3 conclusion. | Both models scored on two independent references. |
| T5 | **Mark, do not delete, when a model disagrees.** Turbo against Large v3 disagreement (0.35 or less) on a unit becomes `[unclear]`: it caught both invented units and flagged 6 of 19 real ones. Only if T1 and T2 fail; cost is a second model (3.1 GB) and extra decode time. | Cheapest mitigation that works on the labelled data. | The real units turned into `[unclear]` stay under 10 %, and the user can restore them. |
| T6 | **Show weak units in the editor** (a thin underline or a dimmed style for units with low speech evidence, with a one-click remove). | Even a good rule will miss some; the user needs the fastest way to delete a sentence nobody said. | Manual script with the maintainer; no new network or model. |

## 3. Carried over from 0.2.7 (not invented-text)

- Normalisation: an AI-page switch, a "Tidy" button, and an automatic mode that shows the raw text first and tidies turn by turn in the background; shorter instructions and prompt caching to cut the time (15 minutes of Vietnamese took 116 to 154 s under contention); try Qwen 3.5 4B as the Vietnamese model (`normalization-2026-11-04.md`); the 50-turn human check `tests/manual/NORM-1.md`.
- Quantised models (Turbo Q8_0, Large v3 Q5_0): pin only if within 0.5 point and 1.3 times faster.
- Two-stream decode (microphone and system audio decoded separately, merged by time).
- Word-time stacks with no room: 32 to 111 words per meeting remain; a better timing fallback.
- A per-region quiet-input lift for the possible 3-point Turbo gain on the 5-minute reference (it lost on FLEURS in 0.2.7).
- Speaker counts and invented-text counts for the private meetings once silent-span marks exist.
- Windows has no Metal: before recommending Large v3 or Turbo there, measure speed on the CPU (or the GPU backend chosen) on the lab laptop; the 0.2.7 numbers do not transfer.
- The 20-minute microphone check `tests/manual/M2H-LIVE.md` for transcription while recording.
