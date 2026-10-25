# EDT-1 manual script: trim, cut, restore, split and undo

Covers the editor window (untested by eye), the exported file played by a real player, and the
keyboard. The arithmetic (merging, clamping, the 0.5 s minimum, undo) and the exported duration
and sync on FFmpeg fixtures are covered by `cargo test -p cm-core -p cm-daemon -p cm-media -p
cm-desktop` (EDT-1.AC1 to AC5). Nothing is recorded: use the synthetic fixture from
[`M4-GATE.md`](M4-GATE.md). The 60 s copy has no pause, so the numbers are plain:

```sh
cargo run -p cm-daemon --example make_fixture -- --library $CAPTUREMANN_LIBRARY \
  --title "Lesson 60" --secs 60 --no-pause          # prints the id: ID=rec_...
```

Times on the CLI are session seconds; the screen starts at 0.39 s, so the recording runs from 0.39
to 60.39 s. A flash and a beep happen every 3 s at session 1.39, 4.39, 7.39, ... 58.39 s.

Handy measurements for an exported file `F`:

```sh
ffprobe -v error -show_entries format=duration -of default=nw=1 "F"          # length
ffmpeg -i "F" -vf "select='gt(scene,0.3)',showinfo" -f null - 2>&1 | grep -o "pts_time:[0-9.]*"   # flash edges
ffmpeg -i "F" -af silencedetect=noise=-30dB:d=0.1 -f null - 2>&1 | grep silence_end               # beeps
```

For the beep measurement, mute the system tone first (`capturemann edit audio ID --system-mute
true`) so only the microphone beeps are left, and unmute it after.

1. **Trim (AC1).** `capturemann edit trim ID --start 2 --end 58`, then `capturemann export ID
   --wait`. Expected: the command prints "the export is now 56.00 s", and the exported file is
   56.0 s long, give or take one frame (0.033 s). In the editor the in and out handles sit at 2 s
   and 58 s and the parts outside are dimmed.
2. **Handles and keys.** Open the recording in the editor (**Edit** on its card). Put the playhead
   at 5 s (click the ruler) and press **I**; move to 50 s and press **O**. Expected: the handles
   jump to the playhead and the length in the header updates. Drag a handle: it snaps (toggle with
   **N**) to the playhead, split lines and frames. Cmd+Z undoes each step.
3. **Cut (AC2).** Undo back to the full recording. `capturemann edit cut ID 10-15`. Expected:
   "the export is now 54.81 s" (the full export of this fixture is 59.81 s, because every export starts at `live_at`, 0.58 s; minus the 5 s cut) and, in the editor, 10 to 15 s shows hatched. Play
   across it: the picture and sound skip it with no click and no freeze.
4. **Sync after the cut.** Mute the system tone, export, and run the flash and beep commands.
   The source flashes at 10.39 and 13.39 s are inside the cut; the next one, 16.39 s, must now
   appear at **10.81 s** of the export (16.39 - 0.58 - 5; adjust if you also trimmed), and the beep
   at the same time. Pass: flash and beep within one frame (0.033 s) of each other, and at
   10.81 s +/- 0.05.
5. **Restore (AC3).** Select the hatched area in the editor and press **R** (or `capturemann edit
   restore ID 10-15`). Expected: the full content returns (the CLI prints 59.81 s and the exported file is 59.8 s: every export starts at `live_at`) and one Cmd+Z
   (or `capturemann undo ID`) brings the cut back: one undo step either way.
6. **Split.** Put the playhead at 30 s and press **S** (or `capturemann edit split ID --at 30`).
   Expected: a boundary appears in the timeline; the export length does not change (a split alone
   changes nothing; speed and layout use it; the file stays 59.8 s).
7. **Several cuts, one step (AC5).** `capturemann edit cut ID 10-12 20-22 30-32`. Expected: the
   export is 53.81 s. `capturemann undo ID` once: all three cuts disappear together.
8. **Masters untouched (AC4).** Run `shasum -a 256 <bundle>/screen.mp4 camera.mp4 mic.denoised.wav
   system.wav cursor.jsonl` before the first step and after the last one. Expected: identical.
9. **Edge cases.** Cut `10-15` then `14-18`: the timeline shows one hatched block 10 to 18 (the
   ranges merge). `capturemann edit cut ID 59-120`: it is clamped to the end, no error.
   `capturemann edit cut ID 0-60.2` leaves under 0.5 s: expected an error
   `invalid_input` with "Nothing left to export", exit code 2, and nothing changed.
10. **Undo history.** Press Cmd+Z repeatedly until nothing is left and Shift+Cmd+Z back up.
    Expected: each press moves exactly one edit, the CLI
    (`capture get` works for screenshots only; `capturemann undo ID` and `redo` print the revision
    they land on) shows the same revision as the title bar, and the export length follows.

Pass criteria: every expected length within one frame, flash and beep together within one frame
after the cut, one undo step per edit (including the three-range cut), masters byte-identical.
Record the lengths you measured.

## Privacy

No camera, microphone or screen is used. The exports are of the synthetic fixture only; delete the
test library when done.

## Status of the author's runs

(none yet)
