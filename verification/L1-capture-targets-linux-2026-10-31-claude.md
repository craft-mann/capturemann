# L1 capture targets report: window and region screenshots on Linux Wayland and X11

- **Date:** 2026-10-31
- **Who:** Claude (assistant), in a VM
- **Machine:** QEMU/HVF Ubuntu 24.04 aarch64, no GPU. Wayland: headless GNOME Shell 46 with one 1280x720 virtual monitor, xdg-desktop-portal 1.18 with the GNOME backend, the Screenshot permission granted beforehand. X11: `Xvfb :99` 1920x1080 with `openbox`.
- **Build:** debug, `--no-default-features`, the commit that adds this file (`cm-daemon`, `capturemann`, `cm-overlay`, the ignored tests of `cm-capture`).
- **FFmpeg:** not used. **Fixture:** none; the Wayland desktop was empty apart from another agent's CaptureMann window; the X11 tests open their own windows through the X protocol.
- **Input:** pointer and key events were injected through Mutter's RemoteDesktop (`/tmp/ag-win-ptr.py`, `/tmp/key.py`).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| `cargo clippy -D warnings` (cm-capture, cm-overlay, cm-daemon, all targets) on Linux and macOS | PASS | |
| `cargo test` of `cm-capture` (125), `cm-overlay` (93 + 3), `cm-daemon --test capture_pick` (5) on Linux; the same plus `cm-cli`, `cm-mcp` on macOS | PASS | scripted portal, scripted layouts, fake picker |
| Wayland `capturemann targets` | PASS | one display, name from Mutter ("MetaVendor"), 1280x720, scale 1, no windows, `capabilities {list_windows: false, capture_window: false, capture_region: true, pick: true}` |
| Wayland `capture screenshot --display 1` | PASS | 1280x720 |
| Wayland `capture screenshot --region 100,100,400,300` | PASS | 400x300 |
| Wayland `capture screenshot --window x` | PASS | `not_implemented`, exit 1, names `--pick` |
| Wayland `capture screenshot --pick`, picker answered with Esc | PASS (cancel path only) | the portal's dialog appeared and Esc gave `user_declined`, exit 3 |
| Wayland `capture screenshot --pick`, picker answered with Return | FAIL to produce an image | the headless GNOME picker logs "InteractiveScreenshot didn't return a file" and the portal answers "Other"; no image could be chosen in this VM, so the success path of the portal's picker was **not** exercised. The daemon path from a returned image to a library capture is tested with a scripted picker only |
| Wayland `cm-overlay region`, drag 400x300 with `--confirm-on-release` | PASS | one fullscreen window 1280x720 over display 1 showing the frozen, dimmed picture with the selection and magnifier (screenshot inspected); stdout `{"kind":"region","width":400,"height":300,"rect":[200,150,400,300],"pixel_identical":true}`, exit 0; first frame painted 121 ms after start |
| Wayland `cm-overlay region`, Esc | PASS | after one click the window had keyboard focus (`Focused(true)`), Esc exited 3 with `{"cancelled": true}`. Earlier runs where Return did not finish the pick are not explained (an injected Return once arrived as Escape; treated as a helper quirk) |
| Wayland `cm-overlay window` | PASS (to the picker) | goes to the portal picker through the daemon `pick` target; same limits as the `--pick` rows |
| `cargo test --test real_portal -- --ignored` on Wayland (6 tests) | PASS | display size, regions, two captures in a row, non-overlap, window id `not_implemented` (the picker test skips without `CM_PORTAL_PICK=1`) |
| X11 `real_display -- --ignored` (4 tests) | PASS | the earlier xterm window shows its title and captures at its bounds |
| X11 `real_x11_window -- --ignored` (2 tests, new) | PASS | a window with `WM_NAME` as `STRING` "Café window" (xcap listed it with an empty title) is found by `find_window("café window")`, captured at 320x200 with its own colour; `_NET_WM_NAME` UTF-8 wins; two windows are told apart |
| Multi-monitor and fractional scale on a real session | not run | the VM has one monitor at scale 1; the mapping is unit-tested only |
| KDE Plasma, wlroots | not run | other compositors report one "Desktop" display |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| CAP-1 display and region capture, Linux Wayland | PASS (this VM only) | rows above |
| CAP-1 window capture, Linux Wayland | PARTIAL | only through the compositor's picker (`pick`); cancel path seen, success path not seen on a real picker |
| CAP-1 window capture, Linux X11 | PASS (Xvfb + openbox) | `real_x11_window`, `real_display` |
| CAP-1.AC2 region = crop of the frozen picture on Wayland | PASS | `pixel_identical: true` |
| CAP-3 picker overlay, Linux | PARTIAL | region mode on Wayland and X11 code path; no window highlight on Wayland; mixed DPI untested |
| Mixed DPI, negative origins, fractional scale mapping | unit tests only | `crates/cm-capture/src/layout.rs`, `sys/mutter.rs`, `sys/portal.rs` tests |

## Problems found

- `xdg-desktop-portal-gnome` was in the failed state at the start (the earlier dialogs of other runs); `systemctl --user restart cm-portal-gnome cm-portal` fixed it. While the portal's picker is open every other Screenshot request waits behind it.
- GNOME answers an interactive screenshot that ends without an image (Esc, or a failure) with response "Other", not "Cancelled"; both are `user_declined` now.
- xcap leaves the title of classic X11 clients empty (WM_NAME as STRING); fixed in `sys/x11_names.rs`. xcap's `current_monitor` also compares `y` with the monitor's `x` (upstream bug, not touched).
- The overlay needed a click before it got keyboard focus on this Mutter; Wayland offers no way to take focus without an activation token. The desktop app launching it from a shortcut should pass `XDG_ACTIVATION_TOKEN` (not done: that code is in another agent's area).
- A mixed-DPI Wayland desktop makes each monitor's picture larger than the monitor; the overlay pairs them by order (`match_monitors`), untested on real hardware.
