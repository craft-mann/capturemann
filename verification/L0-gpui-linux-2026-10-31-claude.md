# L0 spike report: GPUI on Linux (X11 and GNOME Wayland, VM)

- **Date:** 2026-10-31
- **Who:** Claude (run for the maintainer; nobody watched the screen, the screenshots are the evidence)
- **Machine:** the QEMU virtual machine of [`L0-portal-screencast-2026-10-31-claude.md`](L0-portal-screencast-2026-10-31-claude.md): aarch64 Ubuntu 24.04, 4 vCPUs, **software rendering** (Mesa `llvmpipe` 20.1.2 through Vulkan, no GPU). X11 was `Xvfb :99` 1920x1080 with `openbox`; Wayland was the headless GNOME Shell 46 session (virtual monitor 1280x720).
- **Build:** `spikes/spike-ui-gpui` (the S5 prototype: a library grid, an animated 640x360 CPU preview, a button with a key-cap label), `gpui-pre` and `gpui-pre-platform` `=0.3.9`, release, built on the VM in 1 m 12 s to 1 m 44 s.
- **Fixture:** none; the spike draws its own content.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. Build as the macOS spike is configured (`runtime_shaders`, `font-kit`) | PASS (builds) / FAIL (runs) | The program starts, prints `first_render_ms=1` and stays alive but **opens no window** on X11 or Wayland. The two features are macOS-only and enable no windowing backend. |
| 2. Add the Linux backends: `gpui-pre-platform` features `wayland` and `x11` (a Linux-only dependency table) | PASS | Both backends build. |
| 3. Run on X11 (`Xvfb` + `openbox`) | PASS | The window opens, framed by the window manager, with the library list, the 12 capture tiles, the button with its key cap, legible text and the preview animating; first render 126 ms. Screenshot: [`x11-xvfb-openbox.png`](L0-gpui-linux-2026-10-31-screenshots/x11-xvfb-openbox.png). Mesa warns that the X server has no DRI3 (expected on Xvfb) and renders in software. |
| 4. Run on GNOME Shell 46 Wayland (headless) | PASS | Same content, same text quality. Screenshot: [`wayland-gnome-shell-46.png`](L0-gpui-linux-2026-10-31-screenshots/wayland-gnome-shell-46.png). The window has **no title bar or borders** (GPUI draws no client-side decorations and GNOME's compositor adds none), and the window is larger than the 1280x720 monitor's free area. |

## What this answers (platform plan, L0 and risk 1)

- GPUI renders text and animated images on both Linux backends, under software Vulkan. That is the main fact the plan asked for ("GPUI on Wayland and X11"). It is **not** a statement about speed (software rendering at 1280x720 is not a measure), IME, accessibility, HiDPI or fractional scaling, or other compositors (KDE, Sway).
- `apps/desktop` declares only `runtime_shaders` and `font-kit` on `gpui-pre-platform`. On Linux it needs `wayland` and `x11` (target-specific), or it opens no window.
- The product needs its own title bar and window controls on Wayland.

## Acceptance criteria

None claimed.

## Problems found

- Not checked: Vietnamese input through IBus and fcitx5 (risk 9), the 1080p60 preview (risk 5), accessibility (NFR-13), fractional scaling, a real GPU.
- Under Wayland the spike's window opens larger than the screen; the product must size windows from the monitor.

## Verdict

PARTIAL. Go for "GPUI draws a window on X11 and GNOME Wayland" with the two backend features; everything else about GPUI on Linux is still open.
