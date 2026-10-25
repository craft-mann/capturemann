# REC-5 manual script: recording UI (HUD, countdown, area indicator, webcam bubble, tray, toasts)

Covers what unit tests cannot: real windows, real hotkeys, the menu bar, VoiceOver, and the
guarantee that none of the overlay windows ends up in `screen.mp4`. The logic (HUD state, timer
math across pauses, geometry, bubble presets and snapping, tray state, start options, settings
keys) is covered by `cargo test -p cm-desktop` (tests cite REC-4, REC-5, REC-6, CAM-1 and CAM-4
acceptance criteria).

Use the bundle (`make run-bundle`), not `target/release/cm-desktop`, whenever permissions matter
(Screen Recording, Camera, Microphone are granted to the app that runs the binary).

## 0. Look at the UI without recording anything (demo mode, safe)

The demo runs the real engine over **fake tracks**: nothing is captured, no microphone or camera is
opened. Use throw-away folders:

```
export CAPTUREMANN_SOCKET=/tmp/cmp-f/d.sock CAPTUREMANN_LIBRARY=$(mktemp -d) CAPTUREMANN_CONFIG_DIR=$(mktemp -d)
mkdir -p /tmp/cmp-f
cargo run -p cm-desktop -- --demo-recording                  # HUD, brackets, bubble (synthetic camera)
cargo run -p cm-desktop -- --demo-region --demo-paused       # region scrim, dashed outline, Resume
cargo run -p cm-desktop -- --demo-recording --demo-countdown 10   # the countdown
cargo run -p cm-desktop -- --demo-agent                      # agent toast and chip
cargo run -p cm-desktop -- --demo-sheet                      # the start sheet
```

Add `CM_DEMO_CAPTURABLE=1` only to screenshot a single window with `screencapture -l <window id>`
(it stops hiding the windows from capture; never use it for a real recording).

## 0b. Start sheet: buttons always visible (regression: Cancel / Start cut off)

The sheet's window height is computed from the rows it shows and re-fitted whenever they change
(devices arrive, Window or Region chosen, camera on or off). The rows between the header and the
footer scroll; the footer (Cancel, Start recording, the reason Start is off) never does.
Debug-only helpers for the demo (the `--demo-sheet` run from section 0):

```
CM_DEMO_SHEET_KIND=display|window|region     # target kind the sheet opens with
CM_DEMO_SHEET_DEVICES=<json file>            # a list_devices reply, e.g. every permission "denied"
CM_DEMO_DISPLAY=<n>                          # open on the n-th display (0 = primary)
CM_DEMO_VISIBLE_H=520                        # pretend the display is 520 pt tall
CM_DEMO_SHEET_TRACE=1                        # print window height, estimate, hidden overflow
```

