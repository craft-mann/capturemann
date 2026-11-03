# Level-aware transcription: quiet input, normalisation, raw or noise-reduced copy (0.2.7, M2g item 3)

- **Date:** 2026-11-03. **Status:** measured, one run per cell, one machine, synthetic scaling of public and synthetic speech. Not "verified".
- **Machine:** Apple M4 Pro, 14 cores, 48 GB, macOS 26.6.2, GPU on. **Model:** Large v3 Turbo. **Build:** branch `track/mac-ai`, release `asr_bench` with `--quiet-db`, `--gate`, `--normalize` (before the bad-token repair; the path measured is the shipped one).
- **Question:** when a recording is quiet, does peak or loudness normalisation before Whisper change WER and coverage, and is it better to read the raw microphone or a noise-reduced copy?

## 1. Short answer

- **Quiet input costs accuracy on Vietnamese, not on the synthetic English.** Vietnamese (two FLEURS clips, 8 minutes) with room noise at -60 dBFS: mean WER 22.8 % clean, 28.3 % at speech -38 dBFS, 32.1 % at -45, 42.9 % at -52 (raw, no normalisation). English synthetic speech stays at 5.9 % to 7.7 % at every level.
- **Normalisation: no consistent effect.** Loudness normalisation (loudest 30 % of 20 ms frames to -20 dBFS, peak limit -1 dBFS) changed Vietnamese WER by -0.4, -4.8 and -3.6 points at -38, -45, -52 dBFS and by +3.0 points on the clean clips; peak normalisation by -1.3, +3.0, -4.0 and -2.3. Two clips, one run each: a difference between two runs of the same audio at a different gain is itself several points (clean clip, peak: +/-5), so this does not support shipping normalisation.
- **Raw versus noise-reduced copy:** reading the (modelled) noise-reduced copy was worse than reading the raw signal at all three levels on Vietnamese: WER +3.1, +1.3, +1.5 points and coverage -4.4, -3.4, -7.4 points (no normalisation). English: mixed, within a few points. The "denoiser" is our gating model, not RNNoise, so this is a reason to repeat the comparison on real RNNoise output (the app writes both `mic.wav` and `mic.denoised.wav`), not to change the default.
- **Decision for the release: no change to what Whisper reads, no normalisation.** Next step if wanted: the same comparison on a real recording's two files, by the maintainer.

## 2. Setup and how to repeat it

```sh
asr_bench --audio <clip>.wav --reference <clip>.txt --speech <clip>.speech.json --model <ggml-large-v3-turbo.bin> --model-id large-v3-turbo \
  --language vi --quiet-db -45 [--noise-db -60] [--gate] [--normalize peak|loudness]
```

`--quiet-db X` scales the speech spans to X dBFS RMS and adds uniform room noise (-60 dBFS RMS); `--gate` passes the result through the gating-denoiser model (`measure::levels::gate_denoise`: frames under -50 dBFS pushed down 1.5 dB per dB, up to 40 dB); `--normalize` is applied last. Clips: `vi_a`, `vi_b` (FLEURS vi_vn, see the Vietnamese baseline) and `syn4` (synthetic English, 4 voices, 312 s). Levels -38, -45, -52 dBFS; 63 runs in all.

## 3. Results (mean WER % / coverage % over the clips; Vietnamese n=2, English n=1)

| Language | Level | Reading | No normalisation | Peak | Loudness |
|---|---|---|---|---|---|
| vi | clean | raw | 22.8 / 88.2 | 20.5 / 88.3 | 25.8 / 85.3 |
| vi | -38 | raw | 28.3 / 87.1 | 27.0 / 88.1 | 27.9 / 88.3 |
| vi | -38 | gated | 31.4 / 82.7 | 28.6 / 86.4 | 28.5 / 86.4 |
| vi | -45 | raw | 32.1 / 83.9 | 35.1 / 81.7 | 27.3 / 85.2 |
| vi | -45 | gated | 33.4 / 80.5 | 29.8 / 81.1 | 27.8 / 82.3 |
| vi | -52 | raw | 42.9 / 75.3 | 38.9 / 71.2 | 39.3 / 72.6 |
| vi | -52 | gated | 44.4 / 67.9 | 44.0 / 70.0 | 44.4 / 69.7 |
| en | clean | raw | 6.2 / 100 | 6.2 / 100 | 6.3 / 100 |
| en | -38 | raw | 7.7 / 99.0 | 6.1 / 100 | 6.1 / 100 |
| en | -38 | gated | 6.0 / 100 | 6.2 / 100 | 18.0 / 92.5 |
| en | -45 | raw | 6.3 / 100 | 6.1 / 100 | 6.1 / 100 |
| en | -45 | gated | 8.1 / 97.9 | 8.1 / 99.0 | 6.1 / 100 |
| en | -52 | raw | 5.9 / 100 | 6.4 / 100 | 6.4 / 100 |
| en | -52 | gated | 8.7 / 98.4 | 11.8 / 90.0 | 6.6 / 100 |

Whisper calls (re-reads) are the same order in all cells (1 to 7 per clip); the per-run numbers are in the JSON lines of `asr_bench`.

## 4. What this does not show

- Two Vietnamese clips and one English clip, one run each. Whisper's output changes with any change of the input (even a pure gain), so differences of a few points are noise; only the trend with level (22.8 to 42.9 %) is larger than that.
- The quiet versions are the clean clips scaled with synthetic noise, not a real quiet room; the gating model is ours, not RNNoise. The English clip is text-to-speech.
- Low level and low signal-to-noise ratio are mixed: at -52 dBFS over -60 dBFS noise the speech is 8 dB over the noise, and no gain change can fix that. Normalisation only addresses level.
- WER ignores tone marks.

## 5. Decision and follow-up

No normalisation and no change of the track Whisper reads in 0.2.7. Open: the same comparison on real `mic.wav` / `mic.denoised.wav` pairs.
