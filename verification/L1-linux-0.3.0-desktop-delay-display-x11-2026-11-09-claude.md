# L1 report: display capture with a delay on X11 (`desktop.settings_applied`)

- **Date:** 2026-11-09; ThinkPad T490, Intel UHD 620, Ubuntu 24.04.5, GNOME 46, X11 session, one 1920x1080 display; debug build of `track/linux-desktop` (main merged)
- **Cause (product bug):** the delay pill passes GPUI's display id to the daemon; GPUI counts displays from 0 while the engine's X11 id is 65 (`eDP-1`), so the daemon answered "Display 0 not found". Windows maps GPUI's monitor handles to the engine's ids by rectangle; Linux had no mapping. macOS ids agree.
- **Fix:** `ui/delay_pill.rs` now builds the rectangle list from GPUI's displays on Linux too (`platform_window::display_ids_differ_from_engine`), so `engine_display_id` finds the engine's id (primary display as the fallback). Unit test `a_zero_based_gpui_id_finds_the_engines_x11_id`. On a multi-monitor Linux desktop GPUI reports every display at origin (0,0), so the match falls back to the engine's primary display: NOT RUN with two monitors.
- **Status:** PARTIAL (X11, one display)

| Step | Result | Notes |
| --- | --- | --- |
| `cm-desktop --capture display` forwarded to a running app, `capture.delay_s=5` | PASS | capture saved 8.7 s after the request (5 s delay, pill, capture and copy), `scripts/testlab/desk-delay.sh` |
| tray menu "Capture display" (dbusmenu item 3) with delay 5 | PASS | 8.7 to 8.9 s; works twice in a row when the second request comes after the first has finished. A request during the first one's after-capture work (about 1.7 s) is ignored by the busy guard (by design, not changed) |
| `cm-regress --only desktop.settings_applied` | PASS | 12.9 s, same session |
| Global hotkey path on X11 | NOT RUN | the hotkey calls the same `start_capture(Display)` as the tray; no key injection was done |
