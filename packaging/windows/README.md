# Windows packaging (early build)

Status, Nov 1, 2026: the scripts ran on a GitHub `windows-latest` runner (workflow `windows-package.yml`, manual)
and produced the zip and the installer; the runner then unpacked the zip, ran the CLI (it started `cm-daemon`),
started `cm-desktop.exe` (it stayed up and drew its window) and installed and uninstalled silently. Nothing has
run on a physical Windows PC yet (the ThinkPad, planned after the Linux test). Report:
`verification/L1-windows-package-ci-2026-11-01-claude.md`.

## What the scripts do

* `build-windows.ps1`: release build of `cm-cli`, `cm-mcp`, `cm-desktop`, `cm-overlay` (`--no-default-features`,
  static CRT, GUI subsystem for the two window programs through `cargo rustc` link args; the speaker engine is
  built in when `packaging/sherpa/out` holds the matching libraries), `dumpbin /dependents`
  deny-list check, optional LGPL FFmpeg bundle (`CM_FFMPEG_BUNDLE`, `ffmpeg.exe` next to the programs because
  `cm-media` looks beside the executable first), `THIRD_PARTY_LICENSES.md` for `x86_64-pc-windows-msvc` via
  `packaging/macos/gen-third-party.sh` (needs bash and python), `app.ico` from `design/brand/app-icon`, the zip,
  the NSIS installer and `.sha256` files. The .ps1 files parse (PowerShell 7).
* `installer.nsi`: per-user (no admin), `%LOCALAPPDATA%\Programs\CaptureMann`, Start Menu shortcut, HKCU uninstall
  entry, optional PATH entry (`/ADDPATH` when silent) through `path.ps1`. Compiled by `makensis` on the runner (Chocolatey `nsis`).
* `path.ps1`: adds or removes a folder in the per-user PATH, keeping REG_EXPAND_SZ.

## Decision (v0)

NSIS per-user installer plus portable zip (maintainer decision D-W3). MSIX is not chosen: it needs a signing
certificate to install at all and sandbox/identity work; revisit with SignPath. The early build is unsigned, so
SmartScreen shows "Windows protected your PC" on first run (More info, Run anyway).

## Not done

* `packaging/ffmpeg/build-windows.sh` is written (Nov 4, 2026: MSYS2 MINGW64, `--target-os=mingw32
  --enable-mediafoundation`, `h264_mf`/`hevc_mf`, static libvpx and libopus, self-check with `objdump -p` allowing
  only Windows system DLLs) and wired into `windows-package.yml` and `build-windows.ps1` (`CM_FFMPEG_BUNDLE`,
  `CM_REQUIRE_FFMPEG=1`), but it has not run on Windows. The first run decides what needs fixing.
* The speaker engine: `packaging/sherpa/fetch-windows.ps1` and the Windows link settings are written and wired into
  `ci.yml` (/MD libraries) and `windows-package.yml` (/MT libraries; `build-windows.ps1` builds with the default
  features when they match the C runtime, `CM_REQUIRE_SHERPA=1` makes their absence an error). Not linked on Windows yet.
* The Windows section in `docs/development/releasing.md` and the paragraph in `docs/project/status.md`.
* A run on a physical Windows PC (clean install, SmartScreen, real GPU, uninstall leaving no files).
* Executable icon and version resource: needs a `build.rs` (for example `winres`) in `apps/`; not done.
