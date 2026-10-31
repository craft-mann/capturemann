# L1 desktop shell on Linux report

- **Date:** 2026-10-31
- **Who:** Claude (agent), nothing by a person; VM only
- **Machine:** QEMU VM on a Mac, Ubuntu 24.04 aarch64, 4 CPUs, software rendering (llvmpipe). X11: Xvfb :99 with openbox, 1920x1080. Wayland: headless GNOME session, 1280x720, desktop-portal Screenshot permission granted.
- **Build:** branch `worktree-agent-a6707f0ac0158741c` (see `git log`), debug, `cargo build -p cm-desktop --no-default-features` plus `-p cm-cli` and `-p cm-overlay`; run with `CAPTUREMANN_CONFIG_DIR` and `CAPTUREMANN_LIBRARY` in /tmp
- **FFmpeg:** not used
- **Fixture:** none (the VM desktop is the subject)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Build cm-desktop for Linux | PASS | needed GPUI `wayland`+`x11` features and ashpd `async-io` in cm-capture (GPUI turns on async-io; `tokio` and `async-io` cannot both be on) |
| 2 Start on X11 | PASS | window and permissions page shown (`shots/x11-start.png`); toast "Menu bar icon unavailable" |
| 3 Start on Wayland | PASS | window shown (`wayland-cli-display-2.png`) |
| 4 Screenshot on X11 with the CLI against the running app (`capturemann capture screenshot --no-start --region`) | PASS | capture in the library (`x11-gui-click.png` shows it) |
| 5 Screenshot on X11 through the GUI (Display button, cm-overlay, click) | PASS | `x11-overlay.png`, `x11-display-via-overlay.png`; "Copied to clipboard" toast shown; arboard logs "could not hand the clipboard contents over to the clipboard manager" (no manager in the VM) |
| 6 Screenshot on Wayland with the CLI (`--display 1`) | PASS | first call was taken while the portal's "Allow Applications to Take Screenshots?" dialog was up (`wayland-cli-display.png`); after Allow the shot is clean |
| 7 Screenshot on Wayland through the GUI overlay | not run | my own ScreenCast helper dialog was left open on top; retest |
| 8 Tray | not built | `tray-icon` on Linux panics without `gtk::init`; replaced by a clear error toast |
| 9 Global hotkeys | not run | `GlobalHotKeyManager::new()` result not examined on Wayland; GNOME 46 portal has no GlobalShortcuts |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| CAP-1 (display screenshot, X11) | PARTIAL | step 5 |
| CAP-1 (Wayland) | PARTIAL | step 6 (CLI path only) |
| CAP-2 tray / hotkeys | not met on Linux | steps 8, 9 |

## Problems found

- Tray: fixed to degrade (toast). A real tray needs GTK main loop plus a StatusNotifier host.
- The permissions page says "macOS keeps these under your control" on every OS (existing text, also wrong on Windows).
- "could not hide a recording window from capture: only built for macOS" is logged for recording-related windows; the toast/HUD windows will appear in captures on Linux.
- Wayland: the first portal screenshot triggers a one-time permission dialog from the shell; the daemon call waits on it.
- zbus warns "Failed to populate properties cache" for every portal request (harmless).
- Xvfb: GPUI logs "Found no xinput mouse pointers" (harmless), EGL DRI3 warnings (software rendering).
- Capture-hotkey path needs `cm-overlay` next to cm-desktop (it built on Linux unchanged).
- Clipboard on Wayland untested.

## Verdict

PARTIAL: the shell starts and takes screenshots on X11 and GNOME Wayland (Wayland through the CLI path); tray, hotkeys, Wayland overlay, clipboard on Wayland remain.
