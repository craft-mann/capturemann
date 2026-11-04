# Audio preparation before decoding: raw or noise-reduced, lift per region, trimming long silences, pieces that end after speech

Date: 2026-11-04. Machine: Apple M4 Pro, shared with other agents (no real-time factors quoted). Model: `ggml-large-v3-turbo.bin`, Vietnamese rows with `vi`, English with `en`, prompt off. Branch `track/mac-synth` on `main` 7ac19a1; the candidates are in the new module `crates/cm-ai/src/prep.rs` (not wired into `pieces.rs` or `pipeline.rs`), run with the example `prep_probe`. WER-exact = case and punctuation folded, tone marks kept (`tests/tools/wer_exact.py`, own implementation). Wording: "measured".

## 1. Question and short answer

*Which preparation of the audio lowers the invented text over silence (the public set of the other report: 171 units per hour, Turbo, Vietnamese) without raising the WER on FLEURS, PAT and the long meetings?*

| Candidate | Result | Decision |
|---|---|---|
| **(a) Read the raw microphone track instead of the noise-reduced copy** | PAT 5-min reference: **23.4 %** WER-exact on the raw track, **36.4 %** on a copy made with the recorder's own RNNoise (13.0 points worse, substitutions 132 to 230). Nov 2 (44.6 min, no reference): 8,878 words on the raw track against 7,037 on the copy (+26 %), word-free gaps of 5 s or more 14 against 42; Data Platform (88 min): 19,079 against 15,633 words (+22 %), gaps 17 against 41. The copy's room level is 13 to 45 dB below the raw track's, and in the clips where the maintainer hears faint speech the copy has nothing. Words over the 53 s of labelled silence: Nov 2 raw 2 (1 unit) against 1 (1) on the copy; Data Platform raw 44 (3) against 17 (3). | **Adopt raw as the default, but not alone.** Gate (at least 1 point on two recordings, loses nowhere): met on PAT by 13 points; the second recording is supported by the +22 to +26 % words and the listening labels, but has no word reference. **Cost found afterwards:** on the maintainer's pure-background-noise clip (103) Turbo on the raw track writes the outro sentence, which it does not on the copy. Labelled silence (58 s): Turbo raw 56 words in 5 units against 18 words in 4 units on the copy. Raw therefore needs a veto against invented text (none exists yet, see the other report, section 11). |
| (b) Lift per speech region instead of per call | PAT: 23.4 to **22.4 %** (-1.0). Public: FLEURS vi 4-min a 11.4 to 11.2, b 9.8 to **11.6**, c 14.2 to 13.9, h 11.6 to **15.5**; short s_u4 21.2 to **24.2**; syn4 English 6.2 to 6.2. Pooled over the public sets 10.52 to **11.44** (+0.9). Insertions over silence on six clips of the silence set: 75 to 104, 103 to 106, 70 to 75, 25 to 30, 112 to 121, 28 to 19 words (no gain). | **Reject** as built: worse on 3 of 9 public clips by 1.8 to 3.9 points and no help on the silence set. |
| (c) Trim silences of 8 s or more to 2 s, with an offset map | On the FLEURS, syn4 and PAT clips: removes nothing, outputs identical to the baseline. On the silence set: removes 77 of 120 s (long, room tone -55), 99 s (lsb), 50 of 75 s (quiet), 52.5 of 100 s (far), 43 of 90 s (gaps); on music-like filler 0 (not flat). **On the raw meeting tracks it removes 58 s (Nov 2) and 196 s (Data Platform) of flat stretches, and the maintainer's labels say some of that is quiet speech**: removed ranges overlap 4 of the 11 stretches he heard as speech (Nov 2: 254 to 259 s, 470 to 475, 488 to 493, 497 to 502; Data Platform: 1838 to 1843) as well as 5 of his silent ones; 286 words of the raw-track transcript lie inside the removed ranges. On the noise-reduced copy it removes nothing. | **Reject** as a flatness rule: quiet far-field speech in the raw track is as flat as the room (about 0 dB signal to noise). A trim needs a real speech detector; safe only where a second independent signal agrees. Kept in `prep.rs` for that use. |
| (d) Pieces end just after speech (`plan_after_speech`) | Count of pieces ending inside a silence of 5 s or more: not measured on the meetings (their plans end in speech); with the made-up long audio of the unit test the tail drops from 10 s to under 1 s. On the real instances: Data Platform words over labelled silence 17 to 2 (the 15-word instance at a piece start disappears), Nov 2 1 to **15** (a 15-word instance appears in the cough window, mid-piece); total words 15,633 to 15,763 and 7,037 to 6,942. Pieces 17 to 18, 9 to 9. | **Neutral**: the invented unit moves, it does not go away (the lift is decided per piece, so a changed cut changes the gain of the whole piece). Not adopted. |

Bit-identical reruns still hold on this Mac (two decodes of the same Turbo Vietnamese grid gave 863 words over silence both times; the rows above that are equal between modes are equal to the word).

## 2. Setup and commands

```text
cargo run --release -p cm-ai --example prep_probe -- --audio <file> --model <ggml> --model-id large-v3-turbo \
    --language vi --save out.json [--trim] [--region-lift] [--after-speech] [--dry]
python3 -I tests/tools/wer_exact.py --reference <ref.txt> --transcript out.json [--secs 300]
cargo run --release -p cm-audio --example denoise_wav -- <in> <out.wav>   # the recorder's RNNoise over a raw track
```

