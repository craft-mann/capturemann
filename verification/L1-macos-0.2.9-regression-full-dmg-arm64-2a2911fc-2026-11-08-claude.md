# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-08T08:34:25Z to 2026-11-08T08:34:28Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 2a2911fc, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel8/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel8/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T083425Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg --commit 2a2911fc`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `d9c14a7e984e3aee132012050a5720e41bec68e8636caf97606c459397d0c583`
  - `cm-mcp` `e36c28544eee0015eb867324dde62ccc7dbb2251f4affec12e8d4d7ed04941f0`
  - `cm-daemon` `1789f697c37fde5756753cbc42c22cee7f61b0864a13fc9ce2f3e776120279b6`
  - `cm-desktop` `069ac16ce5ab73473a68f8fb19103e6397a3824d27d533b4388ca7924c106a8d`
  - `cm-overlay` `bcff522f558c847a2f37e7f3de1692fb19720a16fdca8c6c6f8c4190484689cf`
  - `ffmpeg` `5c515fadbd4579af1ea9f6918d7697819586346fc00213bf59dc3e900c325afb`
  - `ffprobe` `b78c68129f5d7bb22a6d25509c726b2920ae95d1c1923296ce8ac50005ffcdf7`
  - `llama-server` `45ba4c88844019442357e3a423c8eb7d73e33eaa8971ba154206fb71366fa4b6`
  - `CaptureMann-0.2.9-arm64.dmg` `fd5aed2d16dbcc09dd1ff4220453573a64af0a449948b3feb9e55ceace9b1e09`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `installer.dmg` | PASS | the DMG (or .app) installs into a temp folder: signature intact, programs work, removal leaves nothing |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| make verify-release | PASS | `installer.dmg` |
| packaging/macos | PASS | `installer.dmg` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `installer.dmg` (1274 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr74574/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
