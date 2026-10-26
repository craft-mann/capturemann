# VOC-1 manual script: audio-only recording (sheet, HUD, markers, consent, lid)

Covers what the fakes cannot: the Record sheet's Audio mode drawn on screen, the live microphone
meter, the compact HUD, markers while recording, the consent line, and the lid. The rules are
tested without a display (`cargo test -p cm-desktop --lib audio_sheet`, `hud_layout`,
`recording::model`; `cargo test -p cm-daemon --test voice_audio`); the screens were **not checked
by eye** when this script was written, so every "looks like the board" line below is a check, not a
known fact. Boards: `design/redesign/screens/png/VoiceRecordSheet.png` (V1) and `VoiceHud.png`
(V2).

**Privacy:** steps 2 to 9 record your microphone and, in step 6, whatever the computer plays. Keep
runs short (under a minute), say nothing private, and delete the recordings afterwards (Library >
Audio, or the `.cmrec` folders in your library). Use the development bundle (`make bundle-dev`,
bundle id `dev.capturemann.app.dev`), never your daily copy. Screen Recording is only needed in
step 6.

## 1. Entry points (no recording)

- Tray menu: "Record audio" sits under "Record…". It opens the Record sheet already in **Audio**
  mode, 760 pt wide. "Record…" opens it in Screen mode, 600 pt wide.
- Launcher (⌥⇧Space): type "audio" or "meeting": the "Record audio" row shows ⌥⇧V and opens the
  sheet in Audio mode.
- In the sheet's header the **Screen / Audio** switch changes the mode and the window width (the
  centre stays). Each segment shows its shortcut (⌥⇧R, ⌥⇧V).
- Settings > Shortcuts, Recording group: "Start or stop audio recording" (⌥⇧V) and "Add a marker"
  (⌥⇧M). Try rebinding ⌥⇧M to ⌥⇧V: "Already used by Start or stop audio recording". Reset to
  defaults puts both back.
- Pass when all of the above hold and the Windows variants read Ctrl+Alt+Shift+V / M.

## 2. The sheet, Audio mode

Open it from the tray with Microphone already allowed to the dev bundle.

- Five preset cards: In the room (selected, rec tint), Online meeting, Lecture or talk, Product
  demo, Voice note. Choosing Online meeting or Product demo turns System audio on; the others turn
  it off. The sources can still be changed after choosing.
- Microphone row: the device picker shows your microphone; the 40-segment meter moves when you
  speak and stays dark when the switch is off. Open the picker, choose another microphone: the
  meter follows it. The macOS orange microphone dot is on while the sheet is open in Audio mode
  and goes out when you close the sheet or switch to Screen.
- "How many people?" shows Not sure, 2, 3, 4, 5+ (not for Voice note). It changes nothing in the
  recording yet (the speaker step is a later stage).
- "After you stop": Transcribe (with the model name when a model is installed and automatic
  transcription is on, else "if turned on"), Speaker labels, Suggest: the report of the preset.
- Footer line: "About ... GB per hour · ... GB free", "Keep the lid open: closing it stops the
  recording" (In the room only), "Stops by itself after 4 h". Free space matches Finder to within a
  few GB.
- Compare with the board: spacing, 14 px card radius, the rec tint on the selected card.
- Pass when everything above holds and Escape closes the sheet without recording.

## 3. Consent line (VOC-1.AC7)

- With `recording.consent_notice` on (default) the sheet shows "Let everyone know they're being
  recorded." with "Don't show again", for every preset with other people.
- Press "Don't show again": the line goes at once. Open the sheet again: it is still gone, for
  every preset (the setting is false in `settings.json`). Set `recording.consent_notice` back to
  true (or reset) to see it again.
- A Voice note shows the line only for the very first audio recording, then not again even with
  the setting on.
- ⌥⇧V (no sheet) shows an "Recording audio" toast carrying the same sentence while the setting is
  on.
- Pass when all four behave this way. (An agent's `start_recording { mode: "audio" }` dialog always
  carries the line: `cargo test -p cm-daemon --test voice_audio`.)

## 4. Start, HUD and meter (VOC-1.AC1, AC3)

1. In Audio mode, microphone only, press Start recording (or Enter). With the default countdown
   the countdown disc shows, then the HUD.
