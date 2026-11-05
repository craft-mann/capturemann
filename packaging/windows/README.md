# Windows packaging

Status, Nov 1, 2026: the scripts ran on a GitHub `windows-latest` runner (workflow `windows-package.yml`, manual)
and produced the zip and the installer; the runner then unpacked the zip, ran the CLI (it started `cm-daemon`),
started `cm-desktop.exe` (it stayed up and drew its window) and installed and uninstalled silently. Nothing has
run on a physical Windows PC yet (the ThinkPad, planned after the Linux test). Report:
`verification/L1-windows-package-ci-2026-11-01-claude.md`.

## What the scripts do

* `build-windows.ps1`: release build of `cm-cli`, `cm-mcp`, `cm-desktop`, `cm-overlay` (`--no-default-features`,
  static CRT, GUI subsystem for the two window programs through `cargo rustc` link args; the speaker engine is
  built in when `packaging/sherpa/out` holds the matching libraries), `dumpbin /dependents`
  deny-list check, optional `llama-server.exe` bundle (`CM_LLAMA_BUNDLE`, `CM_REQUIRE_LLAMA=1`; next to the programs, with `Llama\BUILD-INFO.txt` and `Llama\LICENSES\`), optional LGPL FFmpeg bundle (`CM_FFMPEG_BUNDLE`, `ffmpeg.exe` next to the programs because
  `cm-media` looks beside the executable first), `THIRD_PARTY_LICENSES.md` for `x86_64-pc-windows-msvc` via
  `packaging/macos/gen-third-party.sh` (needs bash and python), `app.ico` from `design/brand/app-icon`, the zip,
  the NSIS installer and `.sha256` files. The .ps1 files parse (PowerShell 7).
* `installer.nsi`: per-user (no admin), `%LOCALAPPDATA%\Programs\CaptureMann`, Start Menu shortcut, HKCU uninstall
  entry, optional PATH entry (`/ADDPATH` when silent) through `path.ps1`. Compiled by `makensis` on the runner (Chocolatey `nsis`).
* `path.ps1`: adds or removes a folder in the per-user PATH, keeping REG_EXPAND_SZ.

## Decision (v0)

NSIS per-user installer plus portable zip (maintainer decision D-W3). MSIX is not chosen: it needs a signing
certificate to install at all and sandbox/identity work. Signing (maintainer decision O4, Nov 5, 2026): no code
signing in 0.x; the files are unsigned and published with SHA-256 sums and a winget manifest, so SmartScreen shows
"Windows protected your PC" on first run (More info, Run anyway).

## Text engine (llama-server)

`packaging/llama/build-windows.sh` (MSYS2 MINGW64; static, CPU only, AVX2, no libcurl/OpenSSL, only Windows system
DLLs allowed by its self-check) builds llama.cpp's `llama-server.exe` from the same pin as macOS (`LLAMA_TAG` and
`LLAMA_SHA256` in `packaging/llama/build-macos.sh`). `windows-package.yml` builds and caches it, runs
`--version` and `--help` only (no model), packages it with `CM_REQUIRE_LLAMA=1` and checks `capturemann diagnostics`
reports it as bundled. Whether a model ran with it on Windows: see `verification/` (`L1-windows-0.2.8-win-checks6-*`).

## Not done

* `packaging/ffmpeg/build-windows.sh` is written (Nov 4, 2026: MSYS2 MINGW64, `--target-os=mingw32
  --enable-mediafoundation`, `h264_mf`/`hevc_mf`, static libvpx and libopus, self-check with `objdump -p` allowing
  only Windows system DLLs) and wired into `windows-package.yml` and `build-windows.ps1` (`CM_FFMPEG_BUNDLE`,
  `CM_REQUIRE_FFMPEG=1`), but check the 0.2.8 reports in `verification/` for what ran on the lab laptop before relying on it.
* The speaker engine: `packaging/sherpa/fetch-windows.ps1` and the Windows link settings are written and wired into
  `ci.yml` (/MD libraries) and `windows-package.yml` (/MT libraries; `build-windows.ps1` builds with the default
  features when they match the C runtime, `CM_REQUIRE_SHERPA=1` makes their absence an error). See the 0.2.8 reports in `verification/` for whether it ran on the lab laptop.
* SmartScreen on a real browser download, Windows 10 22H2 and mixed-DPI layouts were not run (see the 0.2.8 reports).
* Executable icon and version resource: needs a `build.rs` (for example `winres`) in `apps/`; not done.
