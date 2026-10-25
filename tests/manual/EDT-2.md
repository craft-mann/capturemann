# EDT-2 manual script: per-segment speed, pitch by ear, audio muted above 2x

Covers what the automated tests cannot: your ears (is the pitch really unchanged, does it sound
natural, are there clicks at the segment edges), the speed badge and inspector in the window, and
the exported file in a real player. Durations, the pitch measurement on synthetic tones at 0.75x,
1.5x and 2x, and the muting above 2x are covered by `cargo test -p cm-core -p cm-audio -p cm-media
-p cm-daemon` (EDT-2.AC1 to AC3). Nothing is recorded: use the synthetic fixture from
[`M4-GATE.md`](M4-GATE.md), the 60 s copy without a pause:

```sh
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY \
  --title "Lesson 60" --secs 60 --no-pause          # prints the id: ID=rec_...
```

Sound in the fixture: a steady quiet **300 Hz** tone (system audio) and a **1 kHz beep every 3 s**
(microphone). The picture shows a changing digit and flashes with each beep, so a faster segment is
visible as a faster digit. Use headphones and keep the volume low.

A pitch measurement of a steady tone (mute the microphone first so only the 300 Hz tone is left:
`capturemann edit audio ID --mic-mute true`), for an exported file `F` and a window of the tone at
`START` seconds of the export:

```sh
ffmpeg -v error -ss START -t 3 -i "F" -vn -ac 1 -ar 48000 -f f32le - | python3 -c "import array,sys;d=array.array('f');d.frombytes(sys.stdin.buffer.read());c=sum(1 for a,b in zip(d,d[1:]) if (a<0)!=(b<0));print(round(c/2/(len(d)/48000),1),'Hz')"
```

1. **Set a speed (AC1).** In the editor split at 20 s and 40 s (**S** at each), select the middle
   segment and press **2x** in the inspector, or `capturemann edit speed ID 20.39-40.39 --x 2`.
   Expected: a **2x** badge on the segment, the segment is drawn at its stored width (the axis is
   session time), and the header length drops from 59.81 s to **49.81 s** (a 20 s segment lasts
   10 s; the full export is 59.81 s because every export starts at `live_at`, 0.58 s). Export and check
   with ffprobe: 49.81 s, give or take one frame.
2. **See it.** Play across the segment. Expected: the digit counts up twice as fast inside the
   segment and normally outside it, the beeps come every 1.5 s inside it.
3. **Pitch at 1.5x (AC2).** `capturemann edit speed ID 20.39-40.39 --x 1.5`, mute the mic, export.
   Measure the tone before the segment (`START` 5) and inside it (the segment starts at 19.8 s of
   the export and lasts 13.3 s, so `START` 22). Expected: both read **300 Hz within 3 %**
   (291 to 309 Hz). By ear: the tone has the same pitch inside and outside, with no warble,
   no chipmunk or slow-motion voice. Listen at the two edges of the segment: no click and no
   dropout longer than a blink.
4. **Pitch at 2x and 0.5x.** Repeat with `--x 2` (measure at `START` 22) and `--x 0.5` (the
   segment lasts 40 s, `START` 25): both still 300 Hz within 3 %. By ear, 0.5x sounds slow
   but not lower.
5. **Muted above 2x (AC3).** `capturemann edit speed ID 20.39-40.39 --x 4`, export. Expected: the
   segment (5 s long in the export) has **no system tone and no beeps** while the sound before and
   after is normal. In the inspector, 4x shows the "Keep audio" switch off.
6. **Keep audio.** `capturemann edit speed ID 20.39-40.39 --x 4 --keep-audio`, export. Expected:
   the segment now has sound; at 4x a 300 Hz tone is still measured within 3 % of 300 Hz (the
   pitch is kept), and speech-like sound is fast but not high-pitched.
7. **Range limits.** `--x 0.25` and `--x 8` are accepted; `--x 0.2` and `--x 9` fail with
   `invalid_input` and exit code 2. Speed on a range with the camera bubble: the bubble plays at
   the same speed (one timeline); check by looking at the camera's flash against the screen's.
8. **Undo.** `capturemann undo ID` once removes a speed change completely (the badge disappears
   and the length returns); `capturemann redo ID` puts it back.
9. **Cuts and speed together.** `capturemann edit cut ID 25-30` inside the 2x segment. Expected:
   the length is 49.81 - 2.5 (5 s of source at 2x) = **47.31 s**; play across the cut at speed: no
   pop, the digit continues.

Pass criteria: lengths within one frame, the pitch within 3 % by measurement and not noticeably
different by ear at 0.5x, 1.5x and 2x, no clicks at segment edges, above 2x muted unless "Keep
audio". Record the measured Hz values.

## Privacy

No camera, microphone or screen is used; the sound comes from the fixture.

## Status of the author's runs

(none yet)
