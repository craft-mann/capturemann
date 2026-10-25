# CAM-1 manual script: camera plug and unplug (`camera.connected`, `camera.disconnected`)

Covers what CI cannot: a real USB camera appearing and disappearing. The logic (the diff of the device
list, the daemon publishing the events, the Record sheet reloading its list, the recording continuing
after an unplug) is covered by `cargo test -p cm-camera watcher`, `cargo test -p cm-daemon --test
camera_events` and `cargo test -p cm-desktop cam1` with a scripted watcher. Nothing here has been run
on a Mac yet; write down what you observe, also when it differs from "Expected".

**Needs:** a USB (UVC) webcam you can unplug, the app from `make bundle` with Camera allowed (see
`PRM-1.md`), and a second terminal for `capturemann record watch --topic 'camera.*' --topic
'recording.*'`. The macOS watcher polls the device list every 2 s, so an event can come up to about
2 s after the cable moves.

**Privacy:** the steps that record turn the camera on. Keep each take to about 15 s and delete it
afterwards.

## While idle
1. Unplug the USB camera. Start the watch command. Plug the camera in. Expected within about 2 s:
   one `camera.connected` event with `device_id`, `name` (the camera's name), `kind: external` and
   `is_default: false`. No second event for the same plug.
2. Unplug it. Expected within about 2 s: one `camera.disconnected` event with the same `device_id`.
3. Open the Record sheet (tray > Record, or the hotkey) with the camera unplugged and turn the camera
   on: the sheet lists only the built-in camera. With the sheet still open, plug the USB camera in:
   within about 2 s the camera list in the sheet shows it (the picker appears when two cameras are
   listed). Unplug it again: the list drops back to one camera; if the USB camera was selected, the
   sheet falls back to the system default and says so.
4. Plug in a Continuity Camera (iPhone) or start OBS Virtual Camera if you have them: each gives a
   `camera.connected` (kind `continuity` or `virtual`). Not seeing one is worth a note (legacy virtual
   cameras are best effort, see `list_cameras`).
5. With no client of the app open (quit the app, run `cm-daemon` headless), the same watch output
   appears: the events do not need the app.

## While recording (the camera track)
6. Pick the USB camera and record for 15 s. After about 5 s pull the cable. Expected: the bubble shows
   the camera-off placeholder, `recording.warning` with `code: camera_lost` and `camera.disconnected`
   both appear (in either order), the screen recording continues and the HUD timer keeps running.
7. Plug the camera in again after about 3 s. Expected: `camera.connected` appears. Whether the camera
   track resumes is not built on macOS yet (the track keeps its gap and no `camera_restored` is
   raised): note what you see.
8. Stop the recording after 15 s. Expected: it is saved, `camera.mp4` plays up to the unplug, the
   screen and mic tracks play to the end, `manifest.json` has a `camera_disconnected` warning.
9. Unplug while paused, plug in before resuming: both events appear, the resume works, nothing
   crashes.

## Pass criteria
Steps 1 to 3, 6 and 8 as described. Steps 4, 5, 7 and 9 are reported as observed.
