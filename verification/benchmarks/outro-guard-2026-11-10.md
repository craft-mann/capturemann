# Text nobody said: the shape check against 13 stored transcripts (AI-1.AC12)

- **Date:** 2026-11-10. **Status:** PARTIAL, measured on the maintainer's library, read-only (the transcripts and `mic.wav` files were read, nothing was written there, no model was run, no sound was played). Not "verified": no dropped line was listened to.
- **Question:** Whisper writes sign-off and promotion sentences that nobody said. Is there a general shape (no phrases) that separates them from real speech in the stored data, with zero loss on the long real transcripts?
- **Data:** the 13 transcripts of the library with 400 words or more and an audio file (4,931 lines: 8 English files from 480 s to 7,638 s and 5 Vietnamese files from 1,406 s to 5,281 s; the two longest are `10-22-2026-Planning` (17,324 words) and the Vietnamese project meeting (18,327 words)). A line is a run of words that ends at a sentence mark or before a pause of 1 s. Audio levels are 20 ms frames of the recording's own `mic.wav` (the track the daemon reads by default), without the speech-model's voice-activity model (the Silero file is not installed on this computer and nothing was downloaded), so the check needs no optional model.
- **Code:** `cm_ai::guard::drop_unsupported`; the counts below come from `cargo run -p cm-ai --example outro_guard <transcript.json> <audio.s16le>`, which runs that function over a stored transcript and prints counts and time ranges only.

## What the invented lines look like

Measured per line: word confidence (mean, minimum), position in the file, level of the audio under the line against the quiet end of the whole recording (10th percentile of its 20 ms levels, "the room"), the share of the line's frames above the energy-rule speech threshold (`vad::speech_regions`), words per second, word durations, pauses before and after.

- The sign-off lines of the maintainer's examples that are in this library (the "Hãy subscribe cho kênh Ghiền Mì Gõ ..." family and the closing "Cảm ơn các bạn đã theo dõi và hẹn gặp lại.") have **confidence 0.98 to 1.00 on every word** in the middle of the file, and sit at **6 to 9 dB over the room** while the same file's speech is 18 dB over it. The closing sentence at the end of the Nov 9 weekly meeting is different: its ten words are stacked in 40 ms at the very end of the audio (word times collapsed), confidence 0.45 to 1.0 (this older transcript predates the word-time repair).
- In the 2,549 s imported demo file the stored transcript has 24 lines written over **digital silence** (the audio there is -111 to -120 dBFS): a loop of a sign-off sentence 20 times in a row with the channel name varying ("La La School" in some, "La La La School" in others), plus sentences cut from real speech repeated over silence.
- Not separated by these numbers: the same family of sentence at 581 s of the Nov 9 meeting sits at a speech level (-32 dB, 98 % of its frames above the speech threshold), a longer line mixing real words with the invented ones (901 s), and the collapsed closing sentence. These stay in the transcript.

## What was tried and rejected (counts of lines of 4,931)

| Rule | Lines it hits | In the 17,324-word English file |
|---|---|---|
| Share of the line's frames above the speech threshold under 0.15, 3+ words, 1 s+ | 100 | 2 |
| Same, and mean confidence 0.9+ | 32 | 0 |
| Word times collapsed (10+ words per second, 6+ words) | 59 | 17 (old stacked word times on real speech) |
| Under 0.3 speech share and every word 0.9+ | 14 | 0 |

The speech-share rule drops real quiet speech of other participants (the meetings are recorded on one microphone): of its 9 lines outside the demo file, 6 read as ordinary quiet sentences ("Thank you so much.", Vietnamese sentences at -35 dB). A collapse rule hits real speech in old transcripts. Neither is safe alone.

## The rule that is shipped

A line (3 words or more, at least 1 s) is dropped when the 80th percentile of its 20 ms levels is
- within **2 dB of the room** and it has **5 words or more** (text over a stretch with no sound), or
- within **9 dB of the room**, it has 3 words or more and **every word has confidence 0.97 or more** (the decoder is certain of text over a stretch with barely any sound).

Never judged: audio under 30 s, or where the 90th and 10th percentiles are less than 10 dB apart. Thresholds were set from this data; the confident rule was set at 0.97 because the next candidates at 0.94 and 0.95 were in running speech and not clearly invented (2 lines). That is tuned on a small sample.

## Result of the shipped rule on the same 13 files

