# HUD-1 manual script: move and collapse the recording HUD (REC-5.AC4, AC16, AC17)

Covers what unit tests cannot: the native window drag, the window resize on Collapse and Expand,
a real display being unplugged, keyboard access with a screen reader, and the guarantee that the
HUD never appears in a recording. The pure logic (the collapsed bar's slots and widths, the
toggle's label and icon, keeping the snapped edge on resize, ignoring a damaged remembered
position, the "wholly on a connected display" check) is covered by `cargo test -p cm-desktop`
(`rec5_ac16_*`, `rec5_ac17_*`). The app-only rule for `recording.hud_collapsed` is covered by
`cargo test -p cm-daemon` (`r2_3_app_only_keys_are_denied_to_agents_only`, `agent_layer`).

Run it on **Windows** (installed package) and on **macOS** (the bundle, `make run-bundle-dev`,
never `target/release/cm-desktop`, so Screen Recording is granted to the app). Steps 1 to 6 need
one display; step 7 needs a second monitor you can unplug. Recordings made here are throw-away:
delete them from the library at the end. The recording captures your screen and microphone: close
anything private first.

Record the result in `verification/HUD-1-<date>-<platform>.md` (template in
`verification/README.md`).

## 1. Drag (screen recording)

1. Start a screen recording of the whole display (Record, Display, Start).
2. Press on the HUD's **background** (the padding next to the buttons) or on the **timer** and drag.
   **Pass:** the HUD follows the pointer; the cursor is a grab hand over the background and the
   timer.
3. Press on **Pause**, **Mic**, **Stop** or any other button and move the pointer a little before
   releasing. **Pass:** the HUD does not move; a full click still runs the button.
4. Drop the HUD 20 px from the right edge. **Pass:** it snaps to the edge margin and is wholly on
   the display. Try to drop it half off the bottom edge. **Pass:** it is pulled back on screen.
   On Windows it never covers the taskbar.
5. Click outside the HUD (on the desktop or the recorded app). **Pass:** the click reaches what is
   under it; the HUD takes no clicks outside its own bar.

## 2. Collapse and expand (screen recording)

1. Click the chevron at the right end of the bar. **Pass:** the bar shrinks to Stop, the timer
   with its red dot, Pause and the chevron (now pointing right). If the HUD was snapped to the
   right edge it keeps its right edge; snapped left it keeps its left edge; elsewhere it keeps its
   centre. It stays wholly on the display.
2. In the collapsed bar: Pause, Resume, then the global hotkeys (Discard, Restart, mute and
   pause). **Pass:** they work as in the full bar.
3. Hover the chevron. **Pass:** the hint says "Expand recording controls" (Vietnamese: "Mở rộng
   điều khiển quay").
4. Click it. **Pass:** the full bar returns (Stop, timer, Pause, Mic, Webcam, Draw, Restart,
   Discard, chevron) at the same place it had before collapsing.
5. Keyboard: click the timer once (the HUD becomes key), press Tab until the chevron has the focus
   ring, press Space, then Enter. **Pass:** it collapses and expands. With VoiceOver (macOS) or
   Narrator (Windows) the button reads "Collapse recording controls" / "Expand recording controls"
   and its expanded state.
6. Collapse the bar, then stop the recording.

## 3. Remembered across recordings and restarts

1. Start a new screen recording. **Pass:** the HUD opens collapsed, at the place it was left.
2. Expand it, drag it to the top-left corner, stop.
3. Quit CaptureMann completely (tray, Quit) and open it again. Start a recording. **Pass:** the
   HUD opens expanded in the top-left corner.
4. Agent check (optional): `capturemann settings set recording.hud_collapsed true` (or the
   `set_settings` MCP tool). **Pass:** refused with `user_declined` / `app_only_setting`; the HUD
   is unchanged.

## 4. Audio recording

1. Start an audio recording (⌥⇧V on macOS, Ctrl+Alt+Shift+V on Windows, or the Record sheet's
   Audio mode).
2. Drag the audio HUD by its background or the meter. **Pass:** it moves and snaps as in step 1.
3. Collapse it. **Pass:** Stop, timer, Pause and the chevron only (no meter, Mic or Marker).
4. Press the marker hotkey (⌥⇧M / Ctrl+Alt+Shift+M). **Pass:** the marker is set (it shows in the
   editor later); the label prompt does not show while collapsed.
5. Expand, stop. **Pass:** the next audio recording opens with the remembered state and place.

## 5. Not in the recording

1. Open the screen recordings made in steps 1 to 3 in the editor (or any player) and scrub through
   the moments when the HUD was dragged, collapsed and expanded. **Pass:** the HUD, its hints and
   the resize never appear in the video.

## 6. Damaged remembered position

1. Quit CaptureMann. In the settings file of a **throw-away** config folder
   (`CAPTUREMANN_CONFIG_DIR=<empty folder>`), set `recording.hud_positions` for your display to
   `[40, -3]`. Start the app with that folder and start a recording. **Pass:** the HUD opens at
   the default bottom-centre place.

## 7. Display unplugged or rearranged (two monitors)

1. With a second monitor connected, start a recording of the **second** monitor (Display, pick
   it), so the HUD opens there. (A HUD dropped on a display that is not recorded snaps back to the
   recorded one: that is today's behaviour.)
2. Unplug the second monitor during the recording (or, on Windows, Settings, Display, "Show only
   on 1"; on macOS, mirror the displays). **Pass:** within about a second the HUD is wholly on the
   remaining display, at its remembered place there or bottom centre. The recording itself
   behaves as it does today without this feature.
3. Rearrange displays (System Settings, Displays, Arrange; Windows, Display, drag the monitors) so
   the HUD's old place is outside every display. **Pass:** the HUD comes back onto a display.
4. Stop. Start a new recording on the remaining display. **Pass:** the HUD opens on screen.

## Clean up

Delete the recordings made here from the library. On macOS, if you used a throw-away config
folder, remove it.
