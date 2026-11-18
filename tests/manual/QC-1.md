# QC-1 manual script: Quick Capture (the panel, the after-capture bar, presets)

Covers what the automated tests cannot: the panel, the after-capture bar and the preset sheet seen
by eye, the keyboard route, the real pickers, a real capture, a real recording and the global
shortcuts. Everything else (the panel's state and keys, the run decisions, the folder with the
date, the Auto-redact step, Undo, the presets file and its shortcut conflicts, the layout of the
labels in English and Vietnamese) is covered by `cargo test -p cm-desktop --lib` and
`cargo test -p cm-daemon preset`. The record steps read your screen, microphone and camera once:
use a window you made up for this, keep the takes short and delete them afterwards.

Report each run in `verification/` (template in its README). Say "verified" only with a report.

Use the bundle (`make run-bundle-dev`) when permissions matter. To look at the panel without a
shortcut: `cm-desktop --demo-quick-capture`. Use a private settings folder when you try the
presets file: `CAPTUREMANN_CONFIG_DIR=/tmp/qc-config CAPTUREMANN_LIBRARY=/tmp/qc-lib`.

## 1. Opening the panel (QC-1 "Opening")

1. **Shortcut.** Press Option+Shift+C (macOS) or Ctrl+Alt+Shift+C (Windows, Linux). Expected: the panel
   opens near the pointer, on the display that has it, and the keyboard goes to it. Press the shortcut
   again: it closes. The Launcher shortcut (Option+Shift+Space) still opens the Launcher.
2. **Tray.** Open the tray or menu-bar menu. Expected: **Quick Capture…** is the first item, then
   Capture region, Capture window, Capture display. Choose it: the panel opens under the icon (macOS,
   Linux top bar) or above the taskbar (Windows). While a recording runs, Stop is the first item and
   Quick Capture… follows the Record group.
3. **Dock.** In the sidebar, under **Record…**, a neutral **Capture…** button with its keycap.
   Click it: the panel opens near the pointer. The five dock tiles and the Long Shot and Click Guide
   tiles work as before.
4. **Launcher.** Open the Launcher, type "quick". Expected: a **Quick Capture** row with a Preview label
   and its keycap, in the Capture group. Press Enter: the panel opens.
5. **Rebind.** Settings > Shortcuts > "Open Quick Capture" > Change. Press the Launcher's chord:
   refused with "Already used by Open launcher". Press a free chord: it works. Reset to defaults.
6. **Hotkey taken.** With another app holding Option+Shift+C: Settings > Shortcuts shows "Couldn't
   register: used by another app" on that row, and the tray item still opens the panel.

## 2. The panel (QC-1, QC-4)

7. **Layout.** Top to bottom: the header (mark, "Quick Capture", the Preview label, the settings button;
   under it the preset pill and Send feedback), the three kind tabs (only the active tab shows its name),
   the target row, four option tiles (2 by 2), the action bar. Corners round, the HUD surface, the keys
   inside the action bar. Red appears only on the Record bar (Video and Audio). It opens on the kind you
   used last.
8. **Kinds.** Press 1, 2, 3 (or click). Screenshot: Region, Window, Display, Long Shot; tiles Editor, Copy,
   Auto-redact, Delay; bar "Capture" (neutral). Video: Region, Window, Display; tiles Mic, System audio,
   Webcam, Countdown; bar "Record" (red). Audio: the line "Mic and system sound, no screen", no target
   row; tiles Mic, System audio, Transcribe, Markers; bar "Record" (red).
9. **Tiles.** Click a toggle tile: it fills its icon circle. Click Delay: a menu of Off, 3 s, 5 s, 10 s
   replaces the grid; pick 5 s. Close and open the panel: every tile is where you left it, per kind.
   **Check Settings > Capture & recording:** "Before a screenshot" still shows its old value, and
   Settings > General "After capture" is unchanged (QC-1.AC6). A capture from a dock tile follows
   Settings, not the panel.
10. **Keyboard.** Without the mouse: 1, 2, 3 switch the kind; Tab moves through the four tiles (a ring on
    the focused one); Up or Down toggles the focused tile or steps its value; Left and Right change the
    target; P opens the preset picker; Enter or Space captures; Esc closes (Esc in a menu or the picker
    goes back first). With VoiceOver or Narrator, every control has a name that includes its state
    ("Copy, on", "Delay, 5 s", "Region, selected").
11. **Not in the picture.** Take a Region screenshot from the panel with Copy on. Expected: the panel is
    gone before the picker opens and is not in the picture. Do it with Delay 3 s too: the pill counts
    down with no panel on screen.
12. **Preview.** The Preview label is in the header; hover it for its tooltip. **Send feedback** opens the
    browser on the feedback form with the feature "Quick Capture" and the app version only.

## 3. A capture from the panel (QC-1, QC-3)

13. **Region, Copy on.** Screenshot > Region, Copy on, Editor off. Enter. Expected: the panel closes, the
    region picker opens, you drag, the capture is copied (paste it), a toast "Copied to clipboard"
    with **Edit** and **Undo** stays for 6 seconds and stays while the pointer is over it. **Undo**: the
    capture disappears from the library. Do it again and press **Edit**: the editor opens.
