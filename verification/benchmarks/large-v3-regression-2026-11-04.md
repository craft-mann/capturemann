# Large v3 regression on the real meeting (v0.2.6 to main), 2026-11-04

Cause: the quiet-input lift (shipped in 0.2.7 stack) lifts by the RMS rule alone. The meeting has a low RMS (about -30 dBFS per 5 minutes) but loud peaks (-4 dBFS; Nov 2 reaches 0 dBFS), so a +10 to +12 dB lift pushes the loud voice far over full scale. Large v3 loses about 5 points on the real recording; Turbo is not affected in the daemon run.

Method (private PAT recording, first 300 s of the full 23.4 min file, 1,049 reference words, language vi, no glossary; counts only). `asr_bench` on the whole file with `--words-out`, words scored against the reference at the cut that fits the first 300 s (the output has text only, no times, so the cut is by word count, best of 940 to 1,040 words in steps of 5: a rough score, within about +/-1 point of a timed cut). The same decode code is used by the daemon's piece path: both cut 5-minute pieces; the clip alone (mic_300.wav) is one piece and scores differently (17.9 %), so rows below are the full-file runs.

| Run (full file) | Large v3 WER (sub / del / ins) | Turbo WER |
|---|---|---|
| v0.2.6 daemon (coordinator, timed cut) | 12.6 (69 / 33 / 24) | 23.1 |
| main daemon, lift on (coordinator) | 18.2 (92 / 64 / 26) | 23.0 |
| asr_bench, no lift | 11.9 (70 / 25 / 24) | 23.1 |
| asr_bench, lift (RMS rule) | 16.6 (93 / 52 / 21) | 20.2 |
| asr_bench, lift with peak limit (the fix) | 11.9 (70 / 25 / 24) | 23.1 |

Findings:
- The stages after the first read (repair, gap re-read, repeat rules, broken-token repair) change nothing on this audio: switching them all off gives the same words (996 words, 3 calls). Whisper parameters are the same in v0.2.6 and main (the diff of `whisper.rs` only adds options that are off by default), the piece plan is the same, and no dependency changed. The commit that introduced the lift is `f6fe726` (Configured wrapper) and its on-by-default setting `315550a`.
- The coordinator's "lift off" daemon run gave the same words as the lift-on run, so the setting did not reach the decoder in that run (or the lift was not off): something to check in how the key is written; not found in the code (`PipelineSettings::from_settings` reads `ai.lift_quiet_input`).
- Earlier far-field report: "the pre-stack binary gives 17.9 %" was the clip, not v0.2.6; the clip is a different decode than the full file (not explained; the clip is one piece, the file is five).
- Turbo: the daemon numbers 23.1 and 23.0 are equal in WER; the lift on the full file in asr_bench gives 20.2 (rough). With the fix Turbo goes back to 23.1 for this recording: the lift's possible benefit for Turbo on this recording is given up (up to about 3 points, one recording, rough score).

Fix: `measure::levels::normalize_rms_capped_peak`: the gain is the smallest of the target gain, the 15 dB cap and the headroom to -1 dBFS peak; none when there is no headroom. `Normalize.peak_limit_db` (default -1.0) used by `Configured::prepared`; `asr_bench --gain-only` uses it (`--no-peak-limit` for the old behaviour). Test with made-up tones: a -34 dBFS tone with a -4 dBFS click is lifted by at most 3 dB; a full-scale click is left alone; the existing lift tests pass (quiet audio still lifted).

Other sets with the fix (Turbo, lift with peak limit vs the old lift): FLEURS vi viu_a 10.1 vs 9.9 %, viu_b 7.9 vs 7.9 %, viu_c 13.7 vs 13.5 % (one-word differences), synthetic English syn4 6.2 vs 6.2 %. The quiet level sets are unaffected by the rule (their peaks are low). Not re-run: the noise set and the 15 s / 1 h rows.

Confidence: one recording for Large v3; the cause is shown (lift on and off on the same file, same code), the Turbo effect is rough.
