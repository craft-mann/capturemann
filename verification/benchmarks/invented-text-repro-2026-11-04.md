# Invented text over silence: a public reproduction, and where the real instances sit

Date: 2026-11-04. Machine: Apple M4 Pro, other agents' jobs on the same GPU (serialised with a lock; no real-time factor is quoted). Build: branch `track/mac-synth` on `main` 7ac19a1 (decode path of `main` unchanged; the new files are tools and examples). Models: `ggml-large-v3-turbo.bin`, `ggml-large-v3.bin` (the installed ones). Wording: "measured" throughout; nothing here is "verified".

## 1. Question and short answer

*Can a public, synthetic test make Whisper Large v3 Turbo write text over silence, and how often per hour of silent audio?* **Yes.** The generated set `tests/tools/make_silence_set.py` (FLEURS vi_vn speech, synthetic English speech, generated room tone, hum, clicks, breaths, music-like tones, generated room impulse responses) makes Turbo write sentences nobody said at **171 sentence units per hour of reference-silent audio in Vietnamese and 109 in English** (the decision rule of the plan is at most 2 per hour). Large v3 does it too in English (111 per hour). The Vietnamese text is the outro-style sentence the maintainer sees (an invitation to subscribe to a channel, a thank-you and goodbye line); the English text is "Thank you." in runs and short loops. The earlier noise set did not reproduce it because it had 30 s windows with the speech in the middle and no long silence after speech.

What provokes it, in order of strength (units per hour of silent audio, Turbo, both languages in one number where they agree):

| Construction | Turbo vi | Turbo en |
|---|---|---|
| speaker at -45 dBFS over room tone at -55, 24 to 30 s gaps (`quiet`) | 345 | 132 |
| 15 s of speech then 105 s of silence in a 120 s file (`long`) | 231 | 206 |
| far field: speech through a synthetic room (RT60 0.5 or 0.9 s), -35 dBFS, level steps, 26 to 30 s gaps (`far`) | 240 | 110 |
| 20 s of silence before the speech (`lead`) | 249 | 38 |
| speech, gap, speech, gap (`gaps`, 90 s) | 41 | 56 |
| a 30 s window with 10 to 20 s of speech then silence (`tail`) | 29 | 0 |
| speech running just past 30 s, then 4 s of silence (`end31`) | 0 | 0 |

By the kind of silence (Turbo, units per hour; all lengths together): room tone at -45 dBFS 157 (vi) / 113 (en); at -55 dBFS 174 / 104; at -60 dBFS **0** / **0**; room tone with clicks and breaths at -50: 153 / 104; music-like tones at -45: 82 / 163; plus or minus one LSB of noise: 82 / 103; **digital zero: 0 / 0**. So: it needs *something* faintly audible and a long enough stretch of it (30 s tails barely trigger it), it does not need a loud room, and exact digital silence does not trigger it. The quiet-input lift is not the cause: lift off gives 169 against 171.

## 2. Setup and commands

```text
python3 -m venv v && v/bin/pip install numpy scipy        # the generator needs both
v/bin/python -I tests/tools/make_silence_set.py --out <set> --utterances <list.tsv> --lang vi \
    --en-wav <syn4.wav> --en-lines <syn4.lines.json>      # 72 clips, 79 min, seed 1
cargo run --release -p cm-ai --example silence_probe -- --dir <set> --model <ggml> --model-id large-v3-turbo \
    --language vi --only _vi_ --lift on --show-text --out turbo_vi.jsonl
```

`silence_probe` runs the app's own path (`transcribe_pieces` over `pipeline::Configured` with the shipped quiet-input lift, prompt off) and counts words (tokens with a letter or digit) whose middle lies in a reference-silent span, and units (such words less than 1 s apart). Silence is everything outside the speech spans widened by 0.5 s (far-field clips: speech spans include a 0.7 s reverb tail). It can also count from a stored transcript and a span file (`--transcript ... --silent spans.json`).

