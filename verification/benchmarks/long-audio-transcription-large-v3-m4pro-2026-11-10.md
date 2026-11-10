# Long audio transcription: a 2 h 07 min MP3 with large-v3 on an M4 Pro (0.3.1)

- **Date:** 2026-11-10. **Status: PARTIAL.** One file, one computer, one run, started and watched by the maintainer; the numbers below are read from the job record and the files, not estimated.
- **Purpose:** measure how the app handles a long import (the 0.3.1 piece-by-piece transcription with checkpoints and partial results) so that later releases can compare reliability and speed. Whether the crash found on Nov 9 (whisper.cpp abort in the DTW word-time step) is fixed: it did not recur in this run.
- **Privacy:** the file is a private recording of a meeting. Its name, title, speakers and text are not written here; only counts, sizes, times and positions.
- **Build:** the dev bundle made from commit `c6091648` of `main` (0.3.1; vendored whisper.cpp 1.8.3 with the short-window guard), `CaptureMann-dev.app`, signed "CaptureMann Local Dev".

## The file and the machine

| | |
|---|---|
| Source | one MP3, imported with "Import" (app), kept as `original.mp3` beside the decoded master |
| MP3 size | 122,201,652 bytes (122.2 MB), about 128 kbit/s |
| Duration | 7,637.6 s = **2 h 07 min 17.6 s** (manifest: 916,512,390 ticks) |
| Master in the bundle | `mic.wav`, 2,932,839,748 bytes (2.93 GB; 48 kHz, 32-bit float, decoded once at import) |
| Spoken language | English (detected: the job was started with automatic language) |
| Computer | Apple M4 Pro (Mac16,8), 48 GB memory, macOS 26.6.2, Metal |
| Other work during the run | the app was open with the maintainer; no recording, no export; the speaker job of an earlier run on the same capture had finished the day before |

## The run

| | |
|---|---|
| Job | `transcribe`, model `large` (ggml large-v3, 3,095,033,483 bytes), language automatic, "Transcribe from": the original audio (`original_audio: true`), attempts 1 |
| Started | 2026-11-10 04:23:25 UTC (11:23 in Vietnam) |
| Finished | 04:48:23.6 UTC, state `done`, no error |
| **Time to transcribe** | **1,498.5 s = 24 min 58 s** |
| **Speed** | **5.10 times faster than real time** (7,637.6 s of audio in 1,498.5 s); 0.196 s of work per second of audio |
| Result | language `en`, **17,324 words**, `transcript.json` 2.0 MB; first word at 2.8 s, last word ends at 7,636.8 s (the file ends at 7,637.6 s) |
| Words per minute of audio | 136 on average; per 5 minutes between 531 and 923 (the last, partial, block has 241); no block empty |
| Words below 0.6 confidence | 882 of 17,324 (5.1 %); mean confidence 0.934 |
| Longest silence between two words | 23 s (at 2 h 05 min), then 14 s, 10 s, 9 s, 9 s |

## Progress and memory (sampled once a minute, from 14 min into the run)

The sampler started 14 min after the job did, so the first 58 % has no samples; progress 58 % at 14 min 14 s means about 4.1 % of the file per minute, and the later samples give 3 to 5 % per minute.

| UTC | progress | memory of `cm-desktop` | CPU (one core = 100 %) |
|---|---|---|---|
| 04:37:39 | 58.2 % | 3,952 MB | 114 |
| 04:38:34 | 60.6 % | 3,879 MB | 106 |
| 04:39:34 | 66.1 % | 3,939 MB | 135 |
| 04:40:34 | 71.5 % | 4,021 MB | 117 |
| 04:41:34 | 74.9 % | 4,068 MB | 115 |
| 04:42:34 | 79.2 % | 4,069 MB | 122 |
| 04:43:34 | 83.6 % | 4,085 MB | 118 |
| 04:44:34 | 90.0 % | 3,770 MB | **13** |
| 04:45:35 | 92.2 % | 3,809 MB | 115 |
| 04:46:35 | 94.2 % | 3,820 MB | 105 |
| 04:47:35 | 99.4 % | 3,929 MB | 115 |
| 04:48:35 | done | **496 MB** | 91 |

