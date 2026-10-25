# CAP-5 manual script: delayed capture (toolbar choice, tray submenu, countdown pill)

Covers what the automated tests cannot: the pill and the capture toolbar seen by eye, a context
menu that stays open, and Esc. The delay choices, the remembered setting, the pill geometry and
digit, the Esc action and the daemon's 5.0 s timing (fake backend) are covered by
`cargo test -p cm-desktop -p cm-daemon` (CAP-5.AC1 timing, AC2 cancel). These steps capture your
screen: run them only when you mean to.

Privacy: the captures show whatever is on your screen. Use an empty desktop and a menu of a
plain app (Finder's right-click menu on the desktop).
Use the bundle (`make run-bundle`) when permissions matter.

1. **Settings choice.** Open Settings -> Capture & recording. "Before a screenshot" shows a "No delay / 3 s / 5 s / 10 s"
   control, with "No delay" selected. Choose 5 s. Open
   the tray menu -> Capture delay: "5 seconds" is checked. Choose 10 seconds in the tray: the
   setting now shows 10 s. Choose 5 s there again.
2. **Remembered.** Quit CaptureMann and start it again. The setting shows 5 s and the tray
   menu has "5 seconds" checked. `capturemann settings get capture.delay_s` prints 5. Set
   `capture.delay_s` to 7 in `settings.json` and restart: the app falls back to No delay.
3. **AC1: Display.** With 5 s: click Display in the sidebar dock (or press the Display shortcut), then
   right-click the desktop within the 5 s so a context menu is open. The pill never takes
   focus, so the menu must stay open. Expected: the pill (48 px disc, top right corner of the display
   under the pointer) counts 5, 4, 3, 2, 1 with a draining red ring; about 5 s after
   the start the toast appears. Open the capture: the context menu is in the image and the
   pill is not. Time it with a stopwatch: 5.0 s +/- 0.1 s.
4. **AC2: Esc.** Start a Display capture with 10 s and press Esc at 3 s (Esc is a global key
   only while the delay runs; if another app owns Esc, click the pill, then Esc). Expected:
   the pill closes, no capture, no toast, and the next capture works. Repeat for Window and
   Region.
5. **AC3: Region.** With 5 s: click Region in the sidebar dock, then open the context menu on the desktop
   during the countdown. Expected: when the pill closes the screen freezes and the picker opens
   showing the menu; drag a region over the menu and confirm the saved image contains it.
6. **Window mode.** With 3 s: click Window. The window picker (R19, see `CAP-WINDOW.md`) opens at
   once: choose a window and press Enter. Expected: the picker closes, the pill counts down 3 s
   (open a menu in that window now), then that window is captured with the menu in it; Esc on
   the pill cancels and nothing is saved. Then press Tab in the picker to use the pointer mode:
   the pill counts down first, then the overlay opens (the old flow).
7. **Two displays.** Start a delayed capture with the pointer on the second display: the pill
   shows in that display's top right corner.
8. **Reduced motion.** Turn on System Settings -> Accessibility -> Display -> Reduce motion:
   the ring stays full and only the digit changes.
9. **No delay.** Choose No delay: Display, Window and Region start at once, with no pill.

## Pass criteria

| AC | Steps | Pass when |
| --- | --- | --- |
| CAP-5.AC1 | 3 | the image is taken 5.0 s +/- 0.1 s after the start and the pill is not in it |
| CAP-5.AC2 | 4 | Esc during the delay captures nothing, for Display, Window and Region |
| CAP-5.AC3 | 5 | the menu that was open when the countdown ended is in the frozen image |
| settings page and tray share one remembered choice | 1, 2 | the same value in both, kept across a restart |

## Report

Copy the template in `verification/README.md` to `verification/CAP-5-<date>-<who>.md`, fill the
step and AC tables (macOS version, displays, the stopwatch reading for step 3) and commit it with
the change. Windows is not built: write "skipped" for it.
