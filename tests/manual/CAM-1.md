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

## Windows notes (0.2.8)
Run the same steps in the logged-in session of a Windows PC. Differences: cameras come from Media Foundation
(`devices` / `list_cameras` shows the Integrated Camera with its modes; a built-in camera cannot be unplugged, use a
USB webcam for steps 6 to 9, or the camera privacy switch in Settings > Privacy & security > Camera for the "denied"
case). A camera that offers 1280x720 at 30 fps in NV12 or MJPG is recorded at 30 fps only when the room is bright
enough; many laptop cameras halve the rate in low light (the lab laptop recorded about 10 fps at 720p in a dark
room): note the light and the measured rate (`ffprobe -show_entries stream=nb_frames,r_frame_rate camera.mp4`).
`camera.mp4` is H.264 from the Media Foundation hardware encoder; check it with `ffprobe` and that it plays.

### Windows: camera comes back during a recording (0.2.8, built, not yet run on hardware)
Unlike macOS, the Windows camera source reopens the same camera by itself (back-off 0.25 s doubling to 2 s, until the
recording stops). Needs a USB webcam; a built-in camera cannot be unplugged (Device Manager > Cameras > right-click the
camera > Disable device, then Enable device, also works and needs no cable).
W1. Start `capturemann events` (or the watch command above), pick the USB camera and record for about 25 s.
W2. After about 5 s unplug the camera (or disable it). Expected: the bubble shows the camera-off placeholder,
    `recording.warning` `camera_lost` and `camera.disconnected`; screen and mic keep recording.
W3. After about 5 s more plug it in again (or enable it), into the same USB port. Expected within a few seconds: the
    bubble shows the picture again, an Info note `camera_restored` ("The camera is back") appears and
    `camera.connected` appears.
W4. Repeat W2 and W3 once. Expected: it comes back again.
W5. Stop the recording. Expected: `camera.mp4` plays with a freeze or gap for each unplug (the screen and mic tracks run
    to the end and stay in sync with the pictures after the gap), `manifest.json` has `camera_disconnected` and
    `camera_restored` warnings in that order, per unplug.
W6. Unplug and do not plug in again; stop after 10 s. Expected: the recording is saved, no hang on Stop.
W7. Plug it into a different USB port instead of the same one and note whether it comes back (Windows may give the
    camera a new device id on another port; the app waits for the same id, so it may not resume; report what you see).
Privacy: the camera light is on during this test and the picture is recorded; use a neutral scene and delete the
recording afterwards.

## Linux notes (0.3.0)
Run the same steps in the logged-in desktop session (X11 or Wayland) of an Ubuntu PC; set `CM_LINUX_RECORDING=1` for the
app or daemon while Linux recording is opt-in. Differences: cameras are the Video4Linux2 nodes `/dev/video*` that
capture pictures (a UVC camera also has a metadata node, which is not listed); the id is the `/dev/v4l/by-id/...` link,
so the same camera keeps its id in another USB port. There is no permission prompt: if the user cannot open the device
(not in the `video` group, no desktop login), the Permissions page shows Camera as Denied and `list_devices` says so.
A built-in camera cannot be unplugged; use a USB webcam for the unplug steps, or `sudo modprobe -r uvcvideo` and
`sudo modprobe uvcvideo` (removes and restores every UVC camera). `camera.mp4` is software VP9 (decision O10: no
software H.264 ships; a hardware H.264 path through VA-API is not wired for the camera yet): `ffprobe` shows `vp9`
and the track details in `manifest.json` say `"codec": "vp9"`. The source reopens the same camera by itself after an
unplug (steps W1 to W6 apply as written, with `modprobe` for W2/W3 on a built-in camera). Note the room light and
`ffprobe -show_entries stream=nb_frames,r_frame_rate camera.mp4`.

## Pass criteria
Steps 1 to 3, 6 and 8 as described. Steps 4, 5, 7 and 9 are reported as observed. On Windows and Linux also W1 to W6 (W7 as observed).