- **Memory:** between 3.77 and 4.09 GB while running (the model is 3.1 GB, the 2 h of 16 kHz samples about 0.5 GB), **peak 4.09 GB, not climbing without end** (it rose 0.2 GB over the 5 minutes from 60 % to 84 % and fell back at 90 %), and 0.5 GB the moment the job ended. This is not the "flat memory whatever the length" of `specs/long-audio.md`: the whole file is still decoded into memory, so a 4-hour file would need about 1 GB for the samples on top of the model.
- **One stall:** at 90.0 % two samples 6 s apart show the app at 13 % CPU; the next sample (a minute later) is at 92.2 %. So there was a pause of at least 6 s and at most about 60 s with little work (the sampler cannot say what the app did: reading a finished piece again, writing a checkpoint or the first phase of a piece check). Worth a log line next time.
- The progress message stayed "Transcribing N %" the whole time (no "Checking the transcript" was seen at the sample times).

## Checks of the result (counts only; computed from `transcript.json`)

- **No repetition loop of the kind found on Oct 28** (one phrase for minutes): the most common 6-word phrase occurs 15 times in 17,324 words.
- **Two short repeated stretches, to listen to once:** (1) at 99 min, a five-word list of numbers appears back to back about 6 times, with all the word times inside 0.2 s (confidence 0.94: likely read-out numbers or a time-bunching of word times, not a loop; the words are in the transcript, the times of that stretch are not usable); (2) at 19.5 min, a 4-word phrase appears 3 times in 0.9 s (mean confidence 0.64; likely a stutter or a doubtful stretch).
- **No coverage hole:** speech is spread over the whole 2 h 07 min (see words per 5 minutes above).
- **Not stored in the transcript file:** the piece count, which pieces were split or repaired, and how many pieces the loop check flagged (`PiecesReport` is not written to `transcript.json`). So this run cannot say how many 5-minute pieces there were or whether any was cut again. See "What would improve the next report".

## What this shows

1. **The crash of Nov 9 did not recur** on a 2 h 07 min file with large-v3 (the longest run so far: the first attempt on the 73-minute file died in `median_filter`; the vendored guard is in this build). One file, so "fixed" is stated as "did not recur in this run".
2. **Speed is not the problem on this Mac:** 5.1 times real time with large-v3, about 25 minutes for 2 h 07 min. A 4-hour file would take about 47 minutes at that rate (an estimate, not a measurement).
3. **Memory is high but bounded:** 4.1 GB peak with large-v3 on a 2-hour file; the audio part grows with the file length (about 0.25 GB per hour), the model part does not.
4. **The user waited 25 minutes with the 0.3.1 build showing the first minutes as they were made** (partial results); the maintainer's screenshot at 22 % shows 24 minutes of text ready, in rows that start a few seconds apart. What was shown on screen at other moments is not measured here.

## What would improve the next report (reliability follow-ups, none done yet)

- Store the pieces report (piece count, splits, repairs, loops collapsed, gap fills) next to the transcript, or in the job result, so a benchmark can read it.
- Sample from the first second (start the sampler with the job), and log the stage names ("Reading", "Checking") with the progress.
- Log what the app does during a pause like the one at 90 %.
- Fix or flag word times that are bunched inside a fraction of a second (the stretch at 99 min); the partial and final transcript should not claim exact times there.
- Memory: read the audio in windows from the decoded file instead of keeping all samples (`specs/long-audio.md` point 1) before 4-hour files are promised; keep this report as the baseline (4.09 GB at 2 h 07 min with large-v3).
- Repeat with `large-v3-turbo` (1.6 GB, said to be several times faster) and on a second language (Vietnamese with PhoWhisper small) so the table has more than one row; same file, same machine.
- A 4-hour file, and one run that is interrupted (app quit, then reopened) to measure the resume from the last finished piece.
