# L1 linux 0.3.2 regression report (full)

- **Date:** 2026-11-12T00:25:40Z to 2026-11-12T00:26:23Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5d493d08, package folder `/home/trung/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261112T002539Z --who cm-regress via regress.sh --verdict PARTIAL --only linux --installer /home/trung/lab/capturemann_0.3.2_amd64.deb --commit 5d493d08`, tier `full`
- **Result:** 7 passed, 0 failed, 2 skipped of 9 cases
- **SHA-256 of the files under test:**
  - `capturemann` `db817b5dfcf5b7287b0aec9b0bc4b778dd124718b09d85edbf8de502802a7a56`
  - `cm-mcp` `2c172c8774b2774d819b535d12d6fde8eda9c2d5cef677c84628d1b4c04d9ae1`
  - `cm-daemon` `48da124b29d14143715ec7c046f430d278b935bf12d861b1fbf1e82e2f488e5a`
  - `cm-desktop` `445fba43eba115f908b2fcd2a82004d1b599357a0e47c00ee6d2e6e55892bee1`
  - `cm-overlay` `a0875b5852ccfb3411a2e5f8d44db0349835818816cd8eb16132fc7c0d4b8273`
  - `ffmpeg` `6c0178f77641bb7a45d71bd599fd56313a2cb376c2617900ba4ecfe21a1e8bf4`
  - `ffprobe` `be61c3bfa8d0389de8a1f62c1369d05ba37a097847f360e321123c54a64b6972`
  - `llama-server` `83319f88df0c5ed4a02a2f26a8c90554f600de887aeeab4026c7bb9ac9f2dc20`
  - `capturemann_0.3.2_amd64.deb` `fe80b52b95a15a57cbcf35368ca0f12ffc0bccb077fae53a7635a1953cb1ba7f`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `linux.session` | PASS | the run sees a desktop session; the report names X11 or Wayland, the desktop and the portal |
| `linux.displays` | PASS | xrandr and the product list the same monitors |
| `linux.desktop_stays_up` | PASS | cm-desktop starts, stays up for 10 s and (X11) shows a window of its own |
| `linux.second_launch` | PASS | a second launch with a file forwards it to the running app and exits |
| `linux.tray_icon` | PASS | the tray icon registers as a StatusNotifierItem while the app runs |
| `linux.hotkey_overlay` | SKIP | the capture hotkey opens the overlay on every monitor: needs --allow screen |
| `linux.vaapi_encoder` | PASS | VA-API (vainfo) and the product's diagnostics agree on hardware or software encoding |
| `linux.deb_install` | PASS | the .deb installs, its programs find their libraries and run, remove and purge leave nothing |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: --installer is /home/trung/lab/capturemann_0.3.2_amd64.deb, this case wants a file ending in .tar.gz or .tgz |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| CAP-1 overlay | not run | `linux.hotkey_overlay` |
| CAP-3 | PASS | `linux.displays` |
| D-R2 software VP9 fallback | PASS | `linux.vaapi_encoder` |
| DIAG-1 | PASS | `linux.vaapi_encoder` |
| IMP-4: second launch opens Import | PASS | `linux.second_launch` |
| LINUX-1 | PASS | `linux.session`, `linux.displays`, `linux.desktop_stays_up` |
| PKG-deb | PASS | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| REC-1 | PASS | `linux.vaapi_encoder` |
| REC-5 tray | PASS | `linux.tray_icon` |
| bug class: crash at start | PASS | `linux.desktop_stays_up` |
| packaging/linux/build-deb.sh | PASS | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |
| platform-acceptance | PASS | `linux.session` |

## Problems found

- None in the cases that ran.
## Not run

- --installer is /home/trung/lab/capturemann_0.3.2_amd64.deb, this case wants a file ending in .tar.gz or .tgz (1): linux.tarball_install
- needs --allow screen (1): linux.hotkey_overlay

## Evidence of passing cases

- `linux.session` (329 ms): session x11 desktop ubuntu:GNOME runtime dir /run/user/1000; desktop portal answers on the session bus
- `linux.displays` (847 ms): xrandr: eDP-1 1920x1080 at (0,0) primary; daemon pid 47290 answered after 281 ms; product: display 65 'eDP-1' 1920x1080 at (0,0) scale 1; log hygiene: 1 WARN line(s) in the case's logs
- `linux.desktop_stays_up` (10945 ms): 1 visible window(s) of the app: hwnd 0x1e00001 pid 47343 class 'dev.capturemann.app' title 'CaptureMann' rect [402, 152, 1582, 912] ex 0x0
- `linux.second_launch` (6994 ms): second launch: exit 0 (0x00000000) after 31 ms
- `linux.tray_icon` (6932 ms): tray items: 3 before, 3 with the app; CaptureMann's: org.kde.StatusNotifierItem-47543-1
- `linux.vaapi_encoder` (954 ms): vainfo: VA-API hardware H.264 encode available (Intel iHD driver for Intel(R) Gen Graphics - 24.1.0 ()): hardware encoder; profiles: VAProfileMPEG2Simple, VAProfileMPEG2Main, VAProfileH264Main, VAProfileH264High, VAProfileJPEGBaseline, VAProfileH264ConstrainedBaseline, VAProfileVP8Version0_3, VAProfileHEVCMain, VAProfileHEVCMain10, VAProfileVP9Profile0, VAProfileVP9Profile2; daemon pid 47625 answered after 281 ms; diagnostics: - VAAPI render device /dev/dri/renderD128; log hygiene: 1 WARN line(s...
- `linux.deb_install` (11024 ms): /home/trung/lab/capturemann_0.3.2_amd64.deb version 0.3.2; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.2; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