Runs were bit-identical on this Mac: the Turbo Vietnamese grid was decoded twice (once on a first version of the set, once on the final one) and gave 863 words over silence both times.

## 3. Data

Speech: FLEURS vi_vn test (CC BY 4.0, utterances of 4 to 14 s, 90 of them used, picked by seed), and the lines of a synthetic text-to-speech meeting (no human voice) for English. Everything else is generated. MUSAN, DEMAND and OpenSLR RIRs were not fetched: network access exists, but the generated versions already reproduce the failure, so the 1.2 GB download was not needed. The set is not copied into the repository (regenerate it); clips are listed in `clips.json` next to the audio.

## 4. Results (units per hour of reference-silent audio; clips with at least one insertion / clips)

| family | silent min | Turbo vi units/h (clips hit/total) | Turbo vi, lift off units/h (clips hit/total) | Turbo en units/h (clips hit/total) | Large v3 en units/h (clips hit/total) |
|---|---|---|---|---|---|
| tail | 4.1 | 29 (1/14) | 29 (1/14) | 0 (0/14) | 0 (0/14) |
| end31 | 0.1 | 0 (0/2) | 429 (1/2) | 0 (0/2) | 0 (0/2) |
| lead | 1.4 | 249 (2/2) | 83 (1/2) | 38 (1/2) | 75 (2/2) |
| gaps | 5.9 | 41 (3/6) | 41 (3/6) | 56 (3/6) | 75 (4/6) |
| long | 8.3 | 231 (5/5) | 231 (5/5) | 206 (5/5) | 135 (5/5) |
| far | 4.5 | 240 (4/4) | 253 (4/4) | 110 (4/4) | 147 (4/4) |
| quiet | 2.6 | 345 (3/3) | 369 (3/3) | 132 (3/3) | 242 (3/3) |
| all | 27.0 | **171** (18/36) | **169** (18/36) | **109** (16/36) | **111** (18/36) |

Large v3 on Vietnamese: see section 4b if present (the run was queued behind other jobs when this was written).

Each insertion in the Vietnamese runs is a whole invented sentence in one place; the words of it get one and the same time stamp (zero length), as in the plan's reports. 18 of 36 clips in each language have at least one insertion; the 18 without are the 14 `tail` clips (except one), the two `end31` clips and the digital-zero or -60 dBFS fillers.

## 5. Where the real instances sit (W2): the three private meetings, counts and times only

Three meetings (Nov 2, 44.6 min, 9 pieces; the Data Platform meeting, 88 min, 17 pieces; the PAT meeting, 23.4 min, 5 pieces), each decoded now by this build with Turbo (and Large v3 for Nov 2 and PAT) through `silence_probe --audio`, and the three stored transcripts (made by earlier builds). Signals per unit (words less than 0.6 s apart): energy share of the pipeline's own speech regions, Silero v5.1.2 share, speaker-engine share (`diarization.json`), computed on the audio the model read (the noise-reduced copy for the two recorded meetings, which matters, see section 7).

| Transcript | units | units with two of three shares below 0.25 (2 s+, 4 words+) | with two of three below 0.4 (1.5 s+, 3 words+) |
|---|---|---|---|
| Nov 2, stored (earlier build) | 468 | 3 | 12 |
| Nov 2, Turbo now | 521 | 0 | 1 |
| Nov 2, Large v3 now | 567 | 0 | 0 |
| Data Platform, stored | 892 | 0 | 2 |
| Data Platform, Turbo now | 982 | 0 | 0 |
| PAT, stored / Turbo now / Large v3 now | 284 / 264 / 281 | 0 / 0 / 0 | 0 / 0 / 0 |

