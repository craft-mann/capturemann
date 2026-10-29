# AI-8 manual script: speaker labels (who spoke when)

The two speaker models (pyannote segmentation-3.0 and CAM++ Chinese-English) **ship inside the app**
(`Contents/Resources/Models/speaker/`, made by `make sherpa` and `make bundle`), so there is no
setup step. Run this script with the bundle (`make run-bundle-dev`), not a bare `cargo run`, which
has no models unless `CM_SPEAKER_MODELS_DIR` points at `packaging/sherpa/out/models`. Steps marked
**[S7]** need real recordings and a person's ear; accuracy is not measured yet. The logic
(numbering, word assignment, overlap, headset mode, rename, merge, assign, undo, the exports, the
voiceprint check) is tested with a scripted diarizer: `cargo test -p cm-ai speakers` and `cargo
test -p cm-daemon --test ai_speakers`. Spec: `specs/features/07-ai.md`, AI-8 (AC1 to AC7). Boards:
`design/redesign/screens/png/` V4 (editor) and V8 (AI page).

**Privacy:** the fixtures record people's voices. Record only people who agreed, on your own
instance (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` on scratch
folders), say nothing private, and delete the recordings afterwards. Voiceprints must never reach
the disk: step 9 checks it. Turn the network off from step 2 on.

## Setup **[S7]**

1. Check the app has them: `ls "$(dirname "$(which cm-desktop)")/../Resources/Models/speaker"` (or the
   bundle path) shows the two `.onnx` files and `LICENSES/`; `capturemann ai models` lists
   `speaker-segmentation` and `speaker-embedding` as installed (bundled). Nothing to download.
2. Fixtures (hand-write the turns as who, start, end):
   - **A**: a 10-minute two-person interview on the Mac's built-in microphone, quiet room.
   - **B**: a 30-minute four-person meeting around the laptop, one microphone.
   - **C**: a short call: you on a headset plus a recording of two other voices playing through
     the speakers (Online meeting preset, mic + system audio).
   Transcribe each: `capturemann ai transcribe ID --wait`.

## 1. Without the models (a developer build made without `make sherpa`; runs now)

- `capturemann ai speakers ID --wait` exits 2 with `no_speaker_model`. The AI page, Speaker labels:
  one row "Speaker model: Not available in this build", no Download or Import button; the switch
  "Find speakers after each transcript" is off and does not move; "Your name in transcripts" keeps what you type
  and `capturemann settings get profile.display_name` agrees; an empty box returns to Me.
- The editor's Speakers tab and lane are empty with a hint; nothing errors.

## 2. Two people, accuracy (AC1) **[S7]**

1. Fixture A: `capturemann ai speakers ID --wait`.
2. `capturemann ai speakers list ID`: two speakers; "Speaker 1" spoke first.
3. Compare `capturemann ai transcript ID --words` (the `speaker` of each word) with your turns.
- Pass when **at least 90% of words** carry the right speaker. Write the count down.

## 3. Four people, speed and error rate (AC2) **[S7]**

1. Fixture B on a Mac with 16 GB: `time capturemann ai speakers ID --wait`.
- Pass when it takes **3 minutes or less** and the diarization error rate against your turns is
  **20% or less** (no collar, overlap scored; use the S7 script if there is one, else compare by
  hand over five sampled minutes). Otherwise report the numbers; S7 thresholds are revisable.

## 4. A hint (AC3) **[S7]**

- `capturemann ai speakers ID --num 2 --wait` on fixture B gives exactly two speakers; `--num 4`
  exactly four. The Record sheet's "How many people?" is sent as `start_recording.people` and stored
  in the manifest `settings`: record a short audio take with 3 selected, then check
  `manifest.json` has `"people": 3` and that the Speakers tab starts on 3.

## 5. Rename, merge, reassign, undo (AC4, AC5) **[S7]**

1. Editor, Speakers tab: rename Speaker 2 to "Linh". Expected: the lane, the "now speaking" card,
   the Transcript tab, a TXT export and the next report (AI-9) all say "Linh". Undo: all say
   "Speaker 2" again.
2. Merge Speaker 3 into Speaker 1: no word is Speaker 3 any more; undo restores both.
3. Select a stretch of the lane and give it to another speaker: the words in it follow; undo.
- Pass when every place agrees after each step and each is one undo step.

## 6. Exports with names

- TXT and Markdown write "Name: text" per turn with times; VTT uses `<v Linh>`; SRT has names only
  with `capturemann export ID --captions srt --speaker-names` (the footer's Speaker names switch),
  then `LINH:`. With no speakers the switch is dimmed.

## 7. Headset mode (AC7) **[S7]**

1. Set your name (`profile.display_name`, default Me). Fixture C: `capturemann ai speakers ID
   --headset --wait`.
- Pass when every word from the mic track carries your name and only the system track was split
  into speakers (the two voices).

## 8. Offline (AC6)

- With the network off, step 2 works end to end, and a network monitor shows no connection from
  the app during the job.

## 9. No voiceprints on disk (AC6)

- After the job: `find <bundle> <config folder> -iname '*voiceprint*' -o -iname '*embedding*'`
  finds nothing, and `diarization.json` holds only segments (start, end, speaker), the model name
  and the date.

## 10. After stop, automatically **[S7]**

- "Find speakers after each transcript" (`ai.auto_speakers`) is ON by default: with automatic
  transcription on, record 1 minute with two voices (preset In the room, "How many people?" 2):
  after stop the transcript and then the speakers appear without a click, the Speakers tab shows the
  run's progress, and the job list shows one `diarize` job started by the user.
- Rename a speaker, then press Generate transcript again: the names stay and no second run starts.
- Record a Voice note: no speaker run starts. Turn the switch off, record and transcribe again
  (In the room): no run. With the speaker models removed from the app folder and the models folder:
  nothing starts and nothing is reported.

## Report

Store the run in `verification/` (template in its README) with the model name and licence, the
numbers from steps 2, 3 and 7, and the timings. Until a report exists AI-8 is "tested with fakes";
without S7 there is no model for users.
