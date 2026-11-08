# L1 report: window screenshot on Wayland, GNOME 46 (experiment before building)

- **Date:** 2026-11-08
- **Who:** Claude (desktop agent) over ssh; the desktop's own dialogs were answered by a virtual input device (`scripts/testlab/desk-uinput.py`, /dev/uinput as root), lab lock held as `desktop`
- **Machine:** Lenovo ThinkPad T490, Intel UHD 620 (i915), Ubuntu 24.04.5 LTS, GNOME Shell 46, **Wayland session** (switched from X11 for this block: AccountsService `Session=ubuntu`, `XSession=ubuntu`, `systemctl restart gdm3`; `ubuntu-wayland` did not work), one 1920x1080 display, scale 1
- **Portals:** ScreenCast v5 (`AvailableSourceTypes` 7: monitor, window, virtual), Screenshot v2, no GlobalShortcuts
- **Scripts:** `scripts/testlab/desk-wl-portal.py`, `desk-uinput.py`, `desk-bg.sh`
- **Status:** PARTIAL (one machine, one GNOME version; KDE and other GNOME versions not run)

## What was tried

| Experiment | Result |
| --- | --- |
| A. Screenshot portal `interactive=true` (what `capture screenshot --pick` uses), pressing Enter in GNOME's screenshot UI | the shell saved `~/Pictures/Screenshots/Screenshot from 2026-11-08 21-12-14.png` (480x270, the area the UI proposed) but the portal answered **response code 2** after 6.9 s with no results; `xdg-desktop-portal` logged `InteractiveScreenshot didn't return a file`. Reproduces the Nov 2 finding |
| A2. Same, pressing Esc in the UI | response code 2 as well and **no file was saved** (the folder is unchanged). So after code 2, "a new PNG appeared in the Screenshots folder" tells the two cases apart: image taken versus cancelled |
| B. ScreenCast portal, `types=window`, one frame | works. GNOME shows its own "Share Screen" chooser listing the open windows (a Files window was listed; an empty list when no window is open); choosing one and Share gives a PipeWire node (`source_type` 2) and a frame through `pipewiresrc` in 1 s. The stream reports 1920x1080 (the whole monitor size) and the frame is that size with the window drawn at the top left (Files window: opaque area 974x634 at 19,19 with its shadow) and the rest transparent. The window's true size therefore has to come from the buffer's crop meta (`SPA_META_VideoCrop`) or the alpha edge; a static window delivers one buffer only (do not wait for more) |
| C. A CaptureMann window in the chooser | the Studio window of the test run did not appear in the chooser and not in a still of the desktop (log: "studio window opened"); it was started detached from ssh. Not investigated further: it may be an artefact of starting the app that way; to check by hand with a person at the screen |

## Choice

1. **Primary: the shell's own file.** Keep `--pick` (the portal's interactive screenshot). When the portal answers code 2 but a new PNG (mtime after the request started, size > 0) appears in `<Pictures>/Screenshots` within about 2 seconds, use it: the person chose the area, window or screen and GNOME saved it. No new file means the person cancelled: "declined". The file stays where the shell put it (it is the person's own screenshot).
2. **Not chosen for now: the ScreenCast window source.** It works and gives windows by name, but needs the crop meta to cut the window out, a second dialog, and a stream the recording agent is also building for window recording. Left as the later route if the shell's folder cannot be found.
3. Where neither works (no `Pictures/Screenshots`, another compositor) the error says: window screenshots on Wayland need GNOME 46 or newer with the Screenshots folder, otherwise capture the display or a region.
