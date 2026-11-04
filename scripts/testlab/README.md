# Test lab: one ThinkPad T490, Windows 11 and Ubuntu 24.04

A dedicated laptop (Intel Core i7 8th gen, UHD Graphics 620, 32 GB, no NVIDIA) for checking the Windows and
Linux early builds on real hardware. Nothing else runs on it, so it can be configured for testing and
restored from an image. The scripts here were written on Oct 31, 2026 and **have not run on the laptop yet**
(`linux-setup.sh --dry-run` was checked in an Ubuntu VM; `windows-setup.ps1` only parse-checked): read their
output the first time and fix what is wrong.

| File | Runs on | Does |
| --- | --- | --- |
| `windows-setup.ps1` | the laptop, Windows, Administrator PowerShell | OpenSSH server + key, never sleep/lock, Fast Startup and BitLocker off, UTC clock, `C:\lab\lab-next-boot.ps1` |
| `linux-setup.sh` | the laptop, Ubuntu | ssh key login, auto-login, no sleep/lock, the tarball's runtime packages, Intel VA-API driver, test tools, GRUB remembers the OS, `lab-next-boot` |
| `lab.sh` | the developer's Mac | run commands, copy files, switch the laptop between Ubuntu and Windows |

## One-time installation order

1. **BIOS** (F1 at power-on): update the BIOS first (Lenovo Vantage or the support site). Config > Power > Sleep
   State: *Linux* (if offered). Keep Secure Boot on (Ubuntu's boot loader is signed). Boot mode UEFI.
2. **Windows 11 Pro** from a USB stick (Microsoft's Media Creation Tool). Use a *local* account (at the network
   screen: Shift+F10, `OOBE\BYPASSNRO`, then "I don't have internet"). In the installer's partition screen
   shrink the main partition so that about 250 GB stay unallocated for Ubuntu. Name the user the same as on
   Ubuntu (for example `lab`). Install all Windows updates, then run `windows-setup.ps1`.
3. **Ubuntu 24.04 Desktop** from a USB stick, "Install alongside Windows Boot Manager" (or choose the free space
   by hand: ext4 for `/`, the existing EFI partition mounted at `/boot/efi`, no new EFI partition). Select
   minimal installation, the same user name, and "Log in automatically". After the first boot run `sudo apt
   update && sudo apt full-upgrade`, then `linux-setup.sh --pubkey <your Mac's public key> --hostname cm-t490`.
4. Reboot once on each system. Give the laptop a DHCP reservation on your router (or a static IP) and put its
   name in `~/.config/capturemann-lab` on the Mac (see the header of `lab.sh`).
5. **Baseline images** (do this before installing any CaptureMann build): boot a Clonezilla USB and save
   an image of the whole disk (or of both OS partitions plus the EFI partition) to an external drive. To get a
   clean slate for a release test, restore it. Re-make the images after you change the base setup.

## Day to day, from the Mac

```sh
scripts/testlab/lab.sh status                       # which OS answers on ssh
scripts/testlab/lab.sh push linux dist/capturemann-0.2.6-linux-x86_64.tar.gz
scripts/testlab/lab.sh linux 'cd ~/lab && tar xzf capturemann-*.tar.gz && ./capturemann-*/install.sh'
scripts/testlab/lab.sh switch windows               # reboot into Windows and wait for it
scripts/testlab/lab.sh windows 'C:\lab\capturemann\capturemann.exe --version'
scripts/testlab/lab.sh pull linux '~/.local/share/capturemann/logs' ./logs
```

Wayland or X11 on Ubuntu: pick it at the login screen (gear icon) after logging out, or set the session in
`/etc/gdm3/custom.conf` (`WaylandEnable=false` forces X11). With auto-login the last session type is kept.
Tests that need the screen (screenshots, `tests/manual/LINUX-1.md`) run in the logged-in desktop session; over
ssh set `XDG_RUNTIME_DIR=/run/user/$(id -u)`, `DBUS_SESSION_BUS_ADDRESS=unix:path=$XDG_RUNTIME_DIR/bus` and
either `WAYLAND_DISPLAY=wayland-0` or `DISPLAY=:0` (and `XDG_SESSION_TYPE`).

## Checks worth running once the laptop is set up

- Intel VA-API: `vainfo` should list `VAProfileH264Main/High` with `VAEntrypointEncSlice` (the iHD driver,
  `intel-media-va-driver`). The software VP9 recorder does not need it; a hardware H.264 path would.
- Windows: `Get-CimInstance Win32_VideoController` shows the Intel UHD 620; Media Foundation hardware H.264
  encoders show up in the encoder list of `capturemann diagnostics` once the Windows recorder exists.
- Rules: hardware tests (screen, camera, microphone) run only when the owner asked; delete their output.

## Windows test programs (built by CI, run on the laptop)

The laptop has no Rust toolchain, so the tests that need real Windows hardware are compiled on a hosted runner by
the manual workflow **Windows tests** (`.github/workflows/windows-tests.yml`: Actions > Windows tests > Run
workflow, on the branch to test). It does not run them; it keeps one artifact, `capturemann-windows-tests.zip`
(+ `.sha256`, 7 days): the hardware and OS tests of `cm-capture`, `cm-encode`, `cm-audio`, `cm-camera`, `cm-media`,
the recognizer test of `cm-render`, the `cm-desktop` parity test, the `record_e2e` and `make_fixture` examples
(release) and the Visual C++ runtime DLLs they link. The list of programs, the exact test names and what each needs
are in `scripts/ci/windows-tests-README.md` (the same file is inside the zip).

```sh
gh workflow run windows-tests.yml --repo craft-mann/capturemann -r track/windows   # or the Actions page
scripts/testlab/win-run-tests.sh latest        # newest successful run on track/windows; or a run id
                                               # downloads, checks the SHA-256, pushes, unpacks to C:\lab\tests
```

`win-run-tests.sh` ends by printing the run commands. Two ways to run a program:

- **Over ssh** (`lab.sh windows '<exe> ...'`): session 0, **no desktop**. Fine for unit tests (`*__lib.exe`), the
  `cm-media` tests, the recognizer test and the encoder test (`cm-encode__mf_real.exe`; if it finds no hardware
  encoder here, run it in the session).
- **In the logged-in session** (`win-run-in-session.ps1`): everything that captures the screen or a window, opens
  the camera or the microphone, hooks the cursor, or needs a GPU adapter for the parity test. The script registers a
  one-shot scheduled task for the console user, runs the program through a wrapper, waits, prints the output (last
  line `EXIT <code>`), and removes the task:

```sh
scripts/testlab/lab.sh windows 'C:\lab\win-run-in-session.ps1 -Exe C:\lab\tests\cm-capture__real_display.exe -Arguments "--ignored --nocapture --test-threads=1" -OutFile C:\lab\out\real_display.txt'
```

  Options: `-TimeoutSec` (default 600; on a timeout the program is stopped and the script exits 124), `-EnvVars
  'CM_OCR_REAL=1;K=V'`, `-WorkDir`. It needs the console user to be the ssh user and logged in (the setup keeps
  the laptop logged in and unlocked). **Not run on the laptop yet, and not parse-checked** (written Nov 4, 2026 on
  a Mac where `pwsh` could not be run from the agent's sandbox): run `[System.Management.Automation.Language.Parser]::ParseFile`
  on it first and read its first output before trusting it.

The `parity` test needs a full `ffmpeg` and `ffprobe` on the laptop's `PATH` (for example `winget install
Gyan.FFmpeg`); no FFmpeg is in the artifact. Hardware tests follow the rule above: only when the owner asked, short,
output deleted; the loopback chime test plays a sound. A result is "verified" only with a report in `verification/`.
