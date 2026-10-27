# AI-8 manual script: speaker labels (who spoke when)

**BLOCKED on spike S7.** No speaker model is chosen or shipped: production uses `NoSpeakerModel`,
so `capturemann ai speakers ID` fails with `no_speaker_model` today and the AI page says "Speaker
model not available yet". This script is written for the day S7 picks a model and a runtime and
the download is wired. Steps marked **[S7]** need the model; the others can run now. The logic
(numbering, word assignment, overlap, headset mode, rename, merge, assign, undo, the exports, the
voiceprint check) is tested with a scripted diarizer: `cargo test -p cm-ai speakers` and `cargo
test -p cm-daemon --test ai_speakers`. Spec: `specs/features/07-ai.md`, AI-8 (AC1 to AC7). Boards:
`design/redesign/screens/png/` V4 (editor) and V8 (AI page).

**Privacy:** the fixtures record people's voices. Record only people who agreed, on your own
instance (`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET` on scratch
folders), say nothing private, and delete the recordings afterwards. Voiceprints must never reach
the disk: step 9 checks it. Turn the network off from step 2 on.

## Setup **[S7]**

1. Install the speaker model S7 chose from the AI page (press Download; licence and size are shown
   first; a CC-BY model shows its attribution) or import the file. Write the model name down.
2. Fixtures (hand-write the turns as who, start, end):
   - **A**: a 10-minute two-person interview on the Mac's built-in microphone, quiet room.
   - **B**: a 30-minute four-person meeting around the laptop, one microphone.
   - **C**: a short call: you on a headset plus a recording of two other voices playing through
     the speakers (Online meeting preset, mic + system audio).
   Transcribe each: `capturemann ai transcribe ID --wait`.

## 1. Without a model (runs now)

- `capturemann ai speakers ID --wait` exits 2 with `no_speaker_model` and says where to get a
  model. The AI page, Speaker labels: "Speaker model not available yet"; the switch "Find speakers
  after each transcript" is off and does not move; "Your name in transcripts" keeps what you type
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
  exactly four. The Record sheet's "How many people?" is shown but not sent yet (known gap): note
  whether that is still so.

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

- Turn on "Find speakers after each transcript" (`ai.auto_speakers`, default off) with automatic
  transcription on, record 1 minute with two voices: after stop the transcript and then the
  speakers appear without a click. Turn the switch off again.

## Report

Store the run in `verification/` (template in its README) with the model name and licence, the
numbers from steps 2, 3 and 7, and the timings. Until a report exists AI-8 is "tested with fakes";
without S7 there is no model for users.
