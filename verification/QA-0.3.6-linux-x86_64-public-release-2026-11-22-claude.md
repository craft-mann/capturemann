# QA 0.3.6 Linux x86_64, public release files and APT repository (2026-11-22), PARTIAL

Machine: ThinkPad T490 (Intel UHD 620), Ubuntu 24.04, GNOME 46, X11 session, one display. Driven from the Mac. Everything downloaded on the laptop from the public URLs. Who: Claude (QA agent).

## Files and hashes (https://github.com/craft-mann/capturemann/releases/download/v0.3.6/)
- capturemann-0.3.6-linux-x86_64.tar.gz 916b583e564322c24d058812b5213bc6835b15ebdb25c7bf09abafcf32e064eb (152,027,811 bytes)
- capturemann_0.3.6_amd64.deb ee7c201c5c906ae1410126fb193cca806bfb33a9ad714dc2ae69a2f61b9a3e5f (113,787,344 bytes)
- the two .minisig and SHA256SUMS.txt (8 lines). `sha256sum -c` on the two x86_64 lines: both OK, equal to the expected values (and to the 96b85253 build). The copy that apt downloaded (/var/cache/apt/archives/capturemann_0.3.6_amd64.deb) has the same sha256 as the direct download.
- minisign `-Vm <file> -P RWTeQv8o...Rxc2k`: both "Signature and comment signature verified", rc 0; trusted comments "CaptureMann Linux: capturemann-0.3.6-linux-x86_64.tar.gz" and "CaptureMann Linux: capturemann_0.3.6_amd64.deb". minisign was already installed.

## APT (steps of docs/development/linux-apt.md with the real URL)
- Before: `ii capturemann 0.3.6 amd64` (the 96b85253 `dpkg -i`). `apt remove` -> state `rc` (config residue); no keyring or capturemann.sources existed.
- Keyring and sources downloaded with curl -fsSL. `capturemann.sources`: Types deb; URIs https://github.com/craft-mann/capturemann/releases/download/apt/; Suites ./; Architectures amd64 arm64; Signed-By /usr/share/keyrings/capturemann-archive-keyring.gpg. `gpg --show-keys`: 516CCD4FC7B351678803F71BDEB1BB54FAEF557D, rsa4096, expires 2031-11-08, "CaptureMann APT Repository <apt@craftmann.app>" (the fingerprint in linux-apt.md; not compared with the release-page text).
- `apt update`: `Get:12 https://github.com/craft-mann/capturemann/releases/download/apt ./ Packages [3,264 B]`; no W:/E: lines. Later updates: `Hit:4 ... ./ InRelease`.
- `apt install capturemann` (universe is enabled on this laptop; Recommends installed): "Setting up capturemann (0.3.6)"; `capturemann --version` = `capturemann 0.3.6`; `dpkg -l`: `ii capturemann 0.3.6 amd64`.
- `apt list -a capturemann`: 0.3.6, 0.3.5, 0.3.4, 0.3.3, 0.3.2, 0.3.1, 0.3.0 for amd64 (all seven); the same seven also listed for arm64 although the laptop has no foreign architecture (`dpkg --print-foreign-architectures` empty); the arm64 rows are cosmetic (apt list shows packages of the Architectures: line) and not installable here.
- Signature protection: keyring moved away, `apt update` -> `Err:4 ... ./ InRelease  The following signatures couldn't be verified because the public key is not available: NO_PUBKEY DEB1BB54FAEF557D`, then `W: An error occurred during the signature verification. The repository is not updated and the previous index files will be used.`, `W: Failed to fetch ... InRelease`, `W: Some index files failed to download. They have been ignored, or old ones used instead.` apt exit code 0 (usual apt behaviour for a warning). Keyring put back (sha256 d543ecce...8b79), next update `Hit:4 ... InRelease`, clean. The no-cached-index first-run refusal was not run (cached index existed).
- Upgrade path: `apt install --allow-downgrades capturemann=0.3.5` -> "Setting up capturemann (0.3.5)", `capturemann --version` 0.3.5; `apt update` clean; `apt upgrade` -> "Unpacking capturemann (0.3.6) over (0.3.5)", "Setting up capturemann (0.3.6)", version 0.3.6. NOTE: `apt upgrade` also upgraded 67 other Ubuntu packages on the laptop (68 upgraded in all), a side effect of the requested command, not a product issue. After the upgrade a private-dir `cm-daemon` started ("daemon 0.3.6 started"), `capturemann --no-start library list` -> "No captures.", `settings get` -> "No settings are set."
- No apt error or dependency problem at any point.

