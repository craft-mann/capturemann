# REC-WINDOW manual script: Record sheet > Window (board R20)

Covers what unit tests cannot: the real window list, thumbnails (once the shared thumbnail service
is wired), the scroll area, keyboard use with VoiceOver, and what the finished recording holds.
The logic (parsing `list_targets`, search, the Last used group, the Privacy note, keyboard moves,
chips, Start enablement, footer text, sheet height) is covered by
`cargo test -p cm-desktop --lib recording::` (`recording::window_rows`, `recording::sheet`).

Use the bundle (`make run-bundle`) so the Screen Recording, Camera and Microphone grants apply.
Reports go in `verification/` (template in its README); say "verified" only with one.

Privacy: the recordings of this script show your windows. Keep them short, use test content, and
delete the recordings afterwards.

## Setup

- Open at least 20 windows across several apps (for example 8 browser windows, 6 editor or
  terminal windows, Finder, Mail, Notes). If you have a second display, spread some across it.
- Make one window sit completely under another one (covered).
- In Settings > Privacy, add an app that has a window open (for example `com.apple.Notes`) to the
  excluded apps. Note its display name.

## 1. The list

1. Open the sheet with Record... and choose the Window tile.
   Pass: an inline list replaces the single Choose button. Rows are in front-to-back order under
   the heading FRONT TO BACK; each row has a small thumbnail tile (a placeholder until the
   thumbnail service is wired), the app's initial in a coloured square, the title, the app name,
   the size in a monospace font (for example `1680 x 1050`) and an empty radio.
2. A window on a display that is not the built-in one shows `. External` after its size.
   Pass: only windows on the external display carry it.
3. The sheet has no minimized windows (minimize one and reopen the sheet: it is not listed).
4. Scroll with the wheel and the trackpad.
   Pass: the list scrolls inside a fixed-height area; the sheet does not grow or move; the footer
   (Cancel, Start recording) stays in place; a scrollbar thumb shows.
5. Start recording is disabled and the footer says "Choose a window to record." while no window is
   chosen.

## 2. Search

1. Press Cmd+F (Ctrl+F on Windows).
   Pass: the search box takes the focus; its placeholder says how many windows ("Search 22
   windows by title or app").
2. Type part of a title, then part of an app name, then something with an accent or a Vietnamese
   mark with and without the mark.
   Pass: the list filters on title and app, ignoring case and accents; the group headings go away
   during a search; "No window matches ..." shows when nothing fits.
3. Clear the box.
   Pass: the full list is back with its groups.
4. Type a search, press Enter.
   Pass: the top result is chosen (radio filled); the footer names it.

## 3. Last used

1. Choose a window, press Start recording, let the countdown finish, stop after a few seconds.
2. Open the sheet again on Window.
   Pass: a LAST USED group at the top holds that window, then FRONT TO BACK for the rest. It still
   shows after the title changes (switch the browser tab: the same app matches).
3. In a throw-away config (`CAPTUREMANN_CONFIG_DIR`) with no `app.window_picker.last_used` key
   there is no LAST USED group.

## 4. Choosing

1. Click a row.
   Pass: the radio fills with the record red; the footer shows `Title . App`; Start recording is
   enabled.
2. Choose the covered window.
   Pass: it can be chosen and the recording (step 7) shows that window's own content, not the one
   on top of it.
3. Choose, then press Display, then Window again.
   Pass: the choice was dropped by the switch (as before), the list reloads.

## 5. Pick on screen

1. Press "Pick on screen".
   Pass: the existing overlay hover mode opens, unchanged (windows highlight under the pointer).
2. Click a window; press Esc in another try.
   Pass: the sheet returns; the clicked window is chosen in the list (it scrolls into view and
   the footer names it); with Esc nothing changes.

## 6. Include chips

1. Below the list: "Include" with Webcam bubble, Microphone and System audio chips. A chip that is
   on has a check, one that is off an x.
   Pass: they start in the state the switches had in the last sheet (remembered settings).
2. Toggle each.
   Pass: Webcam shows the live preview bubble when on and removes it when off; turning the
   microphone or camera on for the first time asks for the permission (PRM-1.AC7).
3. With no camera connected or system audio unavailable the chip is dimmed and does not toggle,
   and the reason shows under the chips; a denied camera or microphone shows its note and Open
   System Settings button under the chips.
4. Record 5 s with a combination (webcam on, microphone on, system audio off), stop, and open the
   details page.
   Pass: the recording has exactly the tracks that were on (`capturemann get_capture` or the
   details page lists screen, camera, mic; no system track).

## 7. Privacy-excluded apps

1. With the app from Setup open, open the sheet on Window.
   Pass: none of that app's windows is a row; the last row of the list is a muted note, for
   example "Notes is hidden by your Privacy settings." (names are matched by the app's display
   name against the bundle id, best effort: an app whose name does not follow its bundle id is
   still listed, and the recording leaves it out anyway).
2. Remove the exclusion, reopen.
   Pass: the windows are back and the note is gone.

## 8. Keyboard only, VoiceOver on

1. Turn VoiceOver on (Cmd+F5). Open the sheet with the keyboard (your Record shortcut).
2. Tab to the Window tile and press Space; Tab through the sheet.
   Pass: the search box, Pick on screen, the three chips, Cancel and Start are reachable in a
   sensible order and each has a spoken name and state (chips: "switch, on/off").
3. With the focus on the sheet itself (click its empty header area), press Down, Down, Up.
   Pass: a cursor outline moves between rows; the list scrolls to keep it in view; it stops at
   the first and last row; Home and End jump.
4. Press Space (or Enter) on a row.
   Pass: it is chosen; VoiceOver reads the row as "Title, App, size, radio button, selected".
   Pressing Enter on the chosen row starts the recording (the countdown begins).
5. With the focus in the search box, Down and Up still move the cursor, Enter chooses.
   Pass: and typing a space in the box does not choose a row.

## 9. Other kinds unchanged

Display and Region look and behave as before (display chips or summary, Choose a region button,
the three source rows). Switching between the three kinds never leaves Cancel and Start out of
view, also on a short display.