Placement of the 12 stored Nov 2 candidates relative to the 300 s pieces (the audio plan has pieces starting at 0, 291, 585, 875, ...): in the first 30 s of a piece 1, in the last 30 s 0, in the last 60 s 2 (17 percent; all units: 20 percent); the others sit 86 to 290 s from a piece end. The three instances the maintainer named (about 1:34, 4:19, 4:53) are the stored units at 94 to 96 s, 259 to 261 s and 285 to 286 s: none of them is near a piece end by more than the chance rate (piece ends at 291 and 585). **H2 and H5 (padding, piece ends, re-read spans) are not supported by these 12 + 3 instances**; the one current-build instance (section 6) is 1 s after a piece *start*, which is the other side of the same cut and is one case only.

Today's pipeline already differs from the stored one: at the three named times the current Turbo transcript has 0, 0 and 0 words (Large v3: 1, 3, 0). That is, on these meetings the build of this branch no longer writes those lines; the plan's insertion count for real audio is therefore small and cannot be measured without more labelled silence (section 6). Pipeline counters of the current runs (Turbo): Nov 2 pieces 9, loop splits 0, broken-reread 44 (43 kept), gap re-reads 46 (44 filled); Data Platform pieces 17, splits 3, broken 47 (44), gaps 55 (51); PAT pieces 5, splits 1, broken 10 (4), gaps 6 (6). Large v3: Nov 2 splits 8, gaps 34 (32); PAT gaps 4 (2). So most gap re-reads (energy but no words) end with words: that is where quiet speech and invented text both come from, and it is the place to look first if invented text shows up again.

## 6. The maintainer's listening test (22 clips from the three meetings)

Clip numbers refer to the first batch (old numbering); answers are kept by meeting and start second in a private file, and `apply_silent_answers.py --keyed` reads that form. Results:

* **Reference-silent spans from the answers**: 8 stretches, 53 s (Nov 2: 4 spans, 19.6 s; Data Platform: 4 spans, 33.4 s). Seven are detector cores in which nobody speaks (speech only in the 1 s padding around them), one is the whole window of the cough clip.
* **Words written over those 53 s, current build**: Turbo 18 words in 4 units (17 of them in Data Platform: one unit of 15 words with the same time stamp at 320.6 to 320.9 s, a window the maintainer called noise only, plus two single words on the edge of cores; 1 word at 100.0 s in the cough window), Large v3 (Nov 2 only) 0 words in 19.6 s. The earlier-build transcripts had 12 words in 5 units (Nov 2) and 16 words in 4 units (Data Platform). Per hour of silent audio that is 4 units / 53 s = 272 per hour (Turbo) or, without the two edge words, 68 per hour, from **one** real instance; the sample is far too small for a rate (a 95 percent interval for one event in 53 s is roughly 2 to 380 per hour). It is a lower bound of 1 confirmed current-build instance, 15 words, **1 s after the start of a piece**, with all words on one time stamp.
* **Confirmed invented, earlier build:** the cough clip (6 words, energy 0.42, Silero 0.00, speaker engine 0.32, confidence 0.52), plus the three named instances.
* **Real speech with weak evidence** (the false-positive pool for any rule): 11 candidates with energy 0.21 to 0.67, Silero 0.00 to 0.32 and speaker engine 0.00 to 0.42. Three of them have Silero 0.00 and one speaker engine 0.00. A control the detectors called silent (no energy, no Silero, no speaker engine for 3.9 s, level -77 dBFS in the noise-reduced copy) *does* hold speech: the detectors ran on the noise-reduced copy.

### Which general signals separate the invented positives from the real quiet speech

Positives: 1 confirmed (cough) + 3 named earlier instances + 1 current-build instance (the 15-word unit) = 5 weak positives against 11 real quiet-speech units. Counts only; nothing was tuned on them.

