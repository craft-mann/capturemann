# AUD-MUTE manual script: mute the microphone while recording (AUD-6)

Covers what CI cannot: the real HUD button, a real microphone and real sync. The mute logic
itself (boundary, denoised file, pause, crash recovery, events, MCP, CLI) is covered by
`cargo test -p cm-audio --test mic_mute`, the daemon tests (`aud6`) and the HUD model tests.

**Privacy:** this records your screen and microphone for about 20 s. Speak or play a steady tone
the whole time; delete the recording afterwards. Use the bundled app (`make bundle`, then open it)
so the permissions apply. Needs Screen Recording and Microphone granted.

## 1. Mute from the HUD

1. Open the Record sheet, turn Microphone on (denoise on), start a recording with a 0 s or 3 s
   countdown. The HUD order is Stop, timer, Pause, Mic, Camera, Draw, Restart, Discard.
2. Speak or play a steady tone. At about 5 s click the Mic button. Pass when: the icon becomes
   the slashed microphone in the red tint, the level bars disappear and its tooltip reads
   "Unmute microphone" (VoiceOver: "Unmute microphone").
3. At about 10 s click it again. Pass when: the icon returns to the plain microphone with the
   level bars, tooltip "Mute microphone".
4. Stop at about 20 s.

## 2. The files

Open the recording folder (Show in Finder on the card, then Show Package Contents).

- Open `mic.wav` and `mic.denoised.wav` in an audio editor, or run
  `ffmpeg -i mic.wav -af silencedetect=noise=-120dB:d=1 -f null -` .
- Pass when: both files are about as long as the recording; from about 5 s to about 10 s the
  waveform is a flat line (digital silence, no noise floor, no click at the edges) and sound is
  present before 5 s and after 10 s.
- `manifest.json` has `"mic_muted": [{ "from": ~600000, "to": ~1200000 }]` (120 000 ticks per
  second; values within a fraction of a second of your clicks) and the same list under
  `session.mic_muted`.

## 3. An agent or the CLI mutes, the HUD follows

1. Start another recording with the mic on. In a terminal run `capturemann record mic --mute`.
   Pass when: the HUD Mic button turns to the muted state within a second, and
   `capturemann record status` prints `microphone: muted`.
2. Click the HUD Mic button. Pass when: the HUD and `capturemann record status` show unmuted.
3. Repeat `capturemann record mic --mute` twice. Pass when: the second answers "already muted".
4. With an MCP client call `set_mic_muted { "muted": false }`. Pass when: no dialog appears and
   the HUD follows.
5. Start a recording with the mic off. Pass when: the Mic button is disabled and its tooltip says
   "Microphone is off for this recording"; `capturemann record mic --mute` fails with
   `invalid_input` (exit 2).

## 4. Sync after a muted span

Record a clap or a click track with the screen visible (a stopwatch app works). Mute for 5 s in
the middle, unmute, clap again. Pass when: in the editor the second clap is heard within one
video frame of its picture, as it was before the muted span (the track was never restarted).

## 5. Pause while muted

Mute, pause for 3 s, resume, unmute. Pass when: the sound returns right at the unmute, the files
have no hole at the pause, and `mic_muted` has one span that started at the mute and ended at the
unmute.

Record the run in `verification/` (template in its README) before calling the feature verified.
