# QC-1 manual script: Quick Capture (the bar, the after-capture bar, presets)

Covers what the automated tests cannot: the Quick Capture bar, the after-capture bar and the preset sheet seen
by eye, the keyboard route, the real pickers, a real capture, a real recording and the global
shortcuts. Everything else (the bar's state, keys and tooltip texts, its size and the two-row rule, the placement, the run decisions, the folder with the
date, the Auto-redact step, Undo, the presets file and its shortcut conflicts, the layout of the
labels in English and Vietnamese) is covered by `cargo test -p cm-desktop --lib` and
`cargo test -p cm-daemon preset`. The record steps read your screen, microphone and camera once:
use a window you made up for this, keep the takes short and delete them afterwards.

Report each run in `verification/` (template in its README). Say "verified" only with a report.

Use the bundle (`make run-bundle-dev`) when permissions matter. To look at the bar without a
shortcut: `cm-desktop --demo-quick-capture`. Use a private settings folder when you try the
presets file: `CAPTUREMANN_CONFIG_DIR=/tmp/qc-config CAPTUREMANN_LIBRARY=/tmp/qc-lib`.

## 1. Opening the bar (QC-1 "Opening")

1. **Shortcut.** Press Option+Shift+C (macOS) or Ctrl+Alt+Shift+C (Windows, Linux). Expected: the panel
   opens near the pointer, on the display that has it (move the pointer to a corner and press it again:
   the panel follows the pointer; on Wayland, which gives no position, it opens centred on the display), and
   the keyboard goes to it (on X11, focus another window first: keys 1, 2, 3 must not reach it). Press the shortcut
   again: it closes. The Launcher shortcut (Option+Shift+Space) still opens the Launcher.
2. **Tray.** Open the tray or menu-bar menu. Expected: **Quick Capture…** is the first item, then
   Capture region, Capture window, Capture display. Choose it: the panel is centred on the tray icon (under the
   macOS status item, above the Windows taskbar icon) and fully on the display; on Linux, where the
   icon has no position, it opens near the pointer (QC-1.AC11, AC12). While a recording runs, Stop is the first item and
   Quick Capture… follows the Record group.
3. **Dock.** In the sidebar, under **Record…**, a neutral **Capture…** button with its keycap.
   Click it: the panel opens near the pointer. The five dock tiles and the Long Shot and Click Guide
   tiles work as before.
4. **Launcher.** Open the Launcher, type "quick". Expected: a **Quick Capture** row with a Preview label
   and its keycap, in the Capture group. Press Enter: the panel opens.
5. **Rebind.** Settings > Shortcuts > "Open Quick Capture" > Change. Press the Launcher's chord:
   refused with "Already used by Open launcher". Press a free chord: it works. Reset to defaults.
6. **Rebind needs the keys.** On X11 the chord you press while Change waits must reach the page even when
   the app itself holds it as a global shortcut (QC-1.AC15; the recorder must show the refusal, not open the Launcher).
6b. **Hotkey taken.** With another app holding Option+Shift+C: Settings > Shortcuts shows "Couldn't
   register: used by another app" on that row, and the tray item still opens the panel.
5. **The bar comes back (AC26).** Open the bar, choose Display and press Enter. Expected: the bar steps
   aside, the capture is taken (the bar is not in it), and about 1.5 s after the after-capture bar closes (or the
   toast is gone) the bar is open again at the same place with the same choices. Repeat with Region and Esc in the
   area picker, with a Video run (the bar returns after Stop) and with Long Shot. Press Esc on the bar itself: it stays closed.

## 2. The bar (QC-1, QC-4)

7. **Layout.** One row, 60 px high, about 870 px wide, rounded corners, the HUD surface. Left to right:
   the mark with a small **Preview** tag under it, the three kinds (1 2 3), the targets (R W D L), four
   options, the preset (P, with a menu badge), the action (Capture or Record, with the Enter key),
   and the expand/collapse toggle (a chevron with the key O under it). There is no Settings button and no
   Send feedback button. **No button has a text label**: each shows an 18 px icon with its key in small mono type
   under it. The selected kind and target are a raised chip inside a darker track; an option that is on has
   a lighter fill and a dot under the icon; Delay and Countdown show their value ("3s") as a badge in the
   corner, or a chevron when off. Red appears only on the Record button (Video and Audio). It opens on the
   kind you used last.
8. **Kinds, and the same width.** Press 1, 2, 3 (or click). Screenshot: Region, Window, Display, Long Shot;
   options Editor, Copy, Auto-redact, Delay; "Capture" (neutral). Video: Region, Window, Display; options
   Mic, System audio, Webcam, Countdown; "Record" (red, with a dark dot). Audio: no targets, the slot
   shows the microphone meter (unlit: the bar does not open the microphone); options Mic, System audio,
   Transcribe, Markers; "Record" (red). **The bar keeps the same width and every button stays where it
   is** when you switch between the three kinds. Take a screenshot of each kind and compare (the bar is
   hidden from captures: use `CM_DEMO_CAPTURABLE=1` for a look, or a phone photo).
