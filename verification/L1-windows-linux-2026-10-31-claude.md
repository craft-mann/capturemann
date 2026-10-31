# L1 windows on Linux report (LINUX-1 section 3b)

- **Date:** 2026-10-31
- **Who:** Claude (agent), nothing by a person; VM only
- **Machine:** QEMU VM on a Mac, Ubuntu 24.04 aarch64, 4 CPUs, software rendering (no GPU). X11: Xvfb :99 with openbox, 1920x1080. Wayland: headless GNOME 46 shell (`cm-shell`, 1280x720 virtual monitor), Mutter RemoteDesktop for pointer input, Mutter ScreenCast + gst for frames. **The GNOME session was shared with other agents' apps** (their windows, a GNOME screenshot UI and their CLI calls appeared on the same screen); the shell itself had been OOM-killed before I started and I started the `cm-shell` unit again.
- **Build:** branch `worktree-agent-a944907aa71ca22d5`, debug, `cargo build -p cm-desktop -p cm-cli -p cm-overlay --no-default-features`; run with `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_LIBRARY` (and on Wayland `CAPTUREMANN_SOCKET`) under `/tmp/ag-win`
- **FFmpeg:** not used
- **Fixture:** none
- **Screenshots:** `L1-windows-linux-2026-10-31-shots/` (X11 frames with `import -window root`, Wayland frames from a Mutter ScreenCast stream)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 X11 (openbox): one title bar, window class | PASS | `x11-start.png`: only openbox's bar (the app draws none); `xwininfo` shows class `dev.capturemann.app`; the window icon is in the openbox title bar |
| 2 GNOME Wayland: the app draws a title bar | PASS | `wl-normal2.png`, `wl-banner.png`: logo, "CaptureMann", minimise, maximise, close; before this change the window had none |
| 3 Maximise/restore button | PASS | `wl-restore-click.png` (window 1180x760 after restore, glyph changes between square and restore) |
| 4 Drag the bar moves the window | PASS | `wl-moved.png` (moved +150 px right and 50 px up) |
| 5 Resize from the right edge | PASS | `wl-i.png` (width 1180 to 930, content re-laid out) |
| 6 Double-click the bar maximises | PASS | `wl-j.png` (maximised; a GNOME screenshot UI from another agent is also on screen) |
| 7 Minimise button | PASS | `wl-k.png` (the window is gone from the screen) |
| 8 Close button | PASS | the process exited (`pgrep` 1 before, 0 after); note: with no tray, closing the only window ends the app |
| 9 Right-click window menu, other edges and corners, keyboard use of the buttons | not tried | |
| 10 X11 pop-ups: toast above | PASS | `x11-start.png`: toast window at the bottom right as an override-redirect notification window (GPUI `WindowKind::PopUp`, `_NET_WM_WINDOW_TYPE_NOTIFICATION`) (source reading of gpui-linux: `override_redirect` and the notification window type; keyboard focus not tried) |
| 11 Wayland: toasts as cards in the main window | PASS | `wl-banner.png` (card bottom right inside the window, no separate toast window). The first attempt (banner only while the window is active) flapped because Wayland reported the window active and inactive within moments when the toast window came and went, so the rule is now "while the Studio window exists" |
| 12 X11 capture with a toast on screen | PASS | `x11-cloak-compare.png`: toast visible on screen, absent in the CLI screenshot, back on screen afterwards (pixel compare: restored frame identical to the first, captured frame differs by 45258 px) |
| 13 Wayland capture with a banner on screen | FAIL / inconclusive | log shows ask, hide and restore (`capture cloak: ...`), but the screenshot taken with the default 180 ms settle (`wl-cloak.png`, `wl-cloak2.png`) still contains the banner: the window probably does not repaint within the settle time on software rendering. A rerun with `CAPTUREMANN_CLOAK_SETTLE_MS=800` was spoiled because another agent's window was in front. Not resolved |
| 14 No "could not hide a ... window from capture" warning on Linux | PASS | `grep -ci "could not hide" x11.log` printed 0 while toast, launcher windows were created |
| 15 Unit tests and clippy | PASS | macOS and Linux: `cargo clippy -p cm-desktop --no-default-features --all-targets --locked -- -D warnings`; Linux: 18 tests in `csd::`, `capture_cloak::`, `platform_window::`, `placement_tests` |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| Linux window frame (specs/platform-acceptance.md, "Window frame") | PARTIAL | steps 1 to 8, 10, 11 on X11 and GNOME Wayland in a VM |
| CAP-1 own windows left out (Linux fallback) | PARTIAL | step 12 passes on X11, step 13 not on Wayland |

## Not tried

KDE Plasma (server-side decorations path is the same code as openbox: `window_decorations()` returns `Server`; not seen on KDE), fractional scaling, HiDPI, a real GPU, KDE Wayland, tiled windows (the edge strips follow GPUI's tiling state, only unit-tested), rounded corners and a shadow (not drawn; a 1 px border instead), always-on-top on Wayland (no protocol), the HUD, bubble and sheet windows (recording is not built on Linux, so they never open; the cloak API only hides the toast stack today), and the launcher keyboard focus under an override-redirect window on X11.

## Problems found

- The shared VM session: `pkill -x cm-desktop` also ends other agents' `cm-desktop`; their windows, GNOME's own screenshot UI and a shared default socket sat in my frames. Use a private `CAPTUREMANN_SOCKET`.
- `cm-shell` (headless GNOME) had been killed by the OOM killer; started again with `systemctl --user start cm-shell`.
- Hot corner: moving the pointer to the top-left corner opens the Activities overview; `/tmp/click.py` does that. My helper `ag-win-ptr.py` goes to the bottom-left instead.
- GPUI on Wayland reports the window's active state unreliably under this headless shell, so no behaviour depends on it.
- GNOME starts the headless session in the Activities overview; click the window thumbnail first.

## Verdict

PARTIAL: the frame, move, resize, maximise, minimise, close and the in-window toasts work on X11 and on GNOME Wayland in a VM; the capture fallback works on X11 and is not proven on Wayland.