## Installed app on X11 (private CAPTUREMANN_LIBRARY/CONFIG_DIR/SOCKET, SECRETS=file)
`cm-desktop` started (library window v0.3.6 opened), Ctrl+Alt+Shift+C opened the bar (log: `quick_capture window ... opened (2 live)`); keys sent with xdotool: 2, 1, Ctrl+1, 2, O, O, Esc. Picture after Ctrl+1 then 2: bar shows Video selected, Region target icon active (the first starter preset "Bug report" is kind screenshot / region; `quick_capture.json` ended `kind: video`, `shot_target: region`, collapsed false). O collapsed/expanded and Esc closed the bar (`closed itself`). No "studio brought to front" line after the bar opened (grep for it was empty). Four starter presets present (`starters_seeded`). CLI `capturemann --no-start capture screenshot --region 100,100,400,300` -> "Captured shot_06H0315B13RNBJTT (400x300 px, region)" and listed in the private library. App quit; no cm-desktop/cm-daemon process left; app log has 9 lines, 0 panic/crash/ERROR matches. Pictures deleted with the test dir. Not stored.
Wayland: not run (switching the session was not done; X11 only).

## cm-regress standard, X11, the APT-installed package (`--package-dir /opt/capturemann/lib/capturemann`, `--allow screen`, `--commit v0.3.6`)
Binary: ~/lab/cm-regress (built 18 Nov / 96b85253, 15,464,720 bytes, not rebuilt). `git diff --stat 96b85253 v0.3.6 -- crates apps` differs in only 3 Windows-only files in apps/desktop (63 lines: platform_win_monitors.rs, platform_window.rs, and an `if cfg!(windows)` hunk in ui/quick_window.rs); the shipped Linux files equal the 96b85253 build, so the regression binary and the product are consistent.
Result: **57 passed, 0 failed, 7 skipped of 64** (exit 0). Reports: `L1-linux-0.3.6-public-regression-standard-x11-apt-2026-11-22-claude.md` and `.json`. Skips: desktop.stays_up, desktop.second_launch, desktop.hotkey_overlay, desktop.no_console_flash, desktop.tray_icon (not implemented for Linux), linux.hotkey_overlay (no key injection by the suite), and `regress.ffmpeg_missing_wording`.
- `regress.ffmpeg_missing_wording`: SKIPPED on this laptop (it has a system FFmpeg in a standard folder that the suite cannot hide), so the known failure (Linux text naming Homebrew, seen on the aarch64 clean root) does not show here; the wording issue is still there in the source (crates/cm-media/src/tools.rs NOT_FOUND_MESSAGE_UNIX) and was not re-run with FFmpeg removed.

## ISSUES
1. (low, product, known, 0.3.7) "FFmpeg was not found" text on Linux names Homebrew; not reproducible on this laptop (case skipped here), reproduced on aarch64 clean root.
2. (cosmetic, repository) `apt list -a capturemann` on amd64 also lists the arm64 versions (the sources file says `Architectures: amd64 arm64`); harmless.
3. (info) linux-apt.md shows the same steps working; its stale placeholders were fixed on the Mac before this run.
No other issue found.

## Isolation
Laptop user's default library `~/Videos/CaptureMann` (153-line recursive listing with sizes and mtimes), `~/.config/CaptureMann/settings.json` sha256 ea37a984...0191, `~/Pictures` entry count and `~/.config/CaptureMann` listing saved before and after: `diff` empty (see hand-back).

## Not run
Wayland smoke, full and hardware tiers, `linux.deb_install` and `linux.tarball_install` on the public files (tarball not installed here; check-package.sh not needed), sound/mic/camera, second monitor, KDE, Debian 13, AMD/NVIDIA, the no-cached-index signature refusal, `regress.ffmpeg_missing_wording` with FFmpeg hidden, comparing the key fingerprint with the release page.

## Final state
0.3.6 installed from APT (`ii capturemann 0.3.6`), repository source and keyring left in place, Ubuntu X11 session, test dirs and downloads removed, ~/wt kept. No code committed or pushed; new files in verification/ are untracked.