9. **Options.** Click a toggle option: it gets the lighter fill and the dot. Click Delay: a small menu
   of Off, 3 s, 5 s, 10 s opens under the bar; pick 5 s (the badge reads "5s"). Close and open the bar:
   every option is where you left it, per kind. **Check Settings > Capture & recording:** "Before a
   screenshot" still shows its old value, and Settings > General "After capture" is unchanged (QC-1.AC6).
   A capture from a dock tile follows Settings, not the bar.
10. **Keyboard.** Without the mouse: 1, 2, 3 switch the kind; R, W, D, L choose the target (L does nothing
    on Video and Audio; R does nothing on Audio); on Screenshot E, C, A toggle Editor, Copy, Auto-redact and
    T opens the Delay menu; on Video M, S, B toggle Mic, System audio, Webcam and T opens the Countdown menu;
    on Audio M, S, N, K toggle Mic, System audio, Transcribe, Markers; Left and Right cycle the target; P
    opens the preset picker; O collapses or expands the bar; Enter or Space captures or records; Cmd+, and
    Ctrl+, do nothing; Esc closes (Esc in a menu or the picker goes back first). Hold Cmd, Ctrl or Alt with a letter
    (Cmd+C, Ctrl+R, Alt+E): nothing happens in the bar. Tab and Shift+Tab move a visible ring through the
    controls left to right; Up or Down on a focused Delay or Countdown steps its value. In the picker
    (QC-1.AC9): Up and Down move the ring and stop at the first and the last row (No preset at the top,
    Edit presets… at the bottom), Enter or Space chooses the highlighted row, Esc closes the picker only and
    the bar stays; the highlighted row scrolls into view when the list is long.
10b. **Ctrl+1 to Ctrl+4** (QC-1.AC17). With the bar open and the picker closed, press Ctrl+2: the second
    preset is picked and the action shows its name on a second line. Open the picker and press Ctrl+3:
    the third preset is picked and the picker closes. Each of the first four rows shows its key (Ctrl+1 to
    Ctrl+4; Control with the digit on macOS).
10c. **The name field owns the keys.** Open the picker, choose **Save current as preset…**, type "reddit
    caps 1 w" (letters, digits, spaces). Expected: the field holds exactly that, the kind, target and
    options did not change, Enter saves, Esc goes back.
