# GitHub release text for 0.2.6 (DRAFT, not published)

Draft of Nov 2, 2026 for the maintainer. Paste the section "Release body" into the release page. Everything in
**[brackets]** is filled after the last tests (reports in `verification/`) and the final package build; delete any
line the tests did not support. Nothing here has been posted. The process is `docs/development/releasing.md`.

- **Tag:** `v0.2.6` (never reuse a tag; see "Rollback and yank").
- **Title:** `CaptureMann 0.2.6: early builds for Linux and Windows`
- **Type:** draft, then **pre-release** (not "latest"): the macOS app is unchanged from 0.2.5 in this release.
- **Files to attach** (after `packaging/linux/sign-linux.sh <folder>` has written the `.minisig` files):

| File | What it is |
| --- | --- |
| `capturemann_0.2.6_amd64.deb` | Ubuntu and Debian, Intel and AMD (x86_64) |
| `capturemann_0.2.6_arm64.deb` | Ubuntu and Debian, ARM (aarch64) |
| `capturemann-0.2.6-linux-x86_64.tar.gz` | Other Linux distributions, x86_64 |
| `capturemann-0.2.6-linux-aarch64.tar.gz` | Other Linux distributions, aarch64 |
| `capturemann-0.2.6-windows-x64-setup.exe` | Windows installer, per user, no administrator rights |
| `capturemann-0.2.6-windows-x64.zip` | Windows, unpack and run |
| `<file>.sha256` for each of the six files above | SHA-256 checksum |
| `<file>.minisig` for the four Linux files | minisign signature (Windows files are not signed) |

The command, when the folder holds the files above (run by you; it creates a draft):

```sh
gh release create v0.2.6 --draft --prerelease --title "CaptureMann 0.2.6: early builds for Linux and Windows" \
  --notes-file release-body.md  <the files above>
```

---

## Release body

# CaptureMann 0.2.6: early builds for Linux and Windows

First builds of CaptureMann for **Linux** and **Windows**. They are early builds: take screenshots, annotate them,
keep them in a library, run the AI features on your own computer, and drive everything from the `capturemann`
command line or from an AI agent through the MCP server. **Recording is not available yet** on either system (it
stays on the macOS app for now). The macOS app is unchanged in this release.

### Which file do I download?

- **Ubuntu 24.04 or later, Debian 13:** the `.deb` for your CPU (`amd64` for Intel and AMD, `arm64` for ARM).
  `sudo apt install ./capturemann_0.2.6_amd64.deb` installs it with the libraries it needs.
- **Other Linux (glibc 2.39 or newer):** the `.tar.gz` for your CPU. Unpack it, run `./check-runtime.sh` (it names
  missing libraries), then `./install.sh`.
- **Windows 11 (x64):** `capturemann-0.2.6-windows-x64-setup.exe` (a per-user installer), or the `.zip` (unpack
  anywhere and run `cm-desktop.exe`; `capturemann.exe` is the command-line client).

### Linux (early build)

What works: screenshots of a region, a window or a display on X11, and of a region or a display on GNOME with Wayland;
the editor and annotations; the library; local AI (transcription, speaker labels, translation, reports; models are
downloaded only when you press Download); Auto-redact of sensitive text through `tesseract`
(`sudo apt install tesseract-ocr`; the `.deb` recommends it); the tray icon; global shortcuts on X11.

What does not work yet: recording and MP4 export of recordings; window screenshots on Wayland (no window list, and
the desktop's own window chooser on GNOME 46 does not hand the picture to the app); global shortcuts on GNOME 46
Wayland. Copying an image on GNOME 46 Wayland goes through XWayland: it worked on the test laptop and may fail on
a Wayland-only session. The tray icon on GNOME needs the
AppIndicator extension.

Passwords and sign-ins need a Secret Service (GNOME Keyring or KWallet). Without one the app says so;
`CAPTUREMANN_SECRETS=file` keeps secrets in a file only you can read (not encrypted).

### Windows (early build)

What works: screenshots of a region, a window (cut to its visible frame) or a display; the editor and annotations; the
library; the command line and the MCP server.

What does not work yet: recording; speaker labels and the bundled FFmpeg; Auto-redact needs
`winget install UB-Mannheim.TesseractOCR`. **The files are not signed**: Windows SmartScreen warns before it runs
them (More info, then Run anyway). Check the download with the `.sha256` file before you run it.

### Check the download

- Every file has a `.sha256`: `sha256sum -c <file>.sha256` (Linux) or `certutil -hashfile <file> SHA256` (Windows),
  and compare.
- The Linux files are signed with minisign: `minisign -Vm <file> -P RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k`
  (key ID `0A821D8028FF42DE`; the same key is in the repository as `packaging/linux/minisign.pub`).

### Tested on

- Automated tests on GitHub's Ubuntu 24.04 (x86_64 and aarch64) and Windows Server 2025 runners; the packages
  install and start there. Reports: `verification/L1-package-workflow-linux-2026-11-01-claude.md`,
  `verification/L1-windows-package-ci-2026-11-01-claude.md`.
- **[after the final tests]** A Lenovo ThinkPad T490 (Intel UHD 620), Ubuntu 24.04 on Xorg and on Wayland, and Windows
  11: what passed and what did not, with the report names (`verification/LINUX-1-*-thinkpad-2026-11-02-claude.md`,
  **[the Windows report]**).
- Not tested: KDE, GNOME versions other than 46, NVIDIA and AMD graphics, two monitors or mixed scaling, Fedora,
  Debian, Windows 10, ARM.

### Known issues

Listed in the repository (`docs/development/linux-early-build.md`). Report problems on the issue tracker with the
output of `capturemann diagnostics`.

### Licences

Apache-2.0. The packages bundle an LGPL build of FFmpeg on Linux (source offer and licences inside), `llama-server`
(MIT) and the sherpa-onnx speaker engine (Apache-2.0); see `THIRD_PARTY_LICENSES.md`.
