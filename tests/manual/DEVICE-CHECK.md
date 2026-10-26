# DEVICE-CHECK manual script: Settings, Webcam & audio (board R13b, CAM-4.AC4, AC5, AUD-1)

Covers what CI cannot: real devices, a real unplug, the camera light, the orange microphone dot,
the chime on real speakers and VoiceOver. The rules (the start and stop rules, the permission
blocks, the verdicts and the meter, the dropdown model, device resolution in the daemon) are
tested with fakes: `cargo test -p cm-desktop --lib webcam`, `cargo test -p cm-desktop --lib
audio_check`, `cargo test -p cm-audio chime`, `cargo test -p cm-daemon recording::params`.

**Privacy:** the camera and the microphone are opened for the checks, and step 6 plays a sound.
Nothing is recorded or saved. Use the bundled app (`make bundle`, then open it) so the permissions
apply. You need two cameras (the built-in one and a USB or Continuity camera you can unplug) and
two microphones (the built-in one and a headset). Before step 6 run
`cargo run -p cm-audio --example chime_probe` once (it plays the chime and prints the verdict).

## 1. Opening the page opens nothing

Settings, Webcam & audio. Pass when: the camera card says "Camera is off" with "Turn on camera",
the microphone part says "Microphone is off" with "Test microphone", the system audio part is idle,
the camera light and the orange microphone dot are off, and no permission prompt appeared.

## 2. Camera check

1. Press "Turn on camera". Pass when: within about 2 s the live picture shows, the chip reads
   Live with the real size and rate (for example `1920 × 1080 · 30 fps`), the light is on, and no
   bubble or overlay is drawn on the picture. Turn Mirror on: the picture flips and "Mirrored, like
   the bubble" shows at the bottom left.
2. Open the Camera dropdown (first entry is the default camera with a "Default" tag, then every
   camera, no "Off"). Pick the USB camera. Pass when: the picture switches within about 1 s.
3. Press "Turn off camera". Pass when: the light goes out and the Off block returns.
4. Turn it on, then open another Settings page, click another app, close the window (Cmd+W), or
   start a recording from the tray. Pass when: the light goes out each time (a recording shows
   the note that the check is paused). Coming back, the camera is off again.
5. Unplug the USB camera while it is live. Pass when: "No camera found" with "Look again" shows
   and the app does not crash. Plug it in and press "Look again". Pass when: it recovers.
6. Start the camera in another app (for example Photo Booth), then press "Turn on camera". Pass
   when: "Camera is in use" with "Try again" shows and Try again works after you close the other app.

## 3. Microphone check

1. Press "Test microphone", speak normally. Pass when: within about 1 s the meter moves, the dB
   readout follows, the chip reads "Good level" at a normal voice, and the orange dot shows.
2. Whisper, then shout close to the mic. Pass when: "Too quiet" and "Too loud" appear (and the
   Clipping tip when the input volume is very high), with the meter kept visible. Note the dBFS
   readings at which the verdict changes; if they feel wrong, tell the maintainer (the band is about
   −30 to −8 dBFS).
3. Switch to the headset in the Microphone dropdown. Pass when: the meter follows the headset
   within about 1 s. Mute the mic with a hardware switch or unplug it. Pass when: "No sound coming
   in" (Silent) shows after about 2 s.
4. Press "Turn off microphone", and again start it and leave the page. Pass when: the orange dot
   goes out each time.

## 4. System audio check

Turn the volume up a little. Press "Play test sound". Pass when: you hear a short two-note chime,
the small meter moves, and the chip reads "Heard". Repeat with headphones or Bluetooth output and
with the output muted. Pass when: headphones still read Heard; muted reads "Chime not heard".

## 5. Permissions

In System Settings, Privacy & Security, turn CaptureMann off for Camera, Microphone and Screen
Recording one at a time. Pass when: pressing "Turn on camera", "Test microphone" or "Play test
sound" shows the matching "No permission" block and starts nothing, and "Allow access" opens
Settings, Permissions. Turn the permission back on and return to the page. Pass when: the check
works again. Opening the page never asked for a permission by itself.

## 6. Settings still save

Change Shape (Circle, Rounded), Size (S, M, L), Mirror, Remember where I put the bubble and Noise
suppression, and pick each device. Pass when: quitting and opening the app shows the same values,
the next recording uses the chosen devices (the `manifest.json` names them), and a device that is
unplugged shows "Not connected" with the note that the system default is used instead.

## 7. Layout and language

Resize the window narrow. Pass when: the camera and audio cards stack and nothing is cut. Switch to
Tiếng Việt. Pass when: every text fits and reads naturally. The Position hint reads under the
Webcam bubble card.

## 8. VoiceOver

Turn VoiceOver on (Cmd+F5). Pass when: the dropdowns read as buttons with their names, the
buttons (Turn on camera, Test microphone, Play test sound) read by label, and the verdict chips are
reachable.