1. Open the sheet (launcher -> "Record screen + webcam", or the tray's "Record..."). Click
   Display, Window, Region in turn: after each click the full sheet is inside its window and
   Cancel and Start recording are fully visible and clickable. In Window and Region Start is dimmed
   and the line "Choose a window to record." sits above the buttons.
2. Turn the webcam on with two cameras, deny Camera and Microphone in System Settings, and
   reopen: the notes and their "Open System Settings" buttons show, the window grew to hold them,
   and the footer is still fully visible. The camera note opens the Camera pane, the microphone
   note the Microphone pane.
3. Short display (or `CM_DEMO_VISIBLE_H=520`): the window stops 24 pt from the menu bar and
   the Dock, the middle scrolls with the wheel or trackpad, the footer stays put.
4. With the webcam on, the live bubble must not hide any part of the sheet; if it overlaps the
   sheet, the sheet is on top (it sits one window level above the bubble).
5. Keyboard: Esc cancels, Enter starts when Start is enabled, Tab reaches every control and
   the focus ring is visible on Cancel and Start.
6. Two displays: put the pointer on the secondary display and run "Record screen + webcam".
   The sheet appears on that display (centered, upper third, clear of its menu bar) and the live
   webcam bubble appears on the same display. Repeat with the secondary above or left of the
   primary (negative coordinates).
7. Launcher: after choosing the command the launcher is gone before the sheet shows (no flash),
   and the sheet has the keyboard right away (type nothing; press Enter or Esc to check).
8. Light and dark system appearance: the sheet is the dark overlay surface in both.

## 1. HUD (REC-5.AC1, AC6, AC11, AC14, AC15)

1. Start any recording (demo or real). Within 200 ms the HUD, the area brackets and the tray icon
   show.
2. Order left to right: Stop (red, black square), timer with a red dot, divider, Pause, Mic with
   five small bars, Webcam, Draw, divider, Restart, Discard (x). Buttons are 44 px; the timer is
   Geist Mono.
3. Hover each button: a hint with the name (and the shortcut for Stop/Pause) shows above the bar.
   Draw is dimmed, hover says "Draw - Coming soon", clicking it does nothing.
4. Speak: the Mic bars follow your voice (real run). Mic does not mute (the engine has no mute yet).
5. While the HUD is clicked or dragged, the app you are recording keeps keyboard focus (type in it).
6. Keyboard-only (NFR-13): the HUD is a non-activating panel, so Tab does not reach its buttons
   while another app has the keyboard; that is by design (clicking it must not take focus from the
   app you record). Every action has a global shortcut instead: Stop or start Option+Shift+R,
   Pause or resume Option+Shift+P, Discard Option+Shift+D, Restart Option+Shift+T (each asks first,
   and the question can be answered with the keyboard: Cancel has the focus, Tab to the red button,
   Space). Settings -> Shortcuts lists them and lets you change them. VoiceOver (VO+arrows) reads
   "Recording controls, toolbar" then each button's name: "Stop recording", "Pause recording",
   "Microphone on, level meter", "Hide webcam bubble", "Draw on screen, coming soon", "Restart
   recording", "Discard recording", and the timer as "Recording, 00:12".

## 2. Pause and resume (REC-4.AC1, AC4, REC-5.AC10)

1. Record 10 s, press Pause (HUD or Option+Shift+P) for 20 s, Resume, record 10 s, Stop. The timer
   showed 00:10 while paused (dot grey, `mist`) and reads 00:20 at the end.
2. While paused the outline around the area is dashed (2 px, 70% red); after Resume it is solid.
   The brackets never change.
3. Resume shows a 1 s countdown (Settings -> Capture & recording -> "Countdown before resuming").
4. Press Option+Shift+P during the start countdown: the countdown cancels and nothing is recorded.

## 3. Area indicator (REC-5.AC9)

1. Record a display: four white brackets at the corners, a thin red outline on the edge, the chips
   `3024 x 1964` and `2x - native` at the top left.
2. Record a region: the rest of the screen is dimmed (55%). Record a window: brackets and outline
   on the window, no dimming.
3. Click through the indicator to the app below it: clicks reach the app (the indicator ignores the
   mouse).
4. After stopping, open the recording in the system player: no brackets, outline, chips, HUD,
   countdown or bubble in `screen.mp4` (REC-5.AC9, CAM-1.AC2). Check the first 3 s (countdown) and a
   paused stretch.

## 4. Webcam bubble (CAM-1, CAM-4)

1. In the start sheet turn the webcam on: the bubble appears at once with your live camera, a 4 px
   red ring, no shadow, in the bottom-left corner (24 px margin).
2. Hover it: a toolbar pill of seven buttons appears ABOVE the bubble (board R18): S, M, L
   (160 / 240 / 320 px, the current one highlighted), a divider, circle, rounded, mirror, camera
   off. Each change is remembered for the next recording. Tooltips and Tab focus rings show.
3. Drag it, in the sheet and during a recording: after you let go it snaps to a corner or edge
   middle (24 px margin) only when within 32 px, else it stays exactly where dropped (and never
   leaves the recorded area). Stop and start again: it is back in the bottom-left (CAM-1.AC8).
   Turn on Settings, Webcam & audio, "Remember where I put the bubble", drag, record again: it
   opens where you left it. Turn it off again: bottom-left. Stop a recording after dragging and
   export: the bubble sits where you left it, not bottom-left.
4. HUD Webcam button hides and shows the bubble; the camera keeps recording.
5. With the camera denied in System Settings the sheet says so, offers "Open System Settings", and
   Start still works without the camera. With the camera unplugged: "No camera found".
6. The preview latency feels live (CAM-1.AC1: under 100 ms).

### 4b. No square behind the bubble, toolbar above (regression: glass/backdrop square; R18)

The bubble window is the bubble plus a transparent band for the toolbar (46 px, on top; below the
bubble when it is within 46 px of the top of the display). Nothing but the circle (or rounded
rectangle) with its 4 px ring, and the toolbar while it shows, may be visible or take clicks: no
fill, no window shadow, no backdrop. Logic is covered by the geometry tests
(`the_toolbar_is_wider_than_the_small_bubble_and_pinned`, `the_toolbar_sits_above_the_bubble_when_there_is_room`,
`the_toolbar_flips_below_when_there_is_no_room_above`, `clicks_on_empty_window_area_pass_through`,
`snapping_uses_the_bubble_not_the_larger_window`, `only_the_circle_and_ring_are_hit`).

1. Run the demo from section 0 with `CM_DEMO_CAPTURABLE=1`. List windows (any tool, e.g. a small
   Swift `CGWindowListCopyWindowInfo` script) and find the cm-desktop window at level 101: its
   size is max(bubble, 235) wide and bubble + 46 tall (S: 235 x 206, M: 240 x 286, L: 320 x 366).
2. `screencapture -l <id> bubble.png` (without `-o`): every pixel outside the circle (distance
   from the centre greater than the radius + 1.5 px) has alpha 0, including the corners and the
   toolbar band (toolbar hidden). Check with Swift (`CGImage` + `CGContext`) or Pillow.
3. Put the bubble over a bright window and over a dark one: only the circle and ring show.
4. Click on a window under the bubble's corner or under the empty band above the bubble: the click
   reaches that window. Click inside the circle and drag: the bubble moves.
5. Hover the circle: the toolbar pill appears above the bubble, centred, 6 px clear of the ring.
   Move the pointer up onto a button without leaving the bubble and the pill: it stays (no
   flicker in the gap). Move away: it disappears, leaving no square. Click each button: size and
   shape change in place, mirror flips the preview, the last one hides the bubble (the HUD camera
   button brings it back).
6. Near-edge flip: drag the bubble to the top-left corner. After it snaps (24 px margin, less than
   the 46 px the band needs) the toolbar shows BELOW the bubble on hover. Drag it back down: above.
7. Side edges: with S in the left or right corner the pill (235 px) stays on the screen, shifted
   from the bubble's centre, and nothing is cut off. Repeat on a second display.
8. Recording: start a recording with the bubble on, hover it and use the toolbar, stop. Play
   `screen.mp4`: no bubble, toolbar or band in any frame (CAM-1.AC2); the camera track has it.
9. VoiceOver or the accessibility inspector: "Webcam bubble" toolbar, buttons "Size Small", "Circle
   shape", "Mirror on", "Hide the bubble".
10. Switch to Rounded: same checks, corners cut at 24 px.
11. The bubble is still not in `screen.mp4` and stays above other windows.

## 5. Discard and Restart (REC-5.AC3, AC12, AC13)

1. Click Discard: "Discard this take?" with Cancel focused. Cancel keeps recording; Discard take
   stops it and deletes the files (no entry in the library). No toast.
2. Click Restart: "Discard this take and start over?". Confirm: the HUD stays, a new countdown
   runs, then a new recording (new id) starts with the same settings. Esc during that countdown
   ends everything with nothing recorded.
3. From an agent (`capturemann` CLI or MCP): `stop_recording`, then `start_recording`: the Confirm
   dialog shows "Requested by <client>" and nothing starts until you accept. An agent calling
   `discard_recording` or `restart_recording` gets the same kind of dialog.

## 6. Agent-started recording (AGT-2, REC-5)

1. Start a recording from an agent and accept the dialog: the countdown, HUD and brackets show as
   usual, the HUD strip carries an "Agent - <client>" chip, and a toast "Started by agent via MCP"
   with the tool name in mono shows top right (blue border).
2. Stop and Discard in the HUD work the same.

## 7. Tray and hotkeys (REC-5.AC7, CAP-2)

1. While a countdown or recording runs the menu bar icon is the red recording icon (not a template
   image); the tooltip reads "CaptureMann - Recording 00:42" (or "Paused 00:42"). When it ends the
   normal template icon returns.
2. The tray menu offers Record... (enabled when idle), Pause recording / Resume recording and Stop
   recording (enabled while recording).
3. Option+Shift+R starts a recording of the main display with the remembered options when idle and
   stops it when recording; during the countdown it cancels. Option+Shift+P pauses and resumes.
4. Quit from the tray while recording: the app waits for the bundle to close (up to 10 s), then
   exits; the recording is in the library.
5. Force-quit the app during a recording, start it again: a toast "Recovered recording from <time>
   (lost up to 2 s)" shows and the recording is in the library.

## 8. Full-screen apps (REC-5 edge case)

Present a Keynote or PowerPoint deck in full screen and start a recording: the HUD and the bubble
stay above it on every Space.

## 9. Real recording smoke test (needs your consent: it records the screen, the mic and the camera)

Run the bundle, open the launcher (Option+Shift+Space), choose "Record screen + webcam", turn the
microphone and webcam on, Start, speak for 5 s, press Stop in the HUD. Pass when the toast "Recording
saved" appears, the entry shows in Recordings with a poster and the right length, and the details
card plays it. Delete the recording afterwards.

## 10. Window lifecycle (R4-1 to R4-12; the logic is unit tested, these are the parts that need a screen)

Use `--demo-recording` where it says demo (the real engine on fake tracks: no screen, camera or
microphone is touched).

1. A start that fails stays clean (R4-1, R4-4). Make a start fail after the countdown began (for
   example point the library at a volume with too little free space), open the sheet, turn the
   camera on, press Start. Pass when: an error toast shows, the countdown disc closes by itself,
   the tray icon is the idle one, "Record..." is enabled again, the webcam bubble and the camera
   light are off, and Option+Shift+R starts a new attempt. (The same checks cover a failed Restart.)
2. Resume countdown (R4-2, R4-11). Pause, then Resume with "Countdown before resuming" at 1 s:
   the small disc shows for one second on the area and is gone when recording continues (it never
   stays on screen). While it shows, the app you record keeps the keyboard (keep typing in it).
   Press Resume and, during the disc, click it and press Esc: the take stays, paused. A stray Esc
   while recording or paused does nothing.
3. Start countdown keys (R4-11). The start countdown does not take the keyboard from the app
   underneath. Click the disc: Space skips it, Esc cancels it. Option+Shift+R cancels it without a click.
4. Lost closing event (R4-3). Recording, then in a second terminal run `capturemann` to stop the
   recording while the app's event stream is blocked (`kill -STOP` of the app for 5 s, then
   `kill -CONT`): within a second of the app waking, the HUD, the brackets, the bubble and the tray
   state are gone and the toast "The recording ended" or the saved toast shows. Do not do this to
   your real running app: use `--demo-recording` with `CAPTUREMANN_SOCKET` under `/tmp/cmp-m/`.
