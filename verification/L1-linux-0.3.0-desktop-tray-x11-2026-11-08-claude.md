# L1 report: Linux tray (StatusNotifierItem) on a real GNOME panel, X11

- **Date:** 2026-11-08
- **Who:** Claude (desktop agent), over ssh on the lab laptop; stills taken with `import` (X11) and driven with `xdotool` (lab lock held as `desktop`)
- **Machine:** Lenovo ThinkPad T490, Intel UHD 620 (i915), Ubuntu 24.04.5 LTS, kernel 7.0.0-42, GNOME Shell 46, **X11 session** (auto-login), extension `ubuntu-appindicators@ubuntu.com` ACTIVE, one 1920x1080 display
- **Build:** debug build of branch `track/linux-desktop` (code of `c256280c` and later), `cm-desktop` started with a private config, library and socket
- **Status:** PARTIAL (X11 only; the Wayland session and KDE were not run)
- **Scripts:** `scripts/testlab/desk-tray.sh` (D-Bus), `desk-panel.sh` (stills and clicks), `desk-sni.sh` (item properties and the shell journal)

| Step | Result | Notes |
| --- | --- | --- |
| Tray icon in the top bar | PASS | the CaptureMann icon is drawn left of the system icons (`...-shots/tray-icon-in-top-bar.png`); the item registers with `org.kde.StatusNotifierWatcher` |
| `WindowId` type | FIXED | gnome-shell logged `Received property WindowId with type u does not match expected type i in the expected interface` at every start; the property is now `i32` (unit test `cap2_window_id_is_an_int32_as_the_interface_says`); the line is gone from the journal afterwards |
| Menu opens on a click, items as the macOS menu | PASS | still taken; separators and disabled Pause/Stop as designed |
| Delay check mark, 0.2.6 bug "did not move" | PASS (moved) | clicked the icon, `Capture delay`, `3 seconds`, reopened: the check mark is on "3 seconds" and off "No delay" (`...-shots/delay-3s-checked.png`). Over D-Bus the change goes out as `ItemsPropertiesUpdated` (monitor `desk-tray.sh`), not as `LayoutUpdated`. I did not run the old build for comparison, so "this fixed it" is not shown, only that it works now |
| `AboutToShow` answering true | REJECTED | tried to make the extension re-read the layout when a menu opens: the extension then asked for the layout with only `type` and `children-display` and opened "Capture delay" empty (side by side with the answer `false`, same build). Reverted in `c256280c`; the code comment says why |
| Recent captures, 0.2.6 bug "only the app's own capture" | PASS (D-Bus level) | a screenshot made by the CLI against the app's library appears in the menu within 6 s (`desk-tray.sh`: the layout lists its title instead of "No captures yet"); the app now asks the library at start and every 10 s. Not seen in the panel as a still |
| Tray activation (left click) | NOT RUN separately | GNOME's extension opens the menu on a left click and never calls `Activate`; the `Activate` method itself is only tested over D-Bus by earlier reports |
| Wayland session, KDE | NOT RUN | |

## Not shown here

Language switch of the tray menu, the "light panel" icon, and the Wayland session.
