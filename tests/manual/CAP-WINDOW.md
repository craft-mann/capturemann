# CAP-WINDOW manual script: the window picker (board R19, screenshot of a window)

Covers what the automated tests cannot: the palette, the outline on the real window, focus and
keys seen by eye, two displays and VoiceOver. The ordering, search, filters, rail counts,
Cmd+N, keyboard navigation and the Enter / Esc / Tab outcomes are covered by
`cargo test -p cm-desktop window_picker`; the thumbnail queue (three at a time, no repeats) by
`cargo test -p cm-desktop window_picker::thumbs`; the hand-over flag and result of the overlay
by `cargo test -p cm-overlay -p cm-desktop overlay`. These steps capture your screen: run them
only when you mean to. No run is recorded yet; put a report in `verification/` (template in its
README) and only then call the picker verified.

Privacy: the captures and the thumbnails show whatever is in your windows. Use plain windows
(Finder, Safari on a blank page, Notes with a test note).
Use the bundle (`make run-bundle`) when permissions matter. Screen Recording permission is needed
(without it the picker shows the usual "Capture didn't start" toast with the Permissions buttons).

Setup: open 20 or more windows: five or six Finder windows (several named `Documents`,
`Downloads`, `Docs`), six Safari windows (one titled "Docs: Q4 launch plan" if you can, one
fully covered by other windows), Terminal x2, Notes, Preview. Keep one Finder window minimized
(it must not appear in the picker: minimized windows are not listed on macOS today; the board's
"Minimized" card is a `TODO(design)`).

1. **Open.** Press the Window shortcut (default Option+Shift+2), or click Window in the sidebar
   dock. Expected: a 1000 x 740 palette centred on the display under the pointer (smaller on a
   small display, never off screen); the search field has the caret; the list is front to back
   with the window you used last first and selected; CaptureMann's own windows are not listed;
   the count reads "22 windows" (your number). CaptureMann is the active app while the palette
   is open (it needs the keyboard); focus goes back to the previous app on close (steps 9 and 12).
2. **Thumbnails.** Cards show a neutral tile with a window glyph, then the thumbnails fill in a
   few at a time. Scroll a long list: only cards near the visible rows load. A window that
   cannot be captured keeps the neutral tile and nothing else breaks.
3. **Outline.** The selected window is outlined on the real screen: a 3 px light border with a
   soft ring and a small light chip with its title above it (inside its top right corner when the
   window touches the top of the display). Move the selection with the arrows: the outline
   follows after a moment, also onto another display. A window fully covered by others is
   outlined at its place (the outline sits above the covering windows).
4. **Search.** Type `doc`. Expected: only titles or apps that match remain (Docs, Documents, a
   Safari "Docs: ..." window); the count reads "4 of 22 windows" (your numbers), the rail counts
   shrink to match, the toolbar says "Best matches first, then front to back", and the dashed
   note appears. Titles that start with `doc` come before ones that only contain it. Type a word
   with no match: "No window matches ..." with the hint; Enter does nothing. Clear the field
   (select all with Cmd+A, Backspace): the whole list returns. Type without accents a window
   whose title has them (`tai lieu` finds `Tài liệu`).
5. **Rail and Cmd+N.** With the search empty, click an app in the rail: only its windows, the
   row is highlighted. Press Cmd+2: the second app of the rail is chosen (Cmd+2 again goes back to
   all windows; Cmd+0 shows all apps). Cmd+9 with fewer than nine apps does nothing.
6. **Display filter (two displays).** With a built-in and an external display, "All displays /
   Built-in / External" appears beside the count. Choose External: only windows on it; the rail
   counts follow. With one display the control is not shown.
7. **Layout.** The grid / list toggle switches between three cards per row and one row per window;
   the selected window stays selected and scrolled into view.
8. **Keys.** Arrows move the selection (no wrap; Up on the top row and Left on the first card stay
   put; Down from a row above a shorter last row lands on the last card); Home and End jump to the
   first and last window. The list scrolls to keep the selection visible. Hover also selects.
9. **Capture.** Select a window and press Enter (or the "Capture window" button, or double-click a
   card). Expected: the palette and the outline are gone at once, the previous app is active
   again, and a screenshot of exactly that window (also a covered one) is saved: the usual
   toast, the clipboard copy and the folder copy when those settings are on. Open it: the
   palette, the outline and its title chip are NOT in the image. Open the picker again: that
   window is first and selected.
10. **Tab hand-over.** Open the picker, press Tab (or "Pick on screen"). Expected: the palette
    closes and the old hover mode opens (frozen displays, a hint bar that now also says
    "Tab  Pick from list"). Press Tab again: the overlay closes without a capture and the list
    opens again. Click a window in the hover mode: captured as before. Esc in the hover mode:
    cancelled, no capture, no toast. With a capture delay set (Settings -> Capture & recording),
    Tab shows the delay pill before the hover mode, like the Window shortcut used to.
11. **Toggle.** Press the Window shortcut while the picker is open: it closes. Press it while
    another capture is running (the hover mode open): nothing happens.
12. **Esc.** Esc closes the palette and the outline, nothing is captured, the previous app has
    focus again.
13. **Click away.** Click another app's window while the picker is open: the picker and outline
    close without capturing.
14. **A window closes while the picker is open.** The picker closes when it loses focus, so close
    the window from a script: open the picker with a Notes window selected, then run
    `osascript -e 'tell application "Notes" to close window 1'` from another terminal that you
    started beforehand with a delay (`sleep 5; osascript ...`), and press Enter after it ran.
    Expected: the picker closes and the error toast "Capture failed" with
    the daemon's reason appears; nothing is saved; the next capture works.
15. **Keyboard only with VoiceOver on.** Turn VoiceOver on (Cmd+F5). Open the picker with the
    shortcut: VoiceOver announces "Capture a window" and the search field. Arrow keys move through
    the cards and VoiceOver reads each as "Title, App, 1680 by 1050 pixels, on Built-in"; the rail
    rows read "App, 3 windows"; the grid / list and display controls read as radio buttons with
    their state; Tab order inside the palette reaches Pick on screen and Capture window. Enter
    captures.
16. **Vietnamese.** Settings -> General -> Language: Tiếng Việt. Open the picker: every label is
    Vietnamese (a draft, awaiting review), nothing is cut off in the header, rail, footer or the
    empty states; long window titles end with an ellipsis. Switch back to English.
17. **No daemon / no permission.** With Screen Recording permission removed, the Window shortcut
    shows the "Capture didn't start" toast with "Open Permissions..." and no palette.

Pass: every expected result above. Note anything that differs from the board
(board R19, kit 4, removed from the repo): the board has a dimmed backdrop over the
display, a "Minimized" card and a "hidden by Privacy" note on the rail; none of them is built
(screenshots ignore the privacy exclusion list by design; minimized windows are not listed).
