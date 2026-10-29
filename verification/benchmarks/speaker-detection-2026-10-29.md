# Speaker detection benchmark (English)

- **Date:** 2026-10-28 and 2026-10-29
- **Who:** maintainer with Claude Code
- **Machine:** Apple M4 Pro, macOS 26.6.2
- **Build:** git `9d36847` plus the examples in `crates/cm-ai/examples` (release build of the `cm-ai` examples, sherpa-onnx 2.0.1, pyannote segmentation 3.0)
- **Question:** the app finds speakers with a CAM++ English VoxCeleb voice model and a clustering threshold of 1.2, tuned on one recording. How accurate is that, and does a different voice model or threshold do better?

## Short answer

- On 14 English conversation clips (3 to 5 minutes each) the app's current setup scores a diarization
  error rate (DER) of **48 to 54 %**, no better than labelling everybody as one speaker (**51 %**).
  It found the right number of speakers in 0 of 6 held-out clips.
- The multilingual 3D-Speaker CAM++ model (`3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced`,
  Apache-2.0 on its model card) with a clustering threshold of 1.0 scores **22.1 %** on the 8
  clips used to choose it and **17.1 %** on 6 clips held back until the choice was made, and found
  the right count in 5 of 6 held-out clips.
- TitaNet-small (NeMo) and ERes2Net (English) are close on the tuning clips. WeSpeaker models were
  worse than the current one on some clips and were dropped after the first round.
- This is a benchmark, not a change to the app. Nothing here covers Vietnamese or any other language.

## Setup and how to repeat it

```text
make sherpa                                   # speaker segmentation + current embedding model
python3 -I tests/tools/make_reference_clips.py primock --out <dir> --count 6
python3 -m venv v && v/bin/pip install pyarrow
v/bin/python tests/tools/make_reference_clips.py ami --out <dir> --count 16 --balanced
cargo build --release -p cm-ai --example baseline
target/release/examples/baseline <clips dir> --sherpa packaging/sherpa/out/models \
  [--embedding <voice model .onnx>] --thresholds 0.6,0.8,1.0,1.2,1.4 [--whisper <ggml> --language en]
target/release/examples/measure der <reference.rttm.txt> <result> [--collar 0.25] [--range a:b]
```

`baseline` runs the same path as the app (`diarize_audio`, including tidy-up) and scores against the
reference turn lists with a 0.25 s collar. Each clip is run once per threshold, so there is no
spread to report. The reference speaker labels are mapped to the found ones one to one by shared
speech time.

## Data

| Source | Clips | Reference | Licence (as published) |
| --- | --- | --- | --- |
| PriMock57 (github.com/babylonhealth/primock57), doctor and patient channels mixed into one | 6 (2 speakers) | hand-timed utterances per speaker | CC BY 4.0 per the Hugging Face mirror (`sdialog/Primock-57`); the GitHub repository has no LICENSE file |
| AMI headset mix (`diarizers-community/ami_ihm_with_transcripts`, test split) | 8 (3 to 4 speakers) | AMI timestamps and speakers | AMI is published under CC BY 4.0; the dataset card states no licence. Not checked at the source |

Clips start at a turn start and end at a turn end. AMI windows were chosen so that every speaker
has a real share of the speech (`--balanced`; an earlier set with two clips where one person spoke
almost all the time was dropped because it made DER meaningless).

- **Tuning set (8 clips):** `primock_day1_consultation01`, `day1_consultation02`, `day2_consultation01`, `day3_consultation01`, `ami_EN2002b`, `EN2002d`, `TS3003a`, `TS3003c`.
- **Held-out set (6 clips), run once after choosing the model and threshold:** `primock_day4_consultation01`, `day5_consultation01`, `ami_ES2004b`, `ES2004d`, `IS1009a`, `IS1009c` (other meetings and other speakers than the tuning set).

## Results

### Tuning set: mean DER by model and threshold (lower is better)

The threshold is the clustering distance used when the speaker count is not given. "Right count" is
how many of the 8 clips got the true number of speakers.

| Voice model | 0.6 | 0.8 | 1.0 | 1.2 | 1.4 | True count given |
| --- | --- | --- | --- | --- | --- | --- |
| Current: CAM++ English VoxCeleb (30 MB) | 58.4 % (1/8) | 60.2 % (2/8) | 47.9 % (4/8) | 51.7 % (1/8) | 55.0 % (0/8) | 49.8 % |
| **3D-Speaker CAM++ zh-en advanced (28 MB)** | 33.5 % (2/8) | 27.0 % (6/8) | **22.1 % (5/8)** | 49.2 % (0/8) | 55.0 % (0/8) | 27.0 % |
| NeMo TitaNet-small (40 MB) | 41.7 % (2/8) | 29.2 % (3/8) | 22.4 % (6/8) | 43.5 % (2/8) | 55.0 % (0/8) | 26.9 % |
| 3D-Speaker ERes2Net English (27 MB) | 38.2 % (1/8) | 30.5 % (3/8) | 25.1 % (4/8) | 41.4 % (3/8) | 55.0 % (0/8) | 28.2 % |
| Everyone as one speaker (no engine) | | | | | | 51.1 % |

