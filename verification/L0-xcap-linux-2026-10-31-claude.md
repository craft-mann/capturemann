# L0 spike report: `xcap` screenshots on Linux (X11 and GNOME Wayland, VM)

- **Date:** 2026-10-31
- **Who:** Claude (run for the maintainer)
- **Machine:** the QEMU virtual machine of [`L0-portal-screencast-2026-10-31-claude.md`](L0-portal-screencast-2026-10-31-claude.md) (aarch64 Ubuntu 24.04, software rendering). X11 was `Xvfb :99` at 1920x1080x24, scale 1.0, with `openbox` as the window manager and `xclock` and `xeyes` open; Wayland was the headless GNOME Shell 46 session of the other report.
- **Build:** `spikes/spike-shot` (the S1 spike, unchanged), release, `xcap` 0.9, commit of the change that adds this file.
- **Fixture:** none beyond the two small X11 clients.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. X11, no window manager: capture the display | PASS | 1 display, 1920x1080, image size equals logical size x scale, 7.8 ms cold, 6.9 ms warm; region 400x300 in 2.3 ms |
| 2. X11, no window manager: list windows | FAIL (expected) | `Window::all failed: _NET_CLIENT_LIST_STACKING not supported`. A window manager that sets it is required; that is what every real X11 desktop has |
| 3. X11 with `openbox`: display and region | PASS | 10.8 ms cold, 6.9 ms warm for 1920x1080; region 400x300 in 2.8 ms |
| 4. X11 with `openbox`: windows | INCONCLUSIVE | 2 windows enumerated, none captured: the spike skips a window when `is_minimized()` errors (it defaults to "minimized"), which the two bare test clients seem to trigger. Window capture on X11 is not shown to work or fail |
| 5. GNOME Wayland: `xcap` | FAIL | `Monitor::all` fails ("error during parsing display string" without `XDG_SESSION_TYPE=wayland`; "Cannot find required wayland protocol" with it). `xcap`'s Wayland path needs a protocol GNOME's Mutter does not offer, so on GNOME the screenshot must go through the portal (Screenshot v2, or the ScreenCast stream of the other report) |

## Acceptance criteria

None claimed. This is input to L1 ("Portal screenshot and X11 path") of the platform plan.

## Problems found

- X11 window capture needs a real check with a normal desktop's windows (and the minimized-window rule of CAP-1).
- Wayland needs the portal path in `cm-capture`; `xcap` alone does not cover GNOME. KDE and wlroots compositors (Sway, Hyprland) were not tried.
- The numbers are for a small software-rendered virtual screen; they say nothing about 4K.

## Verdict

PARTIAL. X11 display and region capture work and are fast at 1080p; X11 windows are not yet shown; GNOME Wayland needs the portal.
