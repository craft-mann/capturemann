# CAM-DEFAULTS manual script: Camera and Microphone dropdowns and the preview card (CAM-4.AC4, AC5, AUD-1)

Covers what CI cannot: real devices, a real unplug, the indicator light and VoiceOver. The rules
(dropdown model, device resolution in the daemon for every combination, the Record sheet's first
choice, the caption in English and Vietnamese, the preview geometry, the live-preview start and
stop rules) are tested with fakes: `cargo test -p cm-desktop --lib webcam`,
`cargo test -p cm-desktop --lib recording::start`, `cargo test -p cm-daemon recording::params`
and `cargo test -p cm-daemon --test recording_engine configured_devices`.

**Privacy:** step 3 and step 4 record your screen, camera and microphone for about 10 s each and
step 7 turns the camera on for the preview. Delete the recordings afterwards. Use the bundled app
(`make bundle`, then open it) so the permissions apply. Needs Screen Recording, Camera and
Microphone granted (step 8 turns Camera off on purpose). You need two cameras (the built-in one
and a USB or Continuity camera you can unplug) and two microphones (for example the built-in one
and a headset).

## 1. The page

Settings, Webcam & audio. Pass when: the page opens on the preview card (a grey wireframe screen
with a bubble and the caption pill), then the "Webcam bubble" card whose first row is Camera
("Used for the bubble and the camera track.") and the "Audio" card whose first row is Microphone
("System default input unless you pick one."). No camera light is on (look at the green dot in the
menu bar and the light beside the lens).

## 2. Choose each device

1. Open the Camera dropdown. Pass when: the first entry is the default camera's name with a
   "Default" tag, followed by every camera, the chosen one has a check mark and a lighter fill,
   and there is no "Off" entry. Pick the USB camera. Pass when: the list closes and the dropdown
   now shows its name.
2. Open the Microphone dropdown. Pass when: same layout; there is no "Off" entry. Pick the
   headset.
3. Quit and open the app again. Pass when: both dropdowns still show your choices.
4. Pick "System default" (the first entry) in each. Pass when: the dropdown shows the default
   device's name again.

## 3. The choice is used

Pick the USB camera and the headset again, then:

1. Open the Record sheet, choose Display, switch Webcam bubble and Microphone on. Pass when: the
   chips show the USB camera and the headset as the first choice, and the bubble preview shows
   that camera. Start, record about 10 s, stop.
2. Open the recording in the library, Show in Finder, Show Package Contents, open `manifest.json`.
   Pass when: the camera track details and the mic track name the USB camera and the headset
   (the `settings` block of the manifest holds their ids).
3. In the Record sheet pick the built-in camera and the built-in microphone for this recording
   only and record 10 s. Pass when: the manifest names those two. Then open Settings. Pass when:
   the dropdowns still show the USB camera and the headset (a choice made in the sheet is not
   saved).
4. Hotkey: press the record hotkey (default Option+Shift+R) with camera and microphone on in the
   sheet's last choices. Pass when: the recording uses the USB camera and the headset. Stop.
5. CLI: `capturemann record start --camera on --mic default --display 1` (confirm the dialog),
   record 10 s, `capturemann record stop`. Pass when: the manifest names the USB camera and the
   headset. Repeat with `--camera <ID of the built-in camera>` (see `capturemann devices`). Pass
   when: the manifest names the built-in camera.

## 4. Unplug the chosen camera

1. With the USB camera chosen, unplug it. Pass when: within a few seconds the Camera dropdown
   (page open) shows its name with a "Not connected" tag and the line "Using the system default
   until it is plugged in." appears under the row. Open the list: the saved camera is at the end
   with "Not connected" and a check mark, and "System default" is not marked.
2. Open the Record sheet. Pass when: it says the camera chosen in Settings is not connected and
   uses the system default. Record 10 s. Pass when: the manifest names the built-in camera.
3. Run the CLI start with `--camera on` again. Pass when: it records with the built-in camera.
4. Plug the USB camera back in. Pass when: the note and the tag disappear and the dropdown shows
   its name without a tag; the next recording uses it. (Settings never lost the saved id.)
5. Repeat 1 to 4 with the headset for the microphone.

## 5. The preview card follows the rows

Change Shape (Circle, Rounded), Size (S, M, L), Mirror and Remember position. Pass when: the
bubble on the card changes at once: a circle or a 4:3 rounded rectangle, three sizes in the real
proportions (S about two thirds of M, L a third larger), the silhouette's head moves to the
other side with Mirror, the caption reads for example "Preview . L, rounded, not mirrored", and
with Remember position on after dragging the bubble in a recording the card shows the remembered
corner (off: bottom-left). Switch the language to Tiếng Việt: the caption reads "Xem trước . M,
tròn, đã lật".

## 6. No camera is used by the card

With "Show live camera" off, leave the page open for 30 s. Pass when: the camera's indicator
light stays off and the menu bar shows no camera in use.

## 7. Live preview and the light

1. Click "Show live camera". Pass when: within about 2 s the silhouette is replaced by your
   camera (the one chosen in the dropdown; with a saved camera that is gone, the default one), the
   button reads "Hide live camera", and the camera's light is on. Toggle Mirror: the picture
   flips (the recording never does).
2. Click "Hide live camera". Pass when: the picture goes back to the silhouette and the light goes
   out within 2 s.
3. Turn it on, then open another Settings page. Pass when: the light goes out; come back, the
   button reads "Show live camera" again (it is never remembered).
4. Turn it on, then click another app. Pass when: the light goes out; come back, it is off.
5. Turn it on, close the window to the menu bar (Cmd+W). Pass when: the light goes out.
6. Turn it on, quit and open the app. Pass when: it is off after launch.

## 8. Permission missing

In System Settings, Privacy & Security, Camera, turn CaptureMann off (or use a test Mac). Reopen
the page. Pass when: the "Show live camera" button is replaced by "Camera permission is missing"
and an "Open System Settings" button that opens the Camera pane, the hint "Camera access is off.
Turn it on in System Settings; you can also record without the camera." shows under the card, and
nothing starts a camera. Turn the permission back on.

## 9. VoiceOver

Turn VoiceOver on (Cmd+F5), open the page. Pass when: the Camera dropdown is announced as a
button with its name and state, for example "Camera: FaceTime HD Camera, button, collapsed";
activating it announces a list with options; each option reads its name (and "Default" or "Not
connected"), the chosen one is "selected"; Tab moves between options; activating one closes the
list and the dropdown reads the new name. The Microphone dropdown behaves the same. The preview
card is announced as a group "Webcam bubble preview"; "Show live camera" is a toggle button
(on or off). The note "Using the system default until it is plugged in." is reachable by reading
the page.