All three better models stop working between 1.0 and 1.2: the clusters merge into one or two speakers.
The best value sits just below that edge, so a fixed constant is fragile.

First round (7 clips, includes two clips dominated by one speaker, so less reliable; true count
given, mean DER): current 42.6 %, 3D-Speaker CAM++ zh-en 18.0 %, ERes2Net 17.7 %, TitaNet-small
17.4 %, WeSpeaker ResNet34 32.7 %, WeSpeaker CAM++ 47.7 %.

### Held-out set (run once)

| Clip | Speakers | Current model, threshold 1.2 | CAM++ zh-en, threshold 1.0 | Everyone as one |
| --- | --- | --- | --- | --- |
| ami_ES2004b | 4 | 59.4 % (found 1) | 23.1 % (found 3) | 58.2 % |
| ami_ES2004d | 4 | 67.9 % (found 2) | 17.4 % (found 4) | 67.0 % |
| ami_IS1009a | 4 | 43.5 % (found 2) | 12.7 % (found 4) | 42.3 % |
| ami_IS1009c | 4 | 51.9 % (found 2) | 19.6 % (found 4) | 44.5 % |
| primock_day4_consultation01 | 2 | 51.1 % (found 1) | 9.4 % (found 2) | 48.1 % |
| primock_day5_consultation01 | 2 | 47.0 % (found 1) | 20.1 % (found 2) | 44.5 % |
| **Mean** | | **53.5 %** (right count 0/6) | **17.1 %** (right count 5/6) | **50.7 %** |

### Transcript word error rate (earlier 7-clip set, Whisper large-v3-turbo, English)

Mean WER 23.8 %, or 20.7 % with hesitation sounds (um, uh, mm, hmm) removed from both texts. Most
of the errors are deleted words. About half of the reference words that were missed in one PriMock
clip were "um", "uh" and "yeah": the references write every hesitation and backchannel down,
Whisper leaves most of them out. This figure measures that style difference as much as recognition.
It is a baseline for comparing options, not an accuracy claim.

## What this does not show

- English only, conversations of 2 to 4 people. Nothing about Vietnamese or other languages (a
  multilingual model may help; it is not measured here).
- 14 clips, one run each, no confidence intervals. The held-out set is small (6 clips, 2 meetings
  series of AMI plus 2 consultations).
- PriMock57 mixes two separate clean channels, easier than one microphone in a room. AMI is the
  headset mix, not far-field audio. Both references are public datasets with their own
  transcription conventions.
- The threshold and the model were chosen on the tuning set. The held-out figure is the fair one,
  and it was run once.
- The best 1.0 threshold is close to the cliff at 1.2; a different recording length or number of
  speakers may move it. The count rule is not solved: 1 of 6 held-out clips and 3 of 8 tuning
  clips still got the wrong count.
- The heavily overlapped AMI clip `EN2002b` stays near 40 % for every model.
- Model licences: confirmed on the ModelScope cards for the two 3D-Speaker models (Apache-2.0);
  not confirmed for the TitaNet-small weights (the NeMo catalogue page says "covered by the license
  of the NeMo Toolkit"). The cards do not say what data the models were trained on.

## Voice models used

| Model | SHA-256 |
| --- | --- |
| `3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx` (current) | `357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b` |
| `3dspeaker_speech_campplus_sv_zh_en_16k-common_advanced.onnx` | `aa3cfc16963a10586a9393f5035d6d6b57e98d358b347f80c2a30bf4f00ceba2` |
| `3dspeaker_speech_eres2net_sv_en_voxceleb_16k.onnx` | `c59158379255ad66e161679cca6af8d52d51e389e3224ab7d7a7baae295c2db5` |
| `nemo_en_titanet_small.onnx` | `ad4a1802485d8b34c722d2a9d04249662f2ece5d28a7a039063ca22f515a789e` |

Files are from the sherpa-onnx `speaker-recongition-models` release.

## Decision and follow-up

- Candidate for 0.2.3: the multilingual CAM++ at a clustering threshold near 1.0, replacing the
  English-only model. Before it ships: bundle it with a pinned SHA-256 and add its licence to
  `THIRD_PARTY_LICENSES`, replace the single threshold by a rule that picks the count from the gap
  between merge distances, and measure Vietnamese recordings (AI-8.AC1 to AC3 stay open until then).
- Not done: tidy-up constants (`FLICKER_SECS`, `MERGE_GAP_SECS`), the folding rule, snapping turn
  boundaries to pauses, speaker windows with progress.
