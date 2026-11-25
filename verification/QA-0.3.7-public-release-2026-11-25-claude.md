# QA 0.3.7, files from the PUBLIC release v0.3.7 and the APT repository (2026-11-25), PARTIAL

Who: Claude, driven from the Mac. Release published 07:22 UTC as Latest (25 files).

## Files and hashes
All eight packages downloaded from the public URLs equal `SHA256SUMS.txt` (`shasum -a 256 -c`: 8 OK). On the ThinkPad the Windows setup (`5e158e64...4b63`) and zip (`b63cc215...2c6b`) and the Linux x86_64 tarball and `.deb` also equal it; minisign verified both Linux files (trusted comments name the files).

## macOS (this Mac)
- `verify-release.sh` for both DMGs: passed, 0 warnings (version 0.3.7, minimum macOS 13.0). Stapler validates both; Gatekeeper accepts the arm64 DMG and its app copy with the Safari quarantine flag (Notarized Developer ID); `codesign --verify --deep --strict` ok. Timestamp of the arm64 DMG signature: Nov 25, 2026, 11:20 local.
- Homebrew: tap updated (commit `a592d79`); `brew fetch --cask craft-mann/tap/capturemann` fetched 0.3.7; `brew audit --cask --online` clean. `brew install` was not run (it would replace the maintainer's /Applications app).
- `cm-regress` standard on the arm64 app copy from the DMG: 42 passed, 9 failed, 13 skipped, all failures `permission_missing` (Screen Recording is not granted to the copy in /tmp); kept as `L1-macos-0.3.7-regression-standard-public-dmg-no-screen-grant-*`. It is NOT a regression result for the DMG. Not repeated; needs a grant.

## Windows 11 (ThinkPad), public setup installed over the CI package
Silent install exit 0, `capturemann --version` 0.3.7. `cm-regress --installed`, `--allow screen`: standard 57/0 (7 skipped); full 65 passed, 1 failed (`installer.silent`), 14 skipped. The failure: a leftover `CaptureMann (dev).lnk` (lab dev install of Nov 24) in the Start menu is reported as left by the uninstaller; with that shortcut moved away the case passes (`...-silent-rerun2-*`), shortcut restored. Reports `L1-windows-0.3.7-regression-{standard,full}-public-installed-*`.

## Linux x86_64 (ThinkPad, Ubuntu 24.04, X11)
`apt update` pulled the new `InRelease` and `Packages` from release `apt`; `apt install capturemann` installed 0.3.7 (`ii capturemann 0.3.7 amd64`); `apt list -a` shows 0.3.0 to 0.3.7; the cached `.deb` has the same sha256 as the release file. `cm-regress` on `/opt/capturemann/lib/capturemann`, `--allow screen`: standard 57/0 (7 skipped), full 64/0 (16 skipped). Reports `L1-linux-0.3.7-regression-{standard,full}-x11-public-apt-*`.

## APT repository
Made in the aarch64 VM (0.3.0 to 0.3.7, flat layout), checked on the Mac: `InRelease` and `Release.gpg` verify with the published keyring (key 516CCD4F...EF557D), `Packages`, `.gz`, `.xz` hashes equal `Release`, every `.deb` hash is in `Packages`; uploaded to release `apt` (25 assets, still a pre-release); the public `InRelease` and `Packages` are byte-identical to the local ones.

## Not run
macOS cm-regress on the DMGs (needs the grant), `brew install`, Intel Mac, aarch64 APT install and public-file regression, Wayland, Windows 10, SmartScreen, second monitor, hardware tiers.