The noise-reduced PAT copy was made with the recorder's own denoiser (48 kHz, `cm_audio::denoise::Denoiser`) from the raw `mic.wav`, so the two share content and level; the PAT reference is seeded from an older machine transcript, which favours the model that made it (Large v3), so a 13 point gap between two Turbo runs on it is a *difference between the inputs*, not a model comparison. Public data: FLEURS vi_vn (CC BY 4.0) four 4-minute clips and four short clips, synthetic English meeting syn4, and the generated silence set.

## 3. WER-exact, public sets and PAT (substitutions / deletions / insertions in brackets)

| clip | base | rlift | trim | trimrlift |
|---|---|---|---|---|
| PAT 5 min (private ref) | 23.4 (132/44/58) | 22.4 (128/51/45) | - | - |
| FLEURS vi 4 min a | 11.4 (45/4/10) | 11.2 (43/5/10) | 11.4 (45/4/10) | 11.2 (43/5/10) |
| FLEURS vi 4 min b | 9.8 (49/3/4) | 11.6 (47/3/16) | 9.8 (49/3/4) | 11.6 (47/3/16) |
| FLEURS vi 4 min c | 14.2 (63/7/12) | 13.9 (60/7/13) | 14.2 (63/7/12) | 13.9 (60/7/13) |
| FLEURS vi 4 min h | 11.6 (58/2/4) | 15.5 (68/9/9) | 11.6 (58/2/4) | 15.5 (68/9/9) |
| FLEURS vi short s_u1 | 43.6 (13/1/3) | 43.6 (13/1/3) | 43.6 (13/1/3) | 43.6 (13/1/3) |
| FLEURS vi short s_u2 | 4.5 (1/0/0) | 4.5 (1/0/0) | 4.5 (1/0/0) | 4.5 (1/0/0) |
| FLEURS vi short s_u3 | 2.1 (1/0/0) | 2.1 (1/0/0) | 2.1 (1/0/0) | 2.1 (1/0/0) |
| FLEURS vi short s_u4 | 21.2 (7/0/0) | 24.2 (8/0/0) | 21.2 (7/0/0) | 24.2 (8/0/0) |
| syn4 English 5 min | 6.2 (46/10/0) | 6.2 (46/10/0) | 6.2 (46/10/0) | 6.2 (46/10/0) |
| public sets pooled (word-weighted) | 10.52 | 11.44 | 10.52 | 11.44 |

## 4. The hooks for `mac-ai` (nothing is wired)

* Raw default: `cm_ai::recording::transcribe_recording` already picks the track by `original_audio`; flip the default (and say so in the job result and the AI page). Everything below is for the case that the copy stays.
* (b) would replace `Configured::prepared` (`pipeline.rs`): `prep::loud_regions` then `prep::lift_per_region`; it is rejected here.
* (c) would go in front of `plan` in `transcribe_pieces_resumable`: `let (copy, map) = prep::trim_quiet(...)`, decode the copy, map every word and sound with `map.to_source_secs`; only with a second speech signal.
* (d) would replace `plan` by `prep::plan_after_speech`.

## 5. What this does not show

One Vietnamese reference with 1,000 words (1 point is 10 words) that is seeded from a machine transcript; the second recording (Nov 2) has no reference, so (a) rests on PAT plus word counts and the listening labels; Turbo only (Large v3 on the same cells was not run for lack of GPU time); the silence-set rows for (c) are in section 6.

## 6. Silence set with trimming

Six Turbo Vietnamese clips of the silence set, words / units over reference-silent audio without and with the trim (silences of 8 s or more cut to 2 s; times mapped back):

| Clip | no trim | trim | seconds removed |
|---|---|---|---|
| long, room tone -55 | 75 / 9 | 17 / 1 | 77.0 |
| far field RT60 0.9 s, room tone -45 | 103 / 9 | 11 / 2 | 52.5 |
| quiet speaker -45 dBFS | 70 / 5 | 0 / 0 | 49.5 |
| gaps, room tone -45 | 25 / 2 | 18 / 2 | 43.0 |
| long, room tone with clicks and breaths | 112 / 8 | 34 / 2 | 37.5 |
| far field RT60 0.5 s, room tone -55 | 28 / 2 | 2 / 2 | 45.0 |
| **sum** | **413 / 35** | **82 / 9** | |

So on the public set the trim cuts invented units by 74 % and words by 80 % with the speech words unchanged (FLEURS, PAT-5-min and syn4 outputs are identical to the baseline because nothing is cut there; PAT trim 23.4 %, trim plus region lift 22.4 %). The same rule is unsafe on the raw meeting tracks (section 1, row c). The two facts together are the finding: a trim removes the *synthetic* failure, but in a real raw track quiet far-field speech and room tone are not separable by flatness; the veto has to come from a speech detector that is not fooled by 0 dB signal to noise, which none of Silero, the speaker engine or RNNoise's voice probability is (see the other report, section 6). Not run for lack of GPU time: Large v3 on these cells, the Data Platform file with trim, and the English silence clips.
