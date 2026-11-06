# L1 macos 0.2.9 regression report (hardware)

- **Date:** 2026-11-06T23:48:41Z to 2026-11-06T23:48:47Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit unknown, package folder `/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier hardware --package-dir /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261106T234840Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --only desktop.hotkey_overlay`, tier `hardware`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `44d5da6176022e08436ec37d8e5a1a4a5eb3adb8525b7901cecffa74a4ed1ea5`
  - `cm-mcp` `c893a7aac9a70d69d914a15d3e2dfae8b5fb2839f89a56c38bc1b6d242a83ee9`
  - `cm-daemon` `34dfeceeee3973515d173c4806c68deb8dcf4e03dc6285f5148f5b56a1f3d3f4`
  - `cm-desktop` `28a0e768dafb1d0bfea4fbd47405658078b46400bc5cbb332ddf1ab38c1a99be`
  - `cm-overlay` `babd5f55c5922679cf648820aa6b64f6d1708a251a0115a9fb8234abbfc7f345`
  - `ffmpeg` `e52a04d18cc2205e66d763904d608b54eddd94738b6d3f96a10235beb545ce05`
  - `ffprobe` `ad4c6850e0864724e1bcaf60d8d22669202b2a5f95a31e10a3afdbe1c45873cc`
  - `llama-server` `d9e916fb218a72a445c4d0e51508ec02c1d1d5bbdd2796bf90a2c06423afde07`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `desktop.hotkey_overlay` | PASS | the screenshot hotkey (Ctrl+Alt+Shift+1; Option+Shift+1 on macOS) opens one overlay window per monitor on its exact rectangle; Esc ends it |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| CAP-1 overlay | PASS | `desktop.hotkey_overlay` |
| CAP-3 | PASS | `desktop.hotkey_overlay` |
| DPI-WIN | PASS | `desktop.hotkey_overlay` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `desktop.hotkey_overlay` (5800 ms): hwnd 0x1f345 pid 77231 class 'CaptureMann-dev' title 'CaptureMann overlay' rect [0, 0, 1512, 982] ex 0x8

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
