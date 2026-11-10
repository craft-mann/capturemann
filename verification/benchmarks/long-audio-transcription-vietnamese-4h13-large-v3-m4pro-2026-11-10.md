# Long audio transcription: a 4 h 14 min Vietnamese MP3 with large-v3 on an M4 Pro, with speakers at the same time (0.3.1)

- **Date:** 2026-11-10. **Status: PARTIAL.** One file, one computer, one run, started and watched by the maintainer; the numbers are read from the job records, the files and a once-every-30-seconds sampler (168 samples), not estimated.
- **Purpose:** the second long-file run after `long-audio-transcription-large-v3-m4pro-2026-11-10.md` (2 h 07 min, English): a file twice as long, in Vietnamese, with the speaker job started together with the transcription, to see stability, speed and memory. It is also the first run with partial results on screen (maintainer's report, not measured here).
- **Privacy:** the file is a private recording of a meeting. Its name, title, speakers and text are not written here; only counts, sizes, times and positions.
- **Build:** the dev bundle made from commit `c6091648` of `main` (0.3.1: piece-by-piece transcription with checkpoints and partial results; vendored whisper.cpp 1.8.3 with the short-window guard), `CaptureMann-dev.app`. It predates the outro guard (AI-1.AC12 to AC14), so nothing was dropped by it.

## The file and the machine

| | |
|---|---|
| Source | one MP3 imported with "Import" (app); kept as `original.mp3` beside the decoded master |
| MP3 size | 243,804,412 bytes (243.8 MB), about 128 kbit/s |
| Duration | 15,237.7 s = **4 h 13 min 57.7 s** |
| Master in the bundle | `mic.wav`, 2,925,642,332 bytes (2.93 GB): 48 kHz, 1 channel, 32-bit float, decoded once at import (15,237.7 s x 48,000 x 4 bytes); the transcription then reads it as 16 kHz mono (about 0.98 GB of samples in memory) |
| Spoken language | Vietnamese (detected: the job was started with automatic language) |
| Computer | Apple M4 Pro (Mac16,8), 14 cores, 48 GB memory, macOS 26.6.2, Metal |
| Other work during the run | the app open with the maintainer; no recording, no export; the transcription and the speaker job started at the same second |

## The two jobs

| | transcribe | diarize (speakers) |
|---|---|---|
| Model | `large` (ggml large-v3, 3,095,033,483 bytes), language automatic, original audio (`original_audio: true`) | sherpa-onnx 2.0.1, pyannote segmentation 3.0 + CAM++, 3 speakers asked |
| Started | 2026-11-10 07:29:23.79 UTC (14:29 in Vietnam) | 07:29:23.79 UTC |
| Finished | 08:54:44.8 UTC, state `done`, no error | 07:58:33.0 UTC, state `done`, no error |
| **Time** | **5,121.0 s = 85 min 21 s** | **1,749.2 s = 29 min 9 s** |
| **Speed** | **2.98 times real time** (15,237.7 s of audio in 5,121 s) | 8.7 times real time |
| Attempts | 1 | 1 |
| Result | language `vi`, **41,060 words**, `transcript.json` 4.8 MB | 3 speakers, `diarization.json` 267 KB |

For comparison, the first run (2 h 07 min English, same model, same computer, no speaker job at the same time): 5.10 times real time. This run was **1.7 times slower per second of audio**. Possible reasons, none separated by this single run: the speaker job took the processor for the first 29 minutes (transcription reached 28 % in that time, about 1 % of the file per minute; after it, 30 % to 96 % took 50 minutes, 1.3 % per minute, still slower than the English file's 4 % per minute); Vietnamese needs more decoder tokens per second of speech; more pieces went through the second reading ("Checking the transcript" appeared in the progress at 42 % and at 72 %, never in the English run).

## Progress and memory (sampled every 30 seconds from 1 minute into the run)

| UTC | transcribe | speakers | memory of `cm-desktop` | CPU (one core = 100 %) |
|---|---|---|---|---|
| 07:31 | 2 % | 12 % | **11,683 MB (peak 11,714 MB)** | 813 |
| 07:36 | 7 % | 37 % | 10,667 MB | 749 |
| 07:41 | 14 % | 52 % | 10,976 MB | 770 |
| 07:46 | 21 % | 64 % | 4,726 MB | 696 |
| 07:51 | 25 % | 76 % | 4,178 MB | 168 |
| 07:56 | 27 % | 84 % | 4,200 MB | 325 |
| 08:01 | 31 % | done | 3,910 MB | 13 |
| 08:16 | 51 % | done | 3,963 MB | 10 |
| 08:31 | 68 % | done | 4,102 MB | 63 |
| 08:46 | 85 % | done | 3,967 MB | 11 |
| 08:51 | 96 % | done | 4,279 MB | 21 |
| 08:55 | done | done | 2,747 MB (app open, idle) | 803 at that sample |

- **Memory:** while both jobs ran, **11.7 GB at the start, about 11 GB for 14 minutes**, then it fell to about 4.2 GB at 07:46 (the speaker job had finished reading the audio; it ended at 07:58). With the transcription alone it stayed between 3.87 and 4.28 GB (the model is 3.1 GB, the 4 h of decoded samples about 1 GB), **peak for the transcription alone 4.28 GB**, and did not climb without end. The speaker job accounts for roughly 7 GB over the transcription alone, as it reads the whole file: that is the heaviest figure of the two runs and the first thing to bring down (`specs/long-audio.md` point 7: read in windows).
- **CPU:** 700 to 810 % while the speaker job ran; 10 to 75 % afterwards, which means the work of the transcription is mostly on the GPU and the processor waits (the English run showed about 115 %).
- No crash, no hold, no restart; the progress went up in one direction the whole time.

## Checks of the result (counts only; from `transcript.json`)

- **Coverage:** speech from 12.1 s to 15,234.9 s of the 15,237.7 s; no empty 10-minute block (between 739 and 2,083 words per 10 minutes; the last, partial, block has 381); the longest silence between two words is 30 s (at 44.7 min), then 27 s, 25 s, 24 s, 24 s.
- **Confidence:** mean 0.764; **10,406 of 41,060 words (25.3 %) are below 0.6**. This is about five times the English file's 5.1 %: Vietnamese with large-v3 is much less sure of itself, which matches `verification/benchmarks/vietnamese-models-2026-11-03.md` (large-v3 is not the best Vietnamese model; PhoWhisper is).
- **Repetition:** the most common 6-word phrase occurs 44 times in 41,060 words (spoken filler in Vietnamese is likely, not a loop); the longest back-to-back repeat of a 3-word group has 13 steps (about 5 repeats) at 199.5 min, of a 5-word group 7 steps at 164 min, of an 8-word group 1. No run of the kind found on Oct 28 (one phrase for minutes).
- **Word times:** 2,161 words (5.3 %) last under 40 ms, i.e. stacked word times in places (the same effect as the one stretch seen in the English run, here much more often); the finished transcript's text is complete but the times of those words are not usable.
- **Invented sign-off lines** ("subscribe to the channel", "thanks for watching" type): not counted by this report; the guard of AI-1.AC12 to AC14 can be run on this transcript (`crates/cm-ai/examples/outro_guard.rs`) to give that number.

## What this shows

1. **No crash on a 4 h 13 min file** (the longest run so far; the Nov 9 crash was in `median_filter` on short windows; the vendored guard is in this build). One file: "did not recur in this run", not "fixed".
2. **Speed:** 2.98 times real time with large-v3 on Vietnamese at the same time as speakers; a 4-hour file takes about 1 h 25 min. The first run (English alone) was 5.1 times. Whether it is the language, the concurrent job or the second readings needs a controlled pair of runs.
3. **Memory is the weak point:** 4.3 GB for the transcription alone, but **11.7 GB with the speaker job at the same time**, on a 48 GB computer. On a 16 GB Mac this combination would swap or fail. Starting speakers after the transcription (or reading in windows) would halve the peak.
4. **Quality on Vietnamese is the larger risk than speed:** a quarter of the words are below 0.6 confidence; stacked word times in 5 % of words.

## Not recorded in this run (improvements for the next report)

- Whether the 48 kHz float master (2.9 GB for 4 h; the 2 h file's was stereo, also 2.9 GB) should be 16 kHz for imported speech: disk use is 12 times the MP3 here (`specs/long-audio.md` point 1).
- Number of pieces, repaired and split pieces and loops caught (the pieces report is still not stored with the transcript).
- Where the time goes during "Checking the transcript" and the 6-second pauses at low CPU seen in the first run.
- The GPU use (no sampler for it), and the time of the first partial piece on screen.
- A controlled pair: the same file with and without the speaker job at the same time; the same file with `large-v3-turbo` and PhoWhisper small.
- The same 4-hour file on a 16 GB computer (swap).
