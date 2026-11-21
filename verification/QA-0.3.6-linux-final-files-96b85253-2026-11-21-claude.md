# QA 0.3.6 Linux x86_64 final files, commit 96b85253, PARTIAL

- **Date:** 2026-11-21. **Who:** Claude (QA agent), driven from the Mac. **Machine:** ThinkPad T490, Intel UHD 620, Ubuntu 24.04, GNOME 46, one display, X11 session.
- **Files (built on the laptop, `~/wt/release-out/96b85253-x86_64/`, not rebuilt):** `capturemann-0.3.6-linux-x86_64.tar.gz` sha256 `916b583e564322c24d058812b5213bc6835b15ebdb25c7bf09abafcf32e064eb` (152,027,811 bytes); `capturemann_0.3.6_amd64.deb` sha256 `ee7c201c5c906ae1410126fb193cca806bfb33a9ad714dc2ae69a2f61b9a3e5f` (113,787,344 bytes). Unsigned (the maintainer signs with minisign).
- **Sums:** recomputed on the laptop with `sha256sum`: both match the values above; both `.sha256` files and `SHA256SUMS.txt` contain exactly those two lines and `sha256sum -c` says OK. The five files were copied to the Mac (`/Users/trungtran/release-0.3.6/linux-x86_64/`) and `shasum -a 256 -c` there says OK for all; the `.deb` hash installed on the laptop is the same.
- **BUILD-INFO.txt:** commit `96b852537cbf9aebc62efe44477fee8f56fcb99d`, built 2026-11-21T13:13:16Z on thinkpadt490, Ubuntu 24.04.5, glibc 2.39, rustc 1.99.1, cmake 3.28.3, gcc 13.3.0, bundled FFmpeg and llama-server records (built 12:58 and 13:03 UTC).
- **Build logs** (`logs/*.log`, `~/wt/release-96b-x86_64.log`): no error lines. Two warning lines in `tarball.log`: `warning: variable does not need to be mutable` and ``warning: `whisper-rs-sys` (build script) generated 1 warning`` (the vendored whisper-rs-sys build script; no effect on the files). The main log has none. Earlier passes reported "no warning lines" for their builds; this one has these two.
- **`packaging/linux/check-package.sh`** (the script is unchanged between 96b85253 and the Mac HEAD; run on the laptop on both release files, full checks including ldd and program start): **PASS** for the tarball and **PASS** for the `.deb`. The build's own `check-*.log` files say the same.
- **Isolation:** private `CAPTUREMANN_LIBRARY/CONFIG_DIR/SOCKET`, `CAPTUREMANN_SECRETS=file` (`cm-regress` makes its own per case). Default library `~/Videos/CaptureMann` (72 listing lines: files, sizes, mtimes) and `~/.config/CaptureMann/settings.json` (sha256 `ea37a984...0191`) and the `~/Pictures` entry count (25): listed before to a file and again after, `diff` empty: **unchanged**.

## Regression (`cm-regress`, installed `.deb` 96b85253, X11, `--allow screen`, `--commit 96b85253`, the `~/lab/cm-regress` of 18 Nov, not rebuilt)

| Run | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard | 57 | 0 | 7 of 64 | `L1-linux-0.3.6-regression-standard-x11-final-96b85253-deb-2026-11-21-claude.md` (+ .json) |
| full | 64 | 0 | 16 of 80 | `L1-linux-0.3.6-regression-full-x11-final-96b85253-deb-2026-11-21-claude.md` (+ .json) |
| `linux.deb_install` (package purged first, programs from a `dpkg -x` copy, installer = the final `.deb`) | 1 | 0 | 0 | `L1-linux-0.3.6-regression-full-x11-final-96b85253-deb-install-2026-11-21-claude.md` (+ .json) |

Same counts as the 57f50732 and 103290cd passes (57, 64). No failure to explain. Skips: standard: `desktop.stays_up`, `desktop.second_launch`, `desktop.hotkey_overlay`, `desktop.no_console_flash`, `desktop.tray_icon`, `linux.hotkey_overlay`, `regress.ffmpeg_missing_wording`; full adds `ai.transcribe`, `ai.speakers` (no models), `installer.dmg`, `installer.silent` (other systems), `desktop.hud_click_through`, `desktop.hud_drag`, `desktop.recording_placement`, `linux.deb_install` and `linux.tarball_install` (they run in their own invocation / not in this one). One lab slip: my first `deb_install` attempt gave `--package-dir` with a literal `$HOME` that the wrapper did not expand; cm-regress exited 2 with no report; the second attempt with an absolute path passed. No report of the failed attempt exists.

## Quick Capture bar, X11, installed final `.deb`, private library/config (about 1 minute)

Opened with Ctrl+Alt+Shift+C (log: `quick_capture window ... opened`), then keys: 2 (Video), 3 (Audio), 1 (Screenshot): the kind changed each time; **Ctrl+1 picked the first preset ("Bug report" shown on the Capture button, kind Screenshot, Region) and the library was not raised (`studio brought to front` count 0 for the whole run)**; 2 right after Ctrl+1 changed the kind (`quick_capture.json` ended with `kind: video`); O collapsed and O expanded the bar; Esc closed it (`closed itself`). Same behaviour as the 103290cd recheck. Pictures were taken during the run and deleted with the test dir (not stored in `verification/`).

## Not done

Wayland, hardware tier, mic/camera/sound cases, Quick Capture on Wayland, N-3 to N-7 re-checks (not asked), the tarball was not installed (only checked by `check-package.sh`), no aarch64, no signing. 

## Cleanup

Removed: my `~/lab/rc96*`, the check-package copy, the `.deb` copies in `~/lab`, the `dpkg -x` folder, `~/lab/regress-out` (the wrapper), session.env and lock; no `cm-desktop`/`cm-daemon`/`cm-regress` processes left. Kept `~/wt` including `release-out`. Left on Ubuntu **X11** with the final 96b85253 `.deb` (`capturemann 0.3.6`) installed.
