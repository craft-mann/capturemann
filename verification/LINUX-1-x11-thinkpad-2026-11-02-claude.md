# LINUX-1 report: the early Linux build on a real X11 desktop (ThinkPad T490)

- **Date:** 2026-11-02
- **Who:** Claude (AI agent) drove the laptop from a Mac over ssh (`xdotool`, `scrot`, the CLI, D-Bus); the maintainer looked at the screen for the launcher and the toast
- **Machine:** Lenovo ThinkPad T490, Intel UHD Graphics 620 (WhiskeyLake), 31 GB RAM, one 1920x1080 display at scale 1, Ubuntu 24.04.5 LTS, GNOME Shell 46.0, kernel 7.0.0-42, **Xorg session**; real hardware (dual boot with Windows 11), not a VM
- **Build:** `capturemann_0.2.5_amd64.deb` from CI run 41366465816 (code of `fbd33d4`), installed with `apt-get install ./...` (6 new packages: `tesseract-ocr` and its data, which the `.deb` recommends); the tarball path of step 1 was not run
- **Status:** PARTIAL (X11 only; Wayland, steps below marked NOT RUN, and recording is off in this build)
- **Smoke reports:** `scripts/testlab/smoke-linux.sh` on Wayland (`smoke-wayland-20261102-134959.txt`) and on X11 (`smoke-x11-20261102-135621.txt`): no FAIL lines (VA-API H.264 encode, portal, keyring, tray watcher, notifications, 7 programs, bundled FFmpeg has VP9 and no libx264, CLI, MCP `tools/list` 75 names)

| Step | Result | Notes |
| --- | --- | --- |
| 3 diagnostics | PASS | `session X11`, portal ScreenCast v5 and Screenshot v2, PipeWire yes, Secret Service yes, VAAPI `/dev/dri/renderD128`, secrets in the system credential store, tesseract found; no macOS wording |
| 4 PipeWire stopped | INCONCLUSIVE | diagnostics still said PipeWire running after `systemctl --user stop ...` (socket activation restarted it); the "needs PipeWire" line was not seen |
| 5 first launch, one title bar | PASS | the window opened within the 6 s wait, system title bar only, `WM_CLASS dev.capturemann.app`, log has `desktop 0.2.5 started` |
| 10 display screenshot | PASS | 1920x1080 PNG in the library |
| 11 region screenshot | PASS | `Ctrl+Alt+Shift+1`, drag, Enter: saved 400x250, picker shows size, "1x native" and the magnifier |
| 12/14b window screenshot | PASS | `targets` lists `xclock` with title; `--window xclock` saved 164x164. `xterm` is not installed, so that half was not run |
| 15 clipboard | PASS (CLI) | `--copy`: `xclip` shows a 1920x1080 PNG. Paste into another app and the edited image: NOT RUN |
| 15a launcher hotkey | PASS | `Ctrl+Alt+Shift+Space` opens it, typing goes into the search box, the same key closes it (maintainer, real keyboard) |
| 15d second instance | PASS | `cm-desktop --library` and plain `cm-desktop` exit at once and show the running window |
| 17/18 recording is off | PASS | the launcher rows say "Not available yet"; `capturemann record start` answers `not_implemented`, nothing was created in the library. The dock and mic button toasts were not clicked |
| 19 tray menu | PASS | icon in the top bar (AppIndicator extension on), menu as specified, Pause and Stop greyed |
| 20 delay and recents | PARTIAL | over D-Bus the delay click stores `capture.delay_s` and moves the menu's toggle (ids 100 to 103, revision up), but the GNOME top bar still showed the check on "No delay" in a screenshot: stale display in the shell extension, or a missed click, to be checked by eye. "Recent captures" listed the one capture made from the app, not the three made with the CLI |
| 3b frame and controls on X11 | PARTIAL | one system title bar seen; minimise, maximise and close were not clicked |
| 6 editor (6a to 6f) | NOT RUN | needs a person; Vietnamese typing needs an IBus engine |
| 7 to 9 keyring and secrets | NOT RUN | |
| 13, 14, 14a, 15b, 15c, 16 Wayland | NOT RUN | |
| 21 to 23 tray activation, language, host restart | NOT RUN | |
| 1, 2 tarball install and uninstall | NOT RUN | |

## Problems found

- **Saved-screenshot toast cut off (fixed in `65a4af0`, not yet seen on the laptop):** the toast has action buttons, so it stays until dismissed, but its window was too short, so the title and the close button were hidden and it could not be closed. Cause: `estimate_height` counted the two shortcut hints (Paste, Open library with `Ctrl Alt Shift L`) as one row; macOS corrects the window to the painted height, Linux does not. The window was 440x206.
- **Stray daemons (fixed in `4c6a77e`):** `smoke-linux.sh` called a `daemon stop` command the CLI does not have and left its daemons running; a CLI call also starts a headless daemon that blocks the app's socket until it is stopped (the app then shows "Command line and agents are offline"). Not changed in the product.
- **Setup script (fixed in `0534952`):** `linux-setup.sh` stopped on a fresh Ubuntu (no `/etc/systemd/logind.conf.d`).
- The ThinkPad has Fn Lock on by default: Esc did not close the launcher until the maintainer toggled it. Not an app problem.

## Verdict

PARTIAL. X11: install from the `.deb`, diagnostics, screenshots (display, region, window), clipboard, launcher, tray menu and second-instance behave as specified. Not "verified": the editor, the keyring, the Wayland session and the tarball install have not been run, and the toast fix needs a look on the laptop.