14. **Bar.** Turn Copy and Editor off. Region again. Expected: no toast; the after-capture bar appears
    under the area with Edit E, Copy C, Save S, Text T, Long Shot L, Record R. Pressing each key does
    what it names: E opens the editor; C copies; S saves to the folder of Settings > General "Save a
    copy to a folder" or opens Save as; T copies the text it reads (needs the text reader, Linux: the
    `tesseract` program); L starts a Long Shot from that area; R starts a recording of exactly that area
    (the countdown, then the normal HUD).
15. **Dismiss.** Esc closes the bar and the capture is in the library. A click on another window closes
    it. With the pointer over the bar it never closes by itself; with the pointer away it closes after
    about 20 seconds.
16. **Edge.** Take a region at the bottom edge of the display: the bar is above the area and fully on the
    display. Do it on a second display, if there is one.
17. **Window and Display.** Window: the window list opens; pick one; the bar sits under that window.
    Display: the display under the pointer is captured. With Delay 3 s: the countdown pill shows and Esc
    cancels it.
18. **Auto-redact.** Show a made-up page with an email address. Turn Auto-redact and Copy on. Capture it.
    Expected: the email is blurred in the library capture and in the clipboard (paste it).
19. **Long Shot target.** Target Long Shot: the Long Shot picker opens (CAP-6). Not available while a
    recording runs.

## 4. Video and audio from the panel (QC-1.AC3)

20. **Video.** Video > Display, Mic on, Webcam on, Countdown 3 s. The red "Record" starts a recording
    after the countdown with the normal HUD and bubble. **Check Settings and the Record sheet:** their
    microphone, webcam and countdown choices are as before. Stop and delete the take.
21. **Video Region and Window.** The region or window picker opens (no screenshot is taken), then the
    countdown, then the HUD.
22. **Audio.** Audio > Mic on, Transcribe on, Markers on. "Record" starts an audio recording (no screen
    picker); a toast shows the marker shortcut. Stop after a few seconds: the transcript starts by itself.
23. **No permission.** With the microphone not allowed: the recording starts without it and says so, or
    the start error points at Permissions.

## 5. Presets (QC-4)

24. **Starters.** Open the picker (the pill, or P). Expected: No preset, Bug report, Docs shot, Demo video,
    Meeting, each with a one-line summary, then **Save current as preset…** and **Edit presets…**. Pick
    "Bug report": the kind, target and tiles fill, and the bar reads "Capture · Bug report". Pick "Demo
    video": the bar is red and reads "Record · Demo video". Switching the tab drops the preset.
25. **Save current.** Set up something, pick **Save current as preset…**, name it, Save. It is the picked
    preset, appears in the picker, and `presets.json` in the settings folder holds the same setup.
    A field you add by hand to that file (for example `"team": "docs"`) is still there after the next save.
26. **Settings > Capture & recording > Presets** (the top of the page; the panel's settings button and
    **Edit presets…** open it). Move a preset up and down; Edit opens the sheet; Delete asks for a second
    click ("Delete for good"). Delete all four starters, quit and start again: they do not come back.
27. **Edit sheet.** Name; Kind and Target; the toggles (Copy to clipboard, Auto-redact, Open in editor for
    a screenshot); "Then open the tool" (after Open in editor); Save to; Shortcut with **Change**;
    Save preset, Cancel, Delete. Make a preset with Save to `~/Bugs/{date}` and the tool "step".
28. **Save to.** Run that preset: the capture is in the library and as a PNG in `~/Bugs/<today's local date>`
    (the folder is made). With "Open in editor" the editor opens with the step tool selected.
29. **Shortcut.** Give a preset Ctrl+Alt+Shift+B (Option+Shift+B). Change, then press a chord another action
    uses (Option+Shift+1): refused with "Already used by Capture region". Press a chord another preset
    uses: "Already used by the preset …". Press the free one: it is kept. Now press it anywhere: the preset
    runs directly, the panel does not open. In Settings > Shortcuts, give an action that preset's chord:
    "Already used by the preset …". (On a Wayland session without the portal, preset shortcuts are not
    registered.)
30. **Launcher and tray.** The Launcher has a row "Preset: {name}" per preset; Enter runs it. The tray menu has
    a **Presets** submenu with the same names.
31. **Hand-edited file.** Break one preset in `presets.json` (for example a video preset with
    `"target": "long_shot"`). Restart. Expected: one notice names that preset, the others load.

## 6. Languages and layout

32. Set the language to Vietnamese. Open the panel, the bar, the sheet and the Settings presets page:
    nothing overlaps or is cut at the default width; the tile labels, the targets, the action bar and
    "Gửi phản hồi" fit. Then `CAPTUREMANN_PSEUDO_LOCALE=1`: report any clipped text.
33. Light and dark theme: the panel and the bar are dark in both (floating surfaces); the Settings page
    follows the theme.

## Pass criteria

All steps behave as written; QC-1.AC1 to AC8, QC-3.AC1 to AC7 and QC-4.AC1 to AC7 hold on the real
machine. Anything that does not is a finding for the report, with the platform and the step.
