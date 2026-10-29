# Performance

How fast CaptureMann's local AI steps are, where the time goes, and what was tried. Numbers come from `tests/tools/perf_matrix.py` and `crates/cm-ai/examples/perf.rs` (one process per measurement); the stored tables are in `verification/benchmarks/`. Machine: Apple M4 Pro, 14 cores (10 performance), 48 GB. Other chips have not been measured; the Intel build is not measured at all.

The app also records these timings for the last run of each kind: Settings, About, Diagnostics (and `capturemann diagnostics`) show them in the "Last runs" section.

## Measured (PERF-1, release 0.2.4)

| Step | Result on the M4 Pro |
|---|---|
| Transcribe, Large v3 Turbo (default), GPU | 27x real time (English meeting), 14x (88 min Vietnamese: 370 s); 2.0 to 2.4 GB peak |
| Transcribe, Large v3 | 13.5x and 4.1x (88 min: 21 min); 3.8 to 4.2 GB peak |
| Transcribe, Medium | 22.6x and 9.0x; cut 91 repeating loops in the 88 min Vietnamese recording |
| Threads (4 to 14) with the GPU | no effect (25.0 to 25.2 s); the GPU is 2.7x faster than the CPU |
| Decode to 16 kHz mono, model load | 1.2 s for 88 min; 0.4 s |
| Find speakers, count known | about 20x real time, 88 min in 217 s (was 275 s with 4 threads); 1.1 GB |
| Find speakers, count unknown | 540 s for 88 min (see below) |

## Done in 0.2.4

1. **Speaker finding starts with the automatic transcription** instead of after it. They run side by side: 11 min 52.6 s to 37 s, 88 min 587 s to 483 s, same transcript text. The transcript itself arrives 20 % later in that case.
2. **Speaker finding uses up to 8 threads on Apple Silicon** (the number of performance cores) instead of 4: 17 to 21 % faster, identical turns. Other computers keep `min(4, cores)` because nothing is measured there.

## Written down, not done

Ranked by what a user would notice.

1. **An unknown-count speaker run can run the engine twice.** When the first pass leaves more than 8 voices after small ones are folded, the whole engine runs again with exactly 8 (88 min: 265 s plus 263 s). The second answer differs a lot (10 voices in the first pass against 4 large ones and 4 almost empty ones), so folding the extra voices without a second run is not the same thing. Options: expose the voice prints from the engine so only the grouping repeats (needs a change inside sherpa-onnx), or window the run. The automatic step after a recording gives a count and runs once.
2. **Memory of speaker finding grows with length**: 0.6 GB at 10 min, 1.2 GB at 60, 3.5 GB at 150. Windowed runs would cap it and let Cancel work (the library ignores a stop until the run ends).
3. **Repeat re-runs**: the 88 min Turbo run made 29 Whisper calls for 17 pieces because looping pieces are split and run again. The cost of those second runs is not measured.
4. **Speed of the window overlap**: `window_shift_ratio` 0.1 is the main cost of speaker finding; larger values are 2x to 6x faster but changed the answers (see the stored report). Needs the threshold retuned on more than 14 clips.
5. Reports, translation, export, app start, library and editor open: not measured yet.

## Rejected

- More Whisper threads with the GPU on: no effect. Fewer Whisper threads while speakers run: no effect.
- 14 speaker threads: slower than 8 (efficiency cores).
- Speeding up audio decode: 1.2 s of 700 s.
