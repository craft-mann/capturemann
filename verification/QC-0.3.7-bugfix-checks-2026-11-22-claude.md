# QC 0.3.7 bug-fix checks report

- **Date:** 2026-11-22
- **Who:** Claude (lab laptop, scripted with xdotool on Ubuntu and a PowerShell probe on Windows)
- **Machine:** ThinkPad T490, Intel UHD 620. Ubuntu 24.04, GNOME 46 on Xorg; Windows 11 (1024x768 console session)
- **Build:** Ubuntu: commit 43ce8c1d, release, `--no-default-features`. Windows: commit b4542260, release, `--no-default-features`
- **Fixture:** none (isolated CAPTUREMANN_LIBRARY, CONFIG_DIR, SOCKET; test output deleted)

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Linux X11: drag the bar by its background | PASS | 8 pointer moves of (+25,+15) moved the bar from 529,540 to 729,660 (xwininfo). The move only took effect once the request's reply was awaited |
| Linux: Tab in the Edit preset sheet (N-7) | PASS | After two Tab presses the focus ring moved from Screenshot to Video (screenshots taken) |
| Windows: bar frame after open (F-4) | PASS | Opened at 159,400 882x107, unchanged after 2 s and after the picker opened and closed (before the fix: 397 then 400) |
| Windows: no "window not found" on close (F-8/K3) | PASS | No ERROR line in the log after open, picker, Esc, Esc (before: one ERROR line) |
| Windows: picker above the bar (F-3) | Observed, no change | The window grows upward to 159,15 882x492; its bottom edge stays at the bar's. By design; spec statement to be added |
| Windows: "RefCell already borrowed" on the recording bar (F-8) | not run | Needs a recording |

## Problems found

- None open from these steps. Not run: Wayland, the strip under the bar (K5), the recording bar.
