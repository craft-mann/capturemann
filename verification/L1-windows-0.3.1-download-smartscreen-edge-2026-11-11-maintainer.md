# Windows 0.3.1: browser download of the installer (Edge) and Homebrew cask on macOS

- **Date:** 2026-11-11. **Status: PARTIAL** (the Edge result is reported by the maintainer, one run, written down by Claude from the maintainer's message; the Homebrew result was run by Claude).
- **Build:** the public release `v0.3.1`, `capturemann-0.3.1-windows-x64-setup.exe` (SHA-256 `d9ffd95665117c1c635e749df27c2f8dbc36d11844aa716abdfe738bf947cc78`, 86,587,794 bytes), downloaded from the release page in Edge on the lab ThinkPad T490 (Windows 11).

| Check | Result |
|---|---|
| Edge download of the installer from the release page | Edge showed its warning that the file **"isn't commonly downloaded"** (as reported). The maintainer pressed no other button: the installer was not kept, not run, and Windows' own "Windows protected your PC" screen was **not** seen. |
| Same download seen by Claude on the laptop's desktop | Edge held the file as `Unconfirmed … .crdownload` at the full size (86,587,794 bytes) and did not save it as the `.exe`; the panel was off the captured screen, so no screenshot of the warning exists. The held file was deleted. |
| Homebrew cask `0.3.1` on a Mac (throwaway tap, cask `capturemann-test`, app in `/tmp`) | Installed from the public DMG URL with the cask's checksums; `capturemann --version` printed 0.3.1; `codesign --verify` ok; `spctl`: accepted, Notarized Developer ID; no ticket stapled to the app itself (it is on the DMG). The existing 0.2.9 install was untouched; the test cask and tap were removed. |

## Not covered

- After "Keep": the Windows "Windows protected your PC" screen (More info, Run anyway) and the install itself, from a browser download. The silent install and the installer case of the regression ran from files copied by script, which carry no download mark.
- The Intel DMG in Homebrew; launching the Homebrew-installed app; a Mac with no network at first launch.
