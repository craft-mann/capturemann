# L0 spike report: ScreenCast portal and PipeWire on GNOME Wayland (headless VM)

- **Date:** 2026-10-31
- **Who:** Claude (run for the maintainer; nobody watched the screen, the consent click was injected)
- **Machine:** a QEMU virtual machine on a MacBook Pro (Apple M4 Pro, Apple hypervisor), 4 vCPUs, 8 GB, **aarch64**. Ubuntu 24.04.5, kernel 6.8.0-147, GNOME Shell 46.0 started with `gnome-shell --headless --wayland --virtual-monitor 1280x720` (software rendering, no GPU, no physical display or seat), `xdg-desktop-portal` 1.18.4, `xdg-desktop-portal-gnome` 46.2, PipeWire 1.0.5. The portal services were started by hand as user units because there is no `gnome-session` (`graphical-session.target`).
- **Build:** `spikes/spike-portal-capture` (release), `ashpd` 0.13.13, `pipewire` 0.10.1; commit of the change that adds this file.
- **FFmpeg:** not used.
- **Fixture:** a Wayland client animating (`weston-simple-shm`) and the pointer moved in a circle through `org.gnome.Mutter.RemoteDesktop` (virtual input), so the screen changes about every frame.

This is **not** a real GNOME desktop on real hardware, it is an aarch64 machine, and the monitor is 1280x720 at 60 Hz. It says whether the portal flow and the stream work and how the stream behaves; it does not say anything about 4K, GPU buffers (DMA-BUF), hardware encoders or other desktops.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1. Start the headless session; list the portals | PASS | ScreenCast v5 (sources: monitor, window, virtual; cursor modes: hidden, embedded, metadata), RemoteDesktop v2, Screenshot v2, Clipboard v1, InputCapture v1, Settings v2, Background v2, Inhibit v3, Camera v1. **GlobalShortcuts: absent** (the frontend 1.18.4 does not offer it). Secret: absent (it is for sandboxed apps). |
| 2. First `Start` shows the consent dialog | PASS | The real "Share Screen" dialog of xdg-desktop-portal-gnome appeared inside the session (screenshot taken with `gnome-screenshot`). Keyboard input injected through Mutter did not reach it; a pointer click on **Share** did. The response held one stream: PipeWire node 47, source type monitor, position (0,0), size 1280x720, and a restore token (persist mode "until revoked"). |
| 3. Second run with the restore token | PASS | No dialog, same stream, first frame after 15 to 30 ms. |
| 4. Count frames for 10 s, cursor mode metadata | PASS | See the table below. |
| 5. Same stream without the portal (`org.gnome.Mutter.ScreenCast`, cursor-mode 1 and 2) | PASS | Same numbers; used to find why cursor metadata first did not arrive. |

## Measurements (portal path, restore token, 10 s)

| What | Value |
| --- | --- |
| Frames | 575 in 9.99 s = 57.6 fps (the virtual monitor is 60 Hz; the missing 4 % is the compositor's own cadence, not drops seen by the consumer) |
| Gap between frames | median 17.3 ms, p95 18.0, p99 18.8, max 20.4 |
| Format | `BGRx`, 1280x720, nominal frame rate reported as 0/1 (variable) |
| Presentation timestamps (`SPA_META_Header`) | present on 575 of 575 frames; the spread of (pts delta - arrival delta), p1 to p99, is 1.5 ms |
| Cursor metadata (`SPA_META_Cursor`) | present and valid on 575 of 575 frames; position follows the pointer; the 64x64 bitmap arrived once (shape change only) |
| Buffer type | CPU memory (the consumer received 2.1 GB in 10 s, i.e. about 212 MB/s of BGRx); no DMA-BUF was negotiated because the spike did not offer it |

## What had to be done to get each piece (for the real implementation)

1. **Timestamps**: the consumer must ask for the header meta (a `Meta` param), or the frames carry none.
2. **Cursor metadata**: the consumer must send `Buffers` and `Meta` params from `param_changed` once the format is known, and the cursor meta size must be a **range** (about 64x64 wanted, from 1x1 to 1024x1024), as other screen-cast consumers do. A single fixed size produced no cursor meta at all, silently (`update_params` returned Ok).
3. **Portal start without a desktop session** (only needed for this headless test): `XDG_CURRENT_DESKTOP=GNOME` in the user manager, and the two portal units started without their `graphical-session.target` requirement.

## Acceptance criteria

None of the acceptance criteria in `specs/features/` is claimed. This answers part of the L0 row of `specs/plans/release-0.3-0.4-platforms-plan.md` ("portal ScreenCast through PipeWire", "portal cursor metadata", "GlobalShortcuts portal") for GNOME 46 only.

## Problems found

- GlobalShortcuts is not in this portal version, so global hotkeys on GNOME 46 need another route (the plan already lists "shortcuts only through portals"); the launcher window and the tray extension are the fallbacks in the plan. To re-check on a newer GNOME (48 or later), where the interface exists.
- The consent dialog cannot be answered from the keyboard by injected input in this session; an automated test of the first-run flow needs pointer injection or a stored restore token.
- Not measured: 4K and 60 fps (the virtual monitor is 720p), DMA-BUF buffers, CPU cost with a real GPU, window and region sources, multiple monitors, KDE, X11, hardware encoders.

## Verdict

PARTIAL. The ScreenCast portal flow, a PipeWire video stream with usable timestamps and cursor metadata, and restore tokens all work on GNOME 46 Wayland in a VM. Not decided here: D-L1 to D-L5, any performance claim, KDE and X11.
