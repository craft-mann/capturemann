# PLAY-1 manual script: editor preview playback, scrubbing, sync and sound

Covers what no automated test can: the editor window (untested by eye), playback that looks
smooth, sound from the real output device, and audio and picture staying together. The decoder,
the pacing and the mix are covered by `cargo test -p cm-media -p cm-audio -p cm-desktop` with
fixtures (`cm_media::decode`, `recedit::pacer`, `cm_audio::playback`). Nothing is recorded: use
the synthetic fixture from [`M4-GATE.md`](M4-GATE.md) ("One-time setup"). Use headphones or
speakers you can hear; keep the volume low, the beep is loud.

1. **Open.** In the Library select the "Fixture" recording and press **Edit**. Expected: the editor
   opens on the recording (no spinner for more than a second), the preview shows a colour pattern
   with a large changing digit, and the timeline shows the whole recording with the pause as a
   gap or marker. The status strip says "Decoded from the recording with FFmpeg" (not "test
   pattern"). If it says the test pattern, FFmpeg is missing: stop, install it.
2. **Play.** Press Space. Expected: the digit counts up steadily, the preview does not stutter or
   freeze, and the playhead moves smoothly. You hear a quiet steady tone (the 300 Hz system audio)
   and a short **beep every 3 seconds** (the microphone). Each beep happens exactly when the
   picture flashes white (flashes at session 1.39, 4.39, 8.39, 11.39 s). Pass: you cannot see
   or hear the flash and beep apart (about 50 ms is the limit, NFR-3).
3. **Pause skipped.** Let it play across session 5.89 to 6.89 s. Expected: playback continues
   without a 1 s silent freeze (pauses are removed); the transport time jumps accordingly.
4. **End and loop.** Let it reach the end. Expected: it stops (Space plays from the start). Press
   **L**, play again: it loops without a click or a gap longer than a moment.
5. **Scrub paused.** Pause. Drag the playhead across the timeline slowly and quickly. Expected:
   the picture follows within about a tenth of a second, no old frame stays on screen after you
   stop, and the CPU stays calm (Activity Monitor: the app under 100 % of one core while
   scrubbing is fine).
6. **Scrub while playing.** Play, click the ruler at another place. Expected: the picture and
   sound jump together and keep in sync (flash and beep still together afterwards).
7. **Frame step.** Pause, press the right arrow 30 times. Expected: the digit advances by one per
   press (30 fps fixture), left arrow goes back; Shift+arrow moves one second.
8. **Edits while playing.** Play, then press **S** (split) and set the second segment's speed to
   2x in the inspector. Expected: the preview reaches that segment, the digit counts up twice as
   fast, the sound switches to the new mix shortly after the edit (a short silence while the
   mix is rendered is acceptable, a wrong pitch is not), and nothing crashes.
9. **Mute and gain.** `capturemann edit audio <id> --mic-mute true`, then play. Expected: no
   beeps, the tone stays. `--mic-mute false --system-gain -20`: beeps return and the tone is
   much quieter. (The preview picks the change up without reopening the editor.)
10. **No audio device.** Unplug headphones or choose an output with nothing connected, play.
    Expected: playback continues on the clock without sound; no hang.
11. **Long clip.** Make `--secs 60` and play it for 30 s. Expected: still in sync at the end
    (flash and beep together at the flash near 30 s), memory does not climb steadily (Activity
    Monitor), no dropped-frame look.

Pass criteria: steps 1 to 8 PASS with flash and beep together, no freeze at the pause, no stale
frame after a scrub. Write down anything that stuttered, the CPU you saw while playing, and which
output device you used.

## Privacy

No camera, microphone or screen is used. The output device plays the fixture's tones.

## Status of the author's runs

(none yet)