10d. **Accessible names.** With VoiceOver or Narrator each button reads the same text as its tooltip: its
    name, its state and its key ("Copy · on · key C", "Countdown · 3 s · key T", "Region · selected · key
    R"); on, off and selected are announced as the control's state.
10e. **Tooltips** (QC-1.AC8, AC19). Rest the pointer on a button: after about 0.4 s a tooltip appears under
    it with the name, the state and the bar key; move away: it goes. Press Tab to a button and wait:
    the same tooltip appears for the focused button. On a kind (1 2 3) it adds "anywhere" and the global
    shortcut (Screenshot Option+Shift+1, Video Option+Shift+R, Audio Option+Shift+V on macOS; the Ctrl+Alt+Shift
    forms elsewhere). Rebind one of them in Settings > Shortcuts: the tooltip shows the new chord. The
    tooltip is never cut at the edge of the bar, also at the ends of the row.
11. **Not in the picture.** Take a Region screenshot from the bar with Copy on. Expected: the bar is
    gone before the picker opens and is not in the picture. Do it with Delay 3 s too: the pill counts
    down with no bar on screen.
11b. **Wayland.** A portal screenshot or a recording started by another program while the bar is
    open can show the bar: there is no way to exclude a window there (spec, Window behaviour). Check
    that the bar closes before a capture started from it.
12. **Preview.** The small Preview tag under the mark: hover it for its tooltip. The bar has no Send
    feedback button (QC-1.AC8); it is in About.
12b. **Two rows** (QC-1.AC1). On a display narrower than 900 points (or a window-manager scaled mode, for
    example a 1280 x 720 display at 150 %), open the bar: two rows, kind and target on top, then options,
    preset, action and the toggle; all keys and tooltips work as in one row, nothing is cut
    at the display's edge. On a wide display: one row.

12c. **Collapse and expand** (QC-1.AC21 to AC25). Click the chevron at the right end (or press O): the
    bar becomes the mark with the Preview tag, 1 2 3, P, the action and the toggle, in one row, with its left
    edge where it was. The tooltip of the chevron reads "Collapse · key O" before and "Expand · key O" after;
    with a screen reader it is announced as expanded or collapsed. Collapsed: R, W, D, L, E, C, A, T, M, S,
    B, N, K, Left and Right do nothing; 1, 2, 3, P, Ctrl+1 to Ctrl+4, Enter, Space, Esc and O work; Tab walks
    only the visible controls. Switch the kind in both states: the collapsed bar has the same width for
    Screenshot, Video and Audio. Open the Delay menu, then press O: the menu closes. Open the picker (P),
    then click the chevron: the picker stays under the bar. Put the bar near the right edge of the display and
    expand it: it moves left just enough to be fully on the display. On a display narrower than 900 points
    the collapsed bar is one row. Close the bar while collapsed and open it again (also after quitting the
    app): it opens collapsed. Delete `collapsed` from `quick_capture.json`: it opens expanded. Settings is
    unchanged by all of this.
- Drag the bar by its background (between the groups, or the logo): it follows the pointer and stays open; pressing and dragging on a button does not move it, and a click on a button still works. (QC-1.AC20)
- On a dark and a light wallpaper the bar and the open picker are solid with a visible edge line; nothing behind shows through.

- Click on another window or on the desktop: the bar stays open. Esc (with the bar focused), the shortcut or the tray item closes it. (QC-1 Window behaviour)

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
22. **Audio.** Audio > Mic on, Transcribe on, Markers on. "Record" starts an audio recording at once, with
    no countdown even when Settings has one (QC-1.AC14; no screen picker); a toast shows the marker shortcut. Stop after a few seconds: the transcript starts by itself.
23. **No permission.** With the microphone not allowed: the recording starts without it and says so, or
    the start error points at Permissions.

## 5. Presets (QC-4)

24. **Starters.** Open the picker (the pill, or P). Expected: No preset, Bug report, Docs shot, Demo video,
    Meeting, each with a one-line summary, then **Save current as preset…** and **Edit presets…**. Pick
    "Bug report": the kind, target and options fill, and the bar reads "Capture · Bug report". Pick "Demo
    video": the bar is red and reads "Record · Demo video". Switching the tab drops the preset. Pick "Docs shot", close the panel, open `quick_capture.json`: it still
    holds the options you had before (QC-1.AC13); choose No preset: the options are back as they were.
25. **Save current.** Set up something, pick **Save current as preset…**, name it, Save. It is the picked
    preset, appears in the picker, and `presets.json` in the settings folder holds the same setup.
    A field you add by hand to that file (for example `"team": "docs"`) is still there after the next save.
26. **Settings > Capture & recording > Presets** (the top of the page; **Edit presets…** in the
    picker opens it). Move a preset up and down; Edit opens the sheet; Delete asks for a second
    click ("Delete for good"). Delete all four starters, quit and start again: they do not come back.
27. **Edit sheet.** Name (it shows the preset's name when you edit an existing one; on X11 click it and type); Kind and Target; the toggles (Copy to clipboard, Auto-redact, Open in editor for
    a screenshot); "Then open the tool" (after Open in editor); Save to; Shortcut with **Change**;
    Save preset, Cancel, Delete. Make a preset with Save to `~/Bugs/{date}` and the tool "step".
    On a 1080p display at 150 % scale (1280 by 720 logical): the title and the footer stay in place,
    the footer never covers a field, the body scrolls with a scrollbar on its right edge, and the
    Save to and Shortcut rows are reached by scrolling and clicked (QC-4.AC9).
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

## 5b. The picker's popover (QC-4.AC8, QC-1.AC16)

34. Open the picker with the four starters: a popover opens under the bar, aligned to its right end, tall
    enough to show every row (a preset row is about 55 px), "Meeting" and its summary are fully visible, no
    row is cut off. The bar does not move. Add presets until the list is taller than the room on the display:
    the popover stops at the room (less the edge gaps), the list scrolls, and moving the highlight with
    Down scrolls it into view. Close the picker (Esc): the window returns to its size at rest.
    Check it on the 720 px display too.
34b. **Near the bottom edge.** Open the bar from the Windows taskbar icon or a tray icon at the bottom of
    the display (or move the pointer near the bottom edge on X11 and press the shortcut). Open the
    picker: on macOS and Windows the popover opens above the bar and the bar stays where it was; on Linux
    the popover takes the room under the bar and scrolls. It is fully on the display either way. Do the same
    with Delay's menu and the name screen.
35. Pick "Save current as preset…": the name screen opens as the popover and fits it; Esc goes back to the
    picker and then to the bar.

## 6. Languages and layout

32. Set the language to Vietnamese. Open the Quick Capture bar, the after-capture bar, the sheet and the Settings presets page:
    nothing overlaps or is cut; the Preview tag, the action's word and the preset name under it, the
    popover rows and the tooltips fit (the bar has no other text). Then `CAPTUREMANN_PSEUDO_LOCALE=1`: report any clipped text.
33. Light and dark theme: the Quick Capture bar and the after-capture bar are dark in both (floating surfaces); the Settings page
    follows the theme.

## Pass criteria

All steps behave as written; QC-1.AC1 to AC25, QC-3.AC1 to AC7 and QC-4.AC1 to AC9 hold on the real
machine. Anything that does not is a finding for the report, with the platform and the step.