| Signal | Invented (cough 1) | Real quiet speech (11) | Verdict |
|---|---|---|---|
| Silero share | 0.00 | 0.00 to 0.32 | overlaps (3 real have 0.00) |
| speaker-engine share | 0.32 | 0.00 to 0.42 | overlaps |
| energy share | 0.42 | 0.21 to 0.67 | overlaps |
| the three named earlier instances: shares | energy 0.21 to 0.65, Silero 0.00 to 0.39, speaker engine 0.20 to 0.65 | same range | do not look weak at all |
| words per second | 3.3 | 2.6 to 4.3 (median unit 4.4) | no separation |
| model confidence | 0.52 | 0.40 to 0.89 | overlaps |
| RNNoise voice probability on the raw track (mean in the words' span) | 0.35 | 0.13 to 0.67 | overlaps |
| spectral flatness of active frames in the span (raw) | 0.026 | 0.001 to 0.006 for 10 of 11; 0.024 for one | separates the cough from 10 of 11, one real unit overlaps |
| collapsed times (4 words or more at under 10 ms each) | yes (15-word unit) | also yes in real speech (see below) | not a signal |

Honest reading: **no single signal separates them, and the gate that failed before would delete real speech** (Silero 0.00 for three real clips). Flatness (a noise-like burst such as a cough is flat, voiced speech is not) is the only one that separates the one cough from most real speech, on one positive; it needs many more labelled instances before it is anything more than a lead. A second batch of 10 clips (numbered 101 and up) was prepared for the maintainer in `~/Movies/CaptureMann-review/silent-clips-2/` with weaker-evidence units (low Silero and low speaker-engine share) and one isolated sound burst.

### Collapsed word times are an alignment failure on real speech

Runs of four or more words, each 10 ms long or less, stacked on one instant: the two collapsed clips the maintainer heard (26 and 24 words) are real speech. Rates: Nov 2 Turbo 24 runs (32 per hour, 2.5 percent of words, median run 5, largest 26), Nov 2 Large v3 25 runs (34 per hour, 4.0 percent, largest 65), Data Platform Turbo 20 runs (14 per hour, 1.0 percent), PAT Turbo 6 (15 per hour), PAT Large v3 6 (15 per hour); earlier-build transcripts 9, 48 and 6 runs. They are not tied to piece ends (5 of 24 within 30 s of a piece edge, chance 0.22) and the next word follows at a median gap of 0 to 0.6 s: a segment's words are all given the segment end. The cause is in the word-time step (DTW/token timestamps in `whisper.rs`, not touched here); `normalize_lines` repairs zero-length caption lines, but the 15 to 65 word stacks still make a caption jump. A spreading of each stack evenly over the span between the previous word's end and the next word's start is a general fallback that needs no model.

## 7. Raw against noise-reduced: the noise-reduced copy hides quiet speech

On the stretches the maintainer listened to, the raw track has a room level of -36 to -44 dBFS RMS; the noise-reduced copy (RNNoise, the recorder's) is at -49 to -86 dBFS there, 13 to 45 dB lower. In the clips where the maintainer hears speech inside a stretch that the detectors (run on the noise-reduced copy) called silent, the speech is in the raw track and is gone from the copy. For the Nov 2 and Data Platform meetings, every energy, Silero and speaker-engine number above comes from the *copy*; that is why a control with a voice in it looked "silent". On the PAT 5-minute reference the result is direct (audio-prep report): Turbo 23.4 percent WER-exact on the raw track and 36.4 percent on a noise-reduced copy made with the recorder's own denoiser.

## 8. What this does not show

One generator, FLEURS and text-to-speech speech, generated noise, one model family; real rooms are -36 to -44 dBFS, quieter noise gave nothing; the real-audio rate rests on 53 s of labelled silence and 1 to 5 instances; Large v3 Vietnamese not yet in the table; no prompt, no beam search, no other parameters (that is `mac-params`).

## 9. Decision and follow-up

1. Keep `silence_probe` and the set as the public insertion-per-hour metric: any later change (parameters, normalisation, gate, pieces) is judged on `make_silence_set.py` output at Turbo and Large v3, vi and en, with the rule of the plan (at most 2 per hour) as the target. Today: 171 and 109.
2. The shipped cure candidates to test on this set first: trimming silences longer than 8 s (audio-prep report), a veto from two independent signals only after more labelled data, and not reading from the noise-reduced copy.
3. Ask the maintainer for the second batch of clips (flatness and low-share units) and keep the keyed answers.
