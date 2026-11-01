# Monday run sheet: first day with the ThinkPad T490

For Monday, Nov 2, 2026. Setup details are in [`README.md`](README.md); the test steps are in
[`tests/manual/LINUX-1.md`](../../tests/manual/LINUX-1.md). Nothing here has run on the laptop yet: read each
script's output the first time.

## Part 1: set up the laptop (about 2 to 3 hours, mostly installers)

1. Update the BIOS, boot mode UEFI, Secure Boot on.
2. Install Windows 11 Pro with a local account, leaving about 250 GB unallocated. Run Windows updates, then
   `windows-setup.ps1` as Administrator.
3. Install Ubuntu 24.04 Desktop alongside Windows (minimal install, same user name, auto-login), then
   `sudo apt update && sudo apt full-upgrade`, then
   `linux-setup.sh --pubkey "$(cat ~/.ssh/id_ed25519.pub)" --hostname cm-t490` (the key is the Mac's).
4. Reboot once on each system. Reserve the laptop's IP in the router. Put its name in
   `~/.config/capturemann-lab`, then `scripts/testlab/lab.sh status` from the Mac.
5. Clonezilla baseline image of the clean disk before any CaptureMann build goes on.

## Part 2: Ubuntu, first install

1. Build the x86_64 tarball and `.deb` with the "Linux package" GitHub workflow (manual run), download them,
   then `lab.sh push linux <file>`.
2. Check the tarball: `sha256sum -c`, extract, `./check-runtime.sh`, then `./install.sh`. Then try the `.deb`
   on a restored clean image: `sudo apt install ./capturemann_*.deb` (note which packages it pulls).
3. `capturemann diagnostics` and `vainfo` (H.264 encode entries mean VA-API export works). Keep the output.

## Part 3: run LINUX-1 twice

| Pass | Session | Notes |
| --- | --- | --- |
| A | Ubuntu on Xorg (log out, gear icon at the login screen) | window list, X11 hotkeys, clipboard, tray |
| B | Ubuntu on Wayland (default) | portal screenshot, `--pick`, GlobalShortcuts, clipboard image on GNOME 46 (a known gap) |

On both: screenshot (region, window, display), the editor and annotations, Vietnamese typing, library, tray menu,
autostart, Auto-redact (after `sudo apt install tesseract-ocr`), local AI (model download, transcript of a short
file), CLI and MCP. HiDPI: set the display scale to 200 % and repeat one capture. Recording stays hidden; run it
only with `CM_LINUX_RECORDING=1` and only when you ask for hardware tests (it shows the screen-share dialog).

Write one report in `verification/` per pass (template in `verification/README.md`). Do not call a feature
"verified" without it. Delete any recording or capture you made that you do not need.

## Part 4: Windows (only if time allows)

Resume the parked branches `parked/windows-packaging` and `parked/windows-ci-real`, and run the smoke steps from
the laptop. Windows is not part of the first early build if Linux needs the time.

## Stop and ask when

- an installer or script wants to change the disk layout in a way the README does not describe,
- anything needs the camera, microphone or screen recording that you did not ask for,
- a result differs from LINUX-1's expected one and the cause is not clear.
