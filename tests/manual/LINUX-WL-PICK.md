# LINUX-WL-PICK: window and area screenshot with the desktop's own picker (CAP-1.AC4, Wayland)

Where: a GNOME Wayland session (tested on GNOME 46, Ubuntu 24.04). Needs a person at the screen. Use a private library
(`CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET`).

1. `capturemann capture screenshot --pick`. GNOME's screenshot UI appears.
2. Choose a window (the window button of the UI), click the shutter. Expected: the command exits 0 and the library holds a
   capture the size of the window; the same picture is also in `~/Pictures/Screenshots` (GNOME saved it; the app does not delete it).
3. Run it again and press Esc. Expected: exit 3 (`user_declined`), nothing added to the library, no new file in `~/Pictures/Screenshots`.
4. Run it and wait 2 minutes without choosing. Expected: `confirmation_timeout`, the picker closes.
5. The app: Capture window in the tray or launcher on Wayland uses the same path.
6. Other desktops (KDE Plasma, GNOME 47 and newer, Ubuntu with another compositor): note whether the portal returns the file
   itself (then step 2 works without the fallback) and record the version in `verification/`.

Why the fallback exists: `xdg-desktop-portal-gnome` on GNOME 46 answers an interactive screenshot with response code 2 and the log line
`InteractiveScreenshot didn't return a file` although the shell saved the picture
(`verification/L1-linux-0.3.0-desktop-wayland-window-experiment-wayland-2026-11-08-claude.md`).