5. Stale question (R4-7). Press Discard in the HUD and leave the question open; stop the recording
   with Option+Shift+R. The question window closes by itself. Start a new recording: it is not
   affected.
6. Bubble click (R4-8). Click the bubble once without dragging, then move the pointer off it and
   over it: the toolbar appears and disappears with the pointer, and clicks on the transparent
   corners reach the app underneath.
7. Self-test (R4-9, REC-3, NFR-11). Run `capturemann record selftest` (it captures the screen for
   about 10 s, deletes the output, and asks for approval first). While it runs: the white
   brackets and red outline show around the display, the tray icon is the recording one, a small
   bar with Stop and "Testing 60 fps - N s left" shows. Stop cancels the test. Pass when all of it is
   gone afterwards and a toast says "60 fps test passed" or "did not pass".
8. Reduced motion (R4-13). Turn on System Settings -> Accessibility -> Display -> Reduce motion: the
   countdown ring stays full and only the digit changes, and the HUD's red dot does not blink.
9. Keyboard (R4-12). Take a screenshot, then press Option+Shift+A: the editor opens on it. While
   recording, Option+Shift+D and Option+Shift+T open the Discard and Restart questions.
10. Single instance (R4-10). Start the installed app, then start it again (Finder, or `open -n`):
    the second launch brings the first to the front and exits; there is one menu bar icon. "Restart
    CaptureMann" in Settings -> Permissions relaunches once the old one has quit.
11. Tray menu (CAP-2.AC4). While recording the first item of the menu bar menu is "Stop recording".
12. Screenshots exclude the app's own floating windows (R4-6). Leave a "Saved" toast open, take a
    display screenshot with Option+Shift+3 from a second display or the launcher: the toast is not in
    the image. The Studio window may appear in a screenshot on purpose (REC-6.AC8): with the library
    window on top of another app, the screenshot shows it.
13. Recording includes the library window (REC-6.AC8). Record the display with the library window
    open on top of another app: it appears in the video; the HUD, countdown, bubble and any toast
    do not. See also `REC-1.md` step 1a.

