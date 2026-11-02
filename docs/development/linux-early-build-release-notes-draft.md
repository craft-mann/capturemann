# Linux early build: release notes (DRAFT, not published)

Draft of Nov 1, 2026 for the maintainer. Fill the lines marked **[after the laptop test]** from the reports of the
ThinkPad run (`verification/`), and delete any claim that run did not support. Nothing here has been posted. The
process is `docs/development/releasing.md`, "Linux early build".

---

# CaptureMann 0.2.6 for Linux (early build)

The first Linux build. It is an early build: screenshots, annotation, the library, local AI, the command-line client
and the MCP server work; **recording is not available yet** (it is switched off in this build).

## What works

- **Screenshots** of a region, a window or a whole display on X11, and of a region or a display on GNOME with Wayland
  (through the desktop's screenshot portal).
- **Editor and annotations**, copy and save, the library with search and bundles, in English and Vietnamese
  (Telex and VNI typing work in the editor).
- **Local AI**: transcription with Whisper (CPU), speaker labels, translation and reports with the built-in text
  engine. Models are downloaded only when you press Download; nothing is sent anywhere.
- **Auto-redact** of sensitive text in screenshots, through the system `tesseract` program (install
  `tesseract-ocr`; the `.deb` recommends it). Recall is lower than on macOS (84.5% of items found on our test set).
- **`capturemann` command line and the `cm-mcp` server** for agents; they start their own background daemon when the
  app is not running.
- **Tray icon**, global shortcuts on X11, and on Wayland through the GlobalShortcuts portal where the desktop has it.

## What does not work yet

- Screen and audio recording, the webcam, and MP4 export of recordings.
- Window screenshots on Wayland: there is no window list, and on GNOME 46 (Ubuntu 24.04) the desktop's own window
  chooser (`capturemann capture screenshot --pick`) takes the picture but does not hand it to the app. Use region or
  display.
- Global shortcuts on GNOME 46 Wayland (no portal); use the tray menu or the launcher.
- Copy image to the clipboard on GNOME 46 Wayland goes through XWayland (GNOME has no data-control protocol); it
  worked on the test laptop, and may fail on a Wayland-only session without XWayland.
- Tray icon on GNOME needs the AppIndicator extension, otherwise the app shows a notice instead.
- Windows and macOS builds are separate; this is Linux only.

## Install

Needs **glibc 2.39 or newer**: Ubuntu 24.04 and later, Debian 13, Fedora 40 and later. x86_64 and aarch64.

- Ubuntu and Debian: `sudo apt install ./capturemann_0.2.6_amd64.deb` (or `_arm64.deb`). It installs the libraries
  it needs. For Auto-redact: `sudo apt install tesseract-ocr`.
- Other distributions: unpack `capturemann-0.2.6-linux-<arch>.tar.gz`, run `./check-runtime.sh` (it names missing
  libraries) and `./install.sh`. The libraries are listed in `packaging/linux/README.md`.
- To keep passwords and sign-ins you need a Secret Service (GNOME Keyring or KWallet). Without one the app says so;
  `CAPTUREMANN_SECRETS=file` keeps secrets in a file only you can read (not encrypted).

## Check the download

Each file has a `.sha256` and a `.minisig`. `sha256sum -c <file>.sha256`, and
`minisign -Vm <file> -P RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k` (key ID `0A821D8028FF42DE`; the same
public key is in the repository as `packaging/linux/minisign.pub`).

## Tested on

- Automated tests on Ubuntu 24.04 x86_64 (GitHub) and aarch64 (a VM): all passed; the packages install and run in a
  clean Ubuntu 24.04 container.
- **[after the laptop test]** A Lenovo ThinkPad T490 (Intel UHD 620) with Ubuntu 24.04 on Xorg and on Wayland:
  list what passed and what did not, with the report names.
- Not tested: KDE, other GNOME versions, NVIDIA and AMD graphics, HiDPI mixes of displays, Fedora, Debian.

## Known issues

Listed in `docs/development/linux-early-build.md`. Report problems at the issue tracker with the output of
`capturemann diagnostics`.

## Licences

Apache-2.0. The packages bundle an LGPL build of FFmpeg (source offer and licences are inside), `llama-server`
(MIT), the sherpa-onnx speaker engine (Apache-2.0) and two speaker models; see `THIRD_PARTY_LICENSES.md` and the
licence folders in the package.
