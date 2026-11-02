# LINUX-1 report: the early Linux build on a real Wayland desktop (ThinkPad T490)

- **Date:** 2026-11-02
- **Who:** Claude (AI agent) drove the laptop over ssh (CLI, D-Bus, `dbus-monitor`, journal); the maintainer used the screen for the region overlay and the desktop's own picker
- **Machine:** Lenovo ThinkPad T490, Intel UHD 620, one 1920x1080 display at scale 1, Ubuntu 24.04.5 LTS, GNOME Shell 46.0, **Wayland session** (`gdm-autologin`, session type `wayland`); real hardware, not a VM
- **Build:** `capturemann_0.2.5_amd64.deb` from CI run 41404587881 (code of `65a4af0`)
- **Status:** PARTIAL (Wayland steps that need a person at the screen are listed as not run)
- **Smoke report:** `smoke-wayland-20261102-152710.txt`: no FAIL lines

| Step | Result | Notes |
| --- | --- | --- |
| 3 diagnostics | PASS | `session Wayland`, portal ScreenCast v5 and Screenshot v2, PipeWire, Secret Service, VAAPI, tesseract; no macOS wording |
| 5a own title bar on GNOME Wayland | PASS | in a display capture the main window has the app's own bar: logo, "CaptureMann", minimise, maximise, close |
| 13 display screenshot | PASS | `capture screenshot --display 1`: 1920x1080, not blank (mean 0.19, stddev 0.23 in grey). No dialog appeared (permission was already granted in the earlier session); `targets` lists one "Built-in display" at scale 1.0. Declining the dialog: NOT RUN |
| 14 region screenshot (`cm-desktop --capture region`) | PASS | the overlay covered the screen dimmed and frozen (maintainer); the saved image is 732x568, the selected region. Esc: NOT RUN |
| 14a `capture screenshot --window x` | PASS | `not_implemented`, names `--pick`; `targets` says the desktop does not list windows |
| 14a `capture screenshot --pick`, choosing a window | **FAIL (desktop)** | GNOME's chooser took the picture (two files in `~/Pictures/Screenshots`) but the portal answered with response code 2 and logged `xdg-desktop-portal: InteractiveScreenshot didn't return a file`; the CLI reports `user_declined` and exits 3. Nothing is saved in the library. The picture the desktop took is not handed to the app |
| 15 clipboard | PASS | `--copy`: `wl-paste --type image/png` shows a 1920x1080 PNG; pasted with Ctrl+V into a text box in Firefox the picture appeared (maintainer, a later Wayland session with the build of CI run 41422283917). The app logs `Tried to initialize the wayland data control protocol clipboard, but failed. Falling back to the X11 clipboard protocol`: GNOME has no data-control protocol, so the copy goes through XWayland |
| 15c no GlobalShortcuts portal | PASS | the log says `no GlobalShortcuts portal`; Settings -> Shortcuts (seen in a display capture): every row says "Not available on this desktop" and a note says global shortcuts need X11 or the GlobalShortcuts portal and tells to add a custom shortcut in the desktop's keyboard settings that runs `cm-desktop --launcher` or `cm-desktop --capture region`; `cm-desktop --launcher` shows the launcher in the running app. The start-up toast: NOT SEEN. Odd: the Recording rows and the Change buttons are shown although recording is off in this build |
| 15d second instance | PASS | exits at once, shows the running app |
| 17/18 recording is off | PASS | `record start` answers `not_implemented`, nothing created |
| 16 portal memory (asks again?) | NOT RUN | no dialog appeared because permission existed |
| 5b/5c window controls on GNOME Wayland (move by the bar, double-click maximise, resize by an edge) | PASS (as reported by the maintainer) | |
| 19 tray menu on Wayland | PASS (as reported by the maintainer) | the menu opens and Open launcher works |
| 6 editor, 7 to 9 keyring | see the X11 report | session independent |

## Problems found

- **`--pick` does not work on GNOME 46 of Ubuntu 24.04.5:** the portal frontend does not return the file the shell saved. This is the desktop's behaviour, not an app crash, but until it is solved "Screenshot window" on Wayland has no working path (the window list is not available either). Known-gap list and release notes should say so. Ideas for later: after a failed pick, look for the newest file in `~/Pictures/Screenshots` (fragile), or capture the screen through the ScreenCast portal's window source.
- The CLI error text "closed, or the desktop's picker failed" fits; the code `user_declined` is misleading when the portal fails (code 2) as opposed to being cancelled (code 1).

## Verdict

PARTIAL. Wayland display and region screenshots, the clipboard (copy and paste into another application), the diagnostics and the window-title bar work on this machine. Picking a window does not.
