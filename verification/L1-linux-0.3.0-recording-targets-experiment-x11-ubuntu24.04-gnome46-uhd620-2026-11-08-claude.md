# Linux recording targets: experiment and triage of the rec.* failures (release 0.3.0, task 3 and 4)

- Date: Nov 8, 2026. Machine: lab ThinkPad T490, Ubuntu 24.04, GNOME Shell 46.0, **X11 session**, Intel UHD 620 (iHD VA-API), PipeWire.
- Build: branch `track/linux-recording` from main 45ef59b1 (debug build, `--no-default-features`).
- Wayland (GNOME 46) results: **not run yet** in this report (see "Not run").
- Scripts: `scripts/testlab/rec-mutter-exp.py`, `scripts/testlab/rec-live.sh`.

## 1. Triage of the 0.2.9 recording failures

| Case | Verdict | Cause |
| --- | --- | --- |
| `rec.screen_cursor`, `rec.mic` ("killed by a signal") | product design problem, shown by a lab limit | `record start` goes to the desktop portal, GNOME shows the "Share Screen" dialog (screenshot taken during the run) and waits for a click. Nobody clicks in the lab; the harness kills the CLI after its timeout, which is the signal. Same for `mcp.policy_allow` (its CLI start waits on the same dialog). An agent-started recording cannot work with a dialog. Fixed by not using the portal on GNOME (section 2). |
| `rec.kill9_recovery` ("portal refused screen capture (create session)") | same cause | A second portal request while the first dialog of the previous case was still open. Fixed by section 2. |
| `rec.region`, `rec.region_min` (`not_implemented`) | missing feature | Region recording did not exist. Built (section 2): exact size verified for 640x360 below. |
| `rec.screen_cursor` expects H.264 | wrong for the product code, right for the test | The test expects H.264. The product always used software VP9 although VA-API H.264 works on this laptop (`vainfo`). Fixed: VA-API first. The UHD 620 driver accepts only constant quality (CQP), not VBR (`Driver does not support any RC mode compatible with selected options (supported modes: CQP)`), which the first attempt hit; the probe now tries a bit rate, then CQP. |
| `rec.screen_cursor` expects a cursor track | missing feature | Linux has no cursor track yet (platform agent owns `cm-cursor`); recording says the pointer is in the picture. |

## 2. Experiment: which path gives a monitor, an area or a window

All run on the X11 session as a plain session-bus client (ssh into the desktop environment), no dialog attended.

| Path | Result |
| --- | --- |
| Portal ScreenCast (`xdg-desktop-portal-gnome`) | Shows the "Share Screen" dialog (monitor or window picker, "Remember this selection"); blocks until a click; cannot crop to a rectangle. Unusable for agents on the first run. |
| `org.gnome.Mutter.ScreenCast` (version 4) `RecordMonitor("")` | Session, stream and PipeWire node 60 with no dialog; one 1920x1080 frame read with `pipewiresrc`. |
| same, `RecordArea(100,100,400,300)` | No dialog; frame is exactly **400x300**: the compositor crops. This is the region path. |
| same, `RecordWindow(window-id)` | Needs Mutter's window id. `org.gnome.Shell.Introspect.GetWindows` answers `AccessDenied: GetWindows is not allowed`, so a program cannot learn the ids. Not usable. |
| Window on X11 | Window rectangle from the X server (existing window listing) recorded with `RecordArea`: the picture is the screen content of that rectangle (windows covering it appear in it). |
| Window on Wayland | No path without the portal's window picker (dialog, person needed) because window rectangles are not available. The product says so in the error and in `list_devices`. |

Decision: on GNOME (Wayland or X11) use Mutter's service for display and region (and X11 window); other desktops keep the portal and get a clear message for region and window. See `crates/cm-capture/src/sys/mutter_cast.rs`, `crates/cm-daemon/src/recording/sys/linux.rs`.
The GNOME panel shows its recording indicator (orange dot) while a Mutter session runs.

## 3. Live results after the change (X11, debug build)

- `record start --size 1080p --region 100,100,640,360`, 6 s, no dialog: `screen.mp4` is **h264 640x360 30 fps**, 6.13 s (VA-API, CQP).
- Display recording with software VP9 (before the CQP fix, VA-API had failed): 1280x720 vp9, plays; debug build reported `encoder_overloaded` (41 of 60 frames refused in the first 3 s): VP9 in a debug build is not representative.

## Not run

Wayland session; window recording on X11; mic, cursor, system audio; the 10-minute gate; kill -9 recovery timing; second monitor and scale 2 (the area is computed in logical pixels from the display's scale, tested with fakes only).