| File (language, length) | Lines | Dropped lines |
|---|---|---|
| English planning meeting (7,638 s, 17,324 words) | 1,713 | **0** |
| Other English files (7 files, 480 to 1,038 s) | 1,396 | 1 |
| Vietnamese project meeting (5,281 s, 18,327 words) | 730 | **0** |
| Vietnamese weekly meeting Nov 2 (2,678 s) | 503 | **0** |
| Vietnamese meeting with PAT (1,406 s) | 148 | **0** |
| Vietnamese weekly meeting Nov 9 (1,524 s) | 189 | 3 (20 words) |
| Vietnamese imported demo (2,549 s) | 252 | 24 (412 words) |
| Total | 4,931 | 28 |

- All 24 of the demo file's drops are over digital silence (levels at -111 to -120 dBFS). The 3 drops of the Nov 9 meeting are the first two fragments of the sign-off sentence (one pause splits it) and the long fragment starting at 542 s: all read as the invented sentence by their text. The one drop among the English files is a six-word closing sentence 9 s before the end of a call, with confidence 0.99 on every word, at 2.7 dB over the room, between two stretches of speech: it reads like an invented sign-off but I cannot tell by eye that nobody said it; **it is the one line that may be a false positive**.
- Zero lines were dropped from the English planning meeting and the 3 long Vietnamese meetings (target met on these files, 3,094 lines).
- Not caught: the sign-offs at speech level, the mixed line and the collapsed closing sentence (see above). The guard catches text over silence and some sign-offs over room noise, not every invented line.

## Default

`ai.drop_unsupported_lines` is **on by default** (the measurement shows no drop of a line that is certainly speech; one line is unconfirmed). Every dropped line is kept in `transcript.json` as `dropped` (start, end, word count, reason) and counted as `lines_dropped` in the job result, and one switch on the AI page turns the check off; a re-generate with it off brings the lines back.

## Not done

- No line was listened to (no sound). The judgement above is by level, confidence and, for the 3 sign-off fragments, the text.
- The Silero voice-activity model was not used (not installed here). With it as a second opinion, the lines at speech level might be separable; not measured.
- The "Always ignore this line" action on a finished transcript row is not built (the AI page list takes the phrase by hand).
- Mixed system-audio recordings: the check reads the track the transcription read; measured only on microphone tracks and an imported file.

## Update: listening result and the language limit (Nov 10, 2026, later the same day)

- **The one unconfirmed drop was real speech.** The maintainer listened to the closing 14 seconds of the English call: the six-word line is something a participant said, softly ("I'll talk to you real soon", with "See ya. Thank you." after it, everything said by one participant except the last "thank you", which was the maintainer's). So the shape check has **one confirmed false positive** in 4,931 lines, and the first drop that was listened to.
- **Level alone cannot separate it from the invented sentences.** Over-the-room margin (80th percentile of the line against the 10th percentile of the whole recording): the real line 2.7 dB; the invented sentence of the Nov 9 Vietnamese meeting 5.6 dB (the maintainer verified in the app that it is gone); the invented lines over digital silence of the demo file about 0 dB and one of the Nov 10 meeting at -55 dB. A threshold low enough to keep the real line also keeps the Nov 9 sentence. The level spread inside the line (90th minus 10th percentile) goes the other way (real 3.6 dB, invented 6.5 dB), so it is no help either (two lines, not a study).
- **The 4 h 13 min Vietnamese file (not listened to):** the shape check drops 12 lines (115 words) with margins of 4.6 to 7.7 dB for the five lines that were close to the rule, and several 10 to 15-word lines inside 1.5 s (stacked word times). It is not known whether they are invented or spoken; they need listening before the guard is trusted on long Vietnamese meetings.
- **Decision (maintainer, Nov 10, 2026): the shape check runs on Vietnamese transcripts only for now** (`cm_ai::guard::LANGUAGES = ["vi"]`; the setting still turns it off). English and every other language keep all lines. This is a limit of what was measured (the invented sign-offs seen were Vietnamese; the only English drop was wrong), not a finding that English does not have them.
- **Still to try:** a speech detector as a second opinion on the lines the shape check would drop (the segmentation model of the speaker engine is already on the computer; Silero was not tried). Not built.

## Update: three drops of the 4 h Vietnamese file listened to (Nov 10, 2026, maintainer's report)

The maintainer listened to three of the 12 drops of the 4 h 13 min file: at 15:31, 1:07:35 and 1:09:15. **All three were correct drops** (background noise, nobody speaking). The other nine were not listened to. The maintainer approved keeping the guard on (Vietnamese only, setting `ai.drop_unsupported_lines` default on) for 0.3.1. Three of twelve is a spot check, not proof: the status stays PARTIAL.
