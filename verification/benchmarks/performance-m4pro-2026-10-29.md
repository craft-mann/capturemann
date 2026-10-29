# Performance, Apple M4 Pro, Oct 29, 2026

Machine: Apple M4 Pro, 52 GB, 14 cores (10 performance), macOS 26.6.2, Now drawing from 'AC Power'. Measured with commit `0f824a6` (the driver was changed afterwards only to fix the decode rows). Release build. Each row is the median of its runs (min to max shown for wall time); peak memory is the process high-water mark.

Clips (ids only; the recordings are private and not published):

- `en11`: 11.3 min, language en, meeting, 2 speakers
- `s1`: 1.0 min, language vi, short, real, one voice
- `vi88`: 88.0 min, language vi, meeting, several speakers

## Transcription (Whisper, the app's piece-by-piece path)

| clip | model | setting | audio min | runs | wall s (min-max) | x real time | load s | decode s | peak MB | words | loops cut |
|---|---|---|---|---|---|---|---|---|---|---|---|
| s1 | large-v3 |  | 1.0 | 3 | 9.0 (8.9-9.2) | 7.2 | 0.76 | 0.0 | 3744 | 247 | 0 |
| en11 | large-v3 |  | 11.3 | 3 | 51.0 (51.0-51.4) | 13.5 | 0.73 | 0.2 | 3820 | 1601 | 0 |
| vi88 | large-v3 |  | 88.0 | 1 | 1283.4 (1283.4-1283.4) | 4.1 | 0.72 | 1.2 | 4205 | 16190 | 0 |
| s1 | large-v3-turbo |  | 1.0 | 3 | 3.4 (3.4-3.4) | 20.1 | 0.43 | 0.0 | 1940 | 241 | 0 |
| en11 | large-v3-turbo |  | 11.3 | 3 | 25.6 (25.5-26.1) | 26.8 | 0.43 | 0.2 | 2045 | 1581 | 0 |
| en11 | large-v3-turbo | gpu off | 11.3 | 3 | 68.1 (66.0-71.0) | 10.0 | 0.36 | 0.2 | 2559 | 1588 | 0 |
| en11 | large-v3-turbo | gpu off, threads 10 | 11.3 | 3 | 64.8 (63.0-67.2) | 10.5 | 0.38 | 0.2 | 2561 | 1588 | 0 |
| en11 | large-v3-turbo | threads 10 | 11.3 | 3 | 25.2 (25.1-25.2) | 27.3 | 0.40 | 0.2 | 2038 | 1581 | 0 |
| en11 | large-v3-turbo | threads 14 | 11.3 | 3 | 25.2 (25.1-25.2) | 27.2 | 0.40 | 0.2 | 2038 | 1581 | 0 |
| en11 | large-v3-turbo | threads 4 | 11.3 | 3 | 25.1 (25.0-25.1) | 27.4 | 0.40 | 0.2 | 2039 | 1581 | 0 |
| en11 | large-v3-turbo | threads 6 | 11.3 | 3 | 25.0 (25.0-25.0) | 27.5 | 0.41 | 0.2 | 2041 | 1581 | 0 |
| en11 | large-v3-turbo | threads 8 | 11.3 | 3 | 25.2 (25.1-25.4) | 27.3 | 0.41 | 0.2 | 2044 | 1581 | 0 |
| vi88 | large-v3-turbo |  | 88.0 | 1 | 370.2 (370.2-370.2) | 14.3 | 0.42 | 1.3 | 2397 | 14360 | 0 |
| s1 | medium |  | 1.0 | 3 | 5.9 (5.9-5.9) | 10.8 | 0.38 | 0.0 | 1973 | 246 | 0 |
| en11 | medium |  | 11.3 | 3 | 30.2 (30.1-30.2) | 22.6 | 0.37 | 0.2 | 2060 | 1479 | 0 |
| vi88 | medium |  | 88.0 | 1 | 584.8 (584.8-584.8) | 9.0 | 0.38 | 1.2 | 2461 | 15662 | 91 |

## Speaker finding (speaker count unknown)

| clip | setting | audio min | runs | wall s (min-max) | x real time | peak MB | speakers |
|---|---|---|---|---|---|---|---|
| en11 | whole file | 11.3 | 3 | 33.8 (33.6-34.1) | 20.0 | 310 | 3 |
| vi88 | first 10 min | 10.0 | 1 | 28.0 (28.0-28.0) | 21.4 | 611 | 3 |
| vi88 | first 15 min | 15.0 | 1 | 45.0 (45.0-45.0) | 20.0 | 644 | 2 |
| vi88 | first 20 min | 20.0 | 1 | 56.7 (56.7-56.7) | 21.1 | 677 | 4 |
| vi88 | first 30 min | 30.0 | 1 | 95.2 (95.2-95.2) | 18.9 | 699 | 4 |
| vi88 | first 60 min | 60.0 | 1 | 184.9 (184.9-184.9) | 19.5 | 1210 | 4 |
| vi88 | tiled to 150 min | 150.0 | 1 | 455.6 (455.6-455.6) | 19.8 | 3474 | 7 |
| vi88 | whole file | 88.0 | 1 | 532.4 (532.4-532.4) | 9.9 | 1840 | 8 |

## Decoding with FFmpeg to 16 kHz mono

| clip | setting | audio min | runs | wall s (min-max) | peak MB |
|---|---|---|---|---|---|
| en11 |  | 11.3 | 3 | 0.2 (0.2-0.2) | 55 |
| vi88 |  | 88.0 | 1 | 1.2 (1.2-1.2) | 350 |
| vi88 | tiled to 150 min | 150.0 | 1 | 1.2 (1.2-1.2) | 927 |

## What the numbers say (first reading)

Speech to text, Large v3 Turbo with the GPU (the app's default):

- 27x real time on the English meeting, 14x on the 88 minute Vietnamese one. Turbo is 3.5x faster than Large v3 (4.1x real time, 21 minutes for 88 minutes) with the same peak memory class (2.4 GB against 4.2 GB).
- Thread count does not matter with the GPU (25.0 to 25.2 s from 4 to 14 threads) and barely matters without it (65 s at 10 threads, 68 s at the default 8). The GPU is 2.7x faster than the CPU. Nothing to tune here.
- Decode with FFmpeg is 1.2 s for 88 minutes, model load 0.4 s. Neither is worth work.
- The 88 minute Turbo run made 29 Whisper calls for 17 pieces (12 more for Large v3, 32 more for Medium): pieces that showed a loop are split and run again. The cost of those second runs is not measured yet.
- Medium cut 91 repeating loops in the long Vietnamese recording; Turbo and Large v3 cut none.

Finding speakers (CPU, one blocking call):

- A steady 19 to 21x real time from 10 to 60 minutes, and also on the file tiled to 150 minutes (456 s, 3.5 GB peak).
- The real 88 minute file takes 532 s (9.9x). A straight line from the 60 minute run predicts about 270 s, so about 260 s are extra. It found 8 voices and 1072 turns. The tiled file repeats the same voices, so it does not show this. The extra time is most likely in the grouping step, which grows faster than the audio length when there are many turns and voices. Not yet confirmed.
- Memory grows with length: 0.6 GB at 10 minutes, 1.2 GB at 60, 3.5 GB at 150.
- Whisper (370 s) and the speaker run (532 s) run one after the other today, so 88 minutes cost about 15 minutes in total.

Not measured yet: reports and translation (need the built-in text model), export, app start, library and editor open.