2. The HUD is the compact bar: Stop, the timer, a 14-bar meter, a divider, Pause, Mute mic,
   **Marker M**, Discard (red trash). No area brackets anywhere, no webcam button, no Draw, no
   Restart. No outline window covers the display.
3. Speak: the meter bars light left to right (grey, green from the tenth). Pause: the meter goes
   dark and the dot goes grey; Resume continues.
4. Mute mic: the button gets the red tint and the slashed icon; the meter stays dark; unmute.
5. The tray icon shows the recording state; its first item is Stop.
6. Stop. The recording is in the library as an audio recording (Library > Audio once that view
   exists; until then in All).
- Pass when the bundle has `"kind": "audio"`, a mic track of the length you recorded (within 20 ms
  of the timer) and no screen, camera or cursor files, and **no Screen Recording prompt appeared**.

## 5. ⌥⇧V and ⌥⇧M (VOC-1.AC3, AC4)

1. With nothing running, press ⌥⇧V: the countdown, then the compact HUD, with the preset and
   sources last used in the sheet.
2. Press ⌥⇧M at about 0:10: the HUD strip shows "Marker at 00:10" with a field "Add a label
   (optional)". Type "budget" and press Enter: the strip closes. Press ⌥⇧M again and ignore it: it
   closes by itself after about 8 s; Esc closes it at once.
3. Click the HUD bar (it takes the key focus), press **M**: a marker is added. Click the label field
   and type an "m": no marker is added.
4. Pause for 30 s, resume, add another marker at 0:40 of recorded time. Stop with ⌥⇧V.
5. Open `manifest.json` in the `.cmrec` folder: `markers` has the entries at about 10 s and 40 s of
   recorded time (the pause is not in them), the first labelled "budget", `by` "user".
6. With a screen recording running, ⌥⇧V says "A recording is already running"; with nothing
   running ⌥⇧M says "No recording to mark".
- Pass when all six hold.

## 6. Online meeting, system audio (VOC-1.AC2, needs Screen Recording)

1. Grant Screen Recording to the dev bundle. Choose "Online meeting", Start.
2. Play a YouTube video or a call for 30 s, stop.
- Pass when the bundle has `mic.wav` and `system.wav` that start and end within 50 ms of each
  other, and the system track holds what played.
- Revoke Screen Recording and open the sheet: with System audio on, the row shows the note "System
  audio needs Screen Recording permission..." with Open System Settings, the footer explains it,
  and turning System audio off (microphone only) lets Start through.

## 7. Permissions (microphone)

- Revoke Microphone for the dev bundle (System Settings > Privacy & Security > Microphone, or
  `tccutil reset Microphone dev.capturemann.app.dev`; never the release id). Open the sheet in
  Audio mode: the Microphone row shows "Microphone access is off..." with Open System Settings,
  the meter stays dark, the orange dot never appears, and Start is off with "Microphone access is
  off for CaptureMann." unless System audio is on.
- With the permission never asked (after the reset), turning the switch off and on asks macOS;
  pressing Start with the microphone on and nothing asked yet also asks once and then waits for a
  second press.

## 8. Lid and long recordings (VOC-1, Edge cases)

- In the room shows "Keep the lid open" in the sheet; Online meeting does not.
- Start a recording on a MacBook with no external display and close the lid: the Mac sleeps, and
  when you open it the recording has stopped with what it captured (reason `system_sleep`; the
  HUD is gone and a toast names the saved recording).
- With the lid open and the display asleep, the recording keeps running (an idle-sleep assertion
  is held): record 2 minutes with the display off and check the length.
- Set `recording.audio_max_duration_s` to 60: a recording stops by itself at 1 min, and the sheet
  says "Stops by itself after 1 min".
- Pass when sleep and the limit stop the recording and keep the file.

## 9. Agent started recording (VOC-1.AC8, AGT-2)

- From a terminal: `capturemann record start --audio --preset room`. The confirm dialog says
  "Record audio" with the microphone name and the consent line, nothing about a screen. Approve:
  the countdown and the HUD show, with the chip "Started by <client>" at the right of the bar.
- `capturemann record marker --label "from the CLI"` adds a marker; the HUD strip does not ask for
  a label (it is only for markers set at the keyboard). `capturemann record stop`.
- Pass when the dialog, the chip and the marker behave this way.

## Report

Store the run in `verification/` (template in its README). Until a report exists, the screens and
the live meter are "tested with fakes, not checked by eye".
