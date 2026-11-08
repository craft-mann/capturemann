# L1 report: `capture screenshot --pick` with the shell-saved fallback, GNOME 46 Wayland

- **Date:** 2026-11-08; ThinkPad T490, Intel UHD 620, Ubuntu 24.04.5, GNOME Shell 46, Wayland session; debug build of `track/linux-desktop` (commit 9aa36f64); GNOME's UI answered by a virtual keyboard (`scripts/testlab/desk-wl-pick.sh`)
- **Status:** PARTIAL (area pick only; the window button of the UI and other desktops not run)

| Step | Result | Notes |
| --- | --- | --- |
| `--pick`, Return in GNOME's screenshot UI | PASS | exit 0, "Captured ... (480x270 px, region)", the picture is in the library; the shell's own copy stays in `~/Pictures/Screenshots` |
| `--pick`, Esc | PASS | exit 3 `user_declined`, nothing added, no new file |
| Window choice inside the UI | NOT RUN | |
