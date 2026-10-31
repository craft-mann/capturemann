# Windows packaging (early build) - UNFINISHED AND UNTESTED

Nothing here has run on Windows. Work was stopped early (coordinator request, Oct 31, 2026). The lead runs it on
a Windows machine or runner and fixes what breaks.

## Done (written, never executed)

* `build-windows.ps1`: release build of `cm-cli`, `cm-mcp`, `cm-desktop`, `cm-overlay` (`--no-default-features`,
  static CRT, GUI subsystem for the two window programs through `cargo rustc` link args), `dumpbin /dependents`
  deny-list check, optional LGPL FFmpeg bundle (`CM_FFMPEG_BUNDLE`, `ffmpeg.exe` next to the programs because
  `cm-media` looks beside the executable first), `THIRD_PARTY_LICENSES.md` for `x86_64-pc-windows-msvc` via
  `packaging/macos/gen-third-party.sh` (needs bash and python), `app.ico` from `design/brand/app-icon`, the zip,
  the NSIS installer and `.sha256` files. Syntax of the .ps1 files was not checked (the PowerShell parse check was
  not run).
* `installer.nsi`: per-user (no admin), `%LOCALAPPDATA%\Programs\CaptureMann`, Start Menu shortcut, HKCU uninstall
  entry, optional PATH entry (`/ADDPATH` when silent) through `path.ps1`. Not compiled (no `makensis` here).
* `path.ps1`: adds or removes a folder in the per-user PATH, keeping REG_EXPAND_SZ.

## Decision (v0)

NSIS per-user installer plus portable zip (maintainer decision D-W3). MSIX is not chosen: it needs a signing
certificate to install at all and sandbox/identity work; revisit with SignPath. The early build is unsigned, so
SmartScreen shows "Windows protected your PC" on first run (More info, Run anyway).

## Not done

* The smoke-test step (silent install into a temp folder, `capturemann.exe --version`, silent uninstall with `_?=`).
* `packaging/ffmpeg/build-windows.*`. Plan: MSYS2 MINGW64 (preinstalled at `C:\msys64` on `windows-latest`),
  same pinned sources and component lists as `build-linux.sh`, `--target-os=mingw32 --enable-mediafoundation`
  (encoder `h264_mf`, no libx264), static libvpx (`x86_64-win64-gcc`) and libopus, `-static` link, self-check with
  `objdump -p` allowing only Windows system DLLs.
* `.github/workflows/windows-package.yml` (manual dispatch, `windows-latest`, `choco install nsis`).
* The Windows section in `docs/development/releasing.md` and the paragraph in `docs/project/status.md`.
* The verification report.
* Executable icon and version resource: needs a `build.rs` (for example `winres`) in `apps/`; not done.
