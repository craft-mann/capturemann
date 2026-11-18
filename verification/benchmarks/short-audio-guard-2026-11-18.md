# Short-audio guard and doubtful lines: thresholds (AI-1.AC12, AC12b, AC16)

- **Date:** 2026-11-18. **Status:** PARTIAL. Synthetic clips only (macOS arm64, models `large-v3-turbo` and `large-v3` from the maintainer's models folder, nothing downloaded, no private recording read). Not "verified": no real invented line over a real room was met, so the doubtful rule's positive side is tested with scripted words only.
- **Tool:** `cargo run --release -p cm-ai --example short_guard_probe -- make|stats` (new). Speech is macOS `say` voices (Samantha, Daniel), scaled to a level over its active frames; the room is uniform noise. Decoding: `silence_probe --audio`, language forced to Vietnamese (the guard's language), which also turns English speech into low-confidence text, the hardest case for a confidence rule.
- **Clips (27, 2 to 25 s):** two bursts of 1.5 s and 2.3 s at about -33 dBFS over a room of -60 (the report's shape), room only (9 s, 20 s, -50 dBFS), speech 6, 12 and 25 s, quiet speech (-45 over -55, -52 over -60), 12 fragments of 0.7 to 1.6 s over rooms of -60, -55 and -50 dBFS, clips of 2 to 5 s, 6 s of digital silence then 3 s of content.

## Reference level on short audio (minimums)

| Clip | Non-silent | Room | Speech (90th) | Range | Judged |
|---|---|---|---|---|---|
| speech 2 s, 3 s | 2 s, 3 s | -60.3 | -31.9, -31.0 | 28 to 29 dB | no (under 4 s) |
| speech 4 s, 5 s, 6 s | same | -60.4, -60.4, -60.3 | -31.0 to -30.7 | 29 dB | yes |
| room only 9 s, 20 s | all | -60.4 | -59.8 | 0.6 dB | no |
| quiet speech -45 over -55 | 12 s | -55.2 | -43.0 | 12.2 dB | no (15 dB for short audio) |
| quiet speech -52 over -60 | 12 s | -60.3 | -49.9 | 10.3 dB | no |
| fragments, rooms -60/-55/-50 | 10 s | -60.4 to -50.3 | -44.9 to -32.1 | 15.5 to 27.4 dB | yes (lowest 15.5) |
| 6 s of zeros, then 3 s | 3 s | -60.3 | -31.8 | 28.6 dB | no (zeros do not count) |

Room estimate (10th percentile of frames that are not digital silence) moves 0.1 dB between 2 s and 25 s on this noise, so 4 s (200 frames, 20 frames per end) is the floor taken; real rooms fluctuate more, not measured. The 15 dB range for audio under 30 s keeps the lowest judged fragment clip (15.5 dB) in and the 10 to 12 dB quiet-speech clips out; the long-audio 10 dB stays.

## Doubtful rule (mostly low-confidence words, not speech-like audio)

- Real speech lines judged (reference present): about 50 lines of 1 to 20 words from `large-v3-turbo` and `large-v3`, forced to Vietnamese, up to 12 of 20 words under 0.5. Share of frames 10 dB over the room under the line: lowest 0.41, most 0.5 to 0.9. The threshold is 0.25 (under it is "not speech-like"), so none of them is doubtful.
- One single-word line of 0.1 s (confidence 0.16) had share 0.22 and was a false hit before the 0.25 s minimum of audio under a line was added; with it no line is hit.
- Room-only clips have no reference (no speech to compare with), so the rule does not fire there; Whisper wrote 10 to 15 words (stacked on the last 20 ms) over 9 s and 20 s of room, which stays as before. Clips with bursts and a quiet remainder produced no low-confidence text over the room in these runs: **no positive was met in the real engine**.
- Real daemon path (`cm-daemon` and `capturemann import --transcribe --language vi` on 3 clips with private config, library and socket): 0 lines dropped, 0 sounds, 0 words with an end at or before the start.

## Word ends (P-1)

Last words stacked at the end of a piece (8.96 to 8.98 s in a 9 s clip) end at the piece end now; a last word that ends more than 1 s before the piece end keeps its end (a first try that stretched it to the end of the piece gave a 15 s word over silence in a 20 s clip, so the 1 s limit was added).

## Not done

- Real invented text over a room (needs a recording of the kind in the report); the maintainer's 9 s recording was not read.
- English transcripts: the rule runs on Vietnamese only, like AC12.
- Intel and Windows: nothing platform-specific, not run.
