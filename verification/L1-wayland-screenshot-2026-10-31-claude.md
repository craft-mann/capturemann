# L1 Wayland screenshot report

- **Date:** 2026-10-31
- **Who:** Claude (assistant), in a VM
- **Machine:** QEMU/HVF Ubuntu 24.04 aarch64, headless GNOME Shell 46 (Wayland, one 1280x720 virtual monitor), xdg-desktop-portal 1.18 with the GNOME backend; no GPU
- **Build:** debug, `cargo test` and the `cm-daemon` + `capturemann` binaries built with `--no-default-features` at the commit that adds this file
- **FFmpeg:** not used
- **Fixture:** none; the screenshots are of the VM's desktop. The Screenshot portal permission was granted beforehand through the portal permission store (the same state as clicking Allow once).

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| `cargo test -p cm-capture --test real_portal -- --ignored` (display size, blank check, three captures in a row, two tests at once) | PASS | one capture 0.2 s after the first, which takes about 5 s |
| `capturemann targets` | PASS | one display "Desktop" 1280x720, no windows |
| `capturemann capture screenshot --display 1` through `cm-daemon` | PASS | 1280x720 PNG in the library |
| `capturemann capture screenshot --region 100,100,400,300` | PASS | 400x300 PNG |
| `capturemann capture screenshot --window x` | PASS | `not_found`, exit 2 (a Wayland session lists no windows) |
| Permission not yet granted (no answer to the prompt) | not run | the code maps "Cancelled" and "Other" to `permission_missing(screen)`; the prompt was not exercised |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| CAP-1 display and region capture on Linux Wayland | PASS (this VM only) | steps 1-4 |
| CAP-1 window capture on Linux Wayland | not built | needs the ScreenCast portal's window source (user picks it) |
| Real GNOME on hardware, KDE, multi-monitor, fractional scale | not run | the portal returns one desktop picture; scale is reported as 1.0 |

## Problems found

- Two bugs found and fixed on the way: a runtime built per call made the second request hang; two requests at once get no answer (now one at a time). The daemon also refused to start on Linux.
- Monitors are not listed separately on Wayland (the portal gives one desktop picture).
