# CAP-1 manual script: capture display, window, region (`cm-capture` library side)

Covers the parts of CAP-1 and CAP-5 that need a real display. The overlay-side ACs
(CAP-1.AC3, AC5, AC6, AC7) belong to apps/overlay and are not covered here.

## Setup
- macOS: grant Screen Recording to the terminal app that runs `cargo`
  (System Settings > Privacy & Security > Screen Recording), then restart the terminal.
- Windows (not run by the author): no permission needed. Test at 100% and 150% scaling.

## Automated-but-ignored tests

```
cargo test -p cm-capture --test real_display -- --ignored --nocapture --test-threads=1
```

| Test | AC | Pass when |
| --- | --- | --- |
| `cap1_ac1_real_display_matches_reported_physical_size` | CAP-1.AC1 | each printed `-> WxH` equals the display's physical size (e.g. 3024x1964 on a 14" MBP at 2x) |
| `cap1_ac2_real_region_has_requested_size` | CAP-1.AC2 | passes; open `$TMPDIR/cm-capture-region.png` and check it is 420x300 and shows the screen area at (100, 80) |
| `cap1_ac4_real_window_matches_bounds` | CAP-1.AC4 | printed window image size equals its bounds (within 1 px) and no shadow is included |

Without permission the tests print "Screen Recording permission missing" and return (that is
the `permission_missing(screen)` path; nothing panics).

## Manual checks
1. CAP-1.AC2 pixel-exact on a real screen: show a 1 px divider or a checkerboard image
   full-screen, capture a region that starts on the divider, open the PNG and confirm
   the divider is in column 0. Repeat on a 1x and a 2x display.
2. Permission: revoke Screen Recording, run the ignored test: it must print the permission
   message, not crash. Re-grant afterwards.
3. CAP-5.AC1: capture a display with `delay = 5` and a stopwatch; the frame is taken
   5.0 s +/- 0.1 s later (the library test `cap5_ac1_capture_happens_after_the_delay` checks
   the timing with a fake backend). In the app: tray > Capture delay > 5 seconds, then Capture
   display: the 48 px pill shows 5..1 in the top right corner and is not in the saved image.
   Repeat with Region: the picker opens when the pill ends and a menu opened during the delay
   is in the frozen image (CAP-5.AC3). Press Esc without clicking (and again after a click on the pill): nothing is captured, no
   toast, and the next capture works (CAP-5.AC2).
4. Two displays: `list_targets` shows both with distinct ids and non-overlapping bounds;
   capturing each gives its own size.
