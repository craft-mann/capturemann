# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-08T01:17:24Z to 2026-11-08T01:17:26Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 53a3cb4e, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel6/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel6/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T011723Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg --commit 53a3cb4e`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `ac33aaf1b350a43ca95591b415f50096808156520f53a34b53e4e59e2dccaccc`
  - `cm-mcp` `df2df224d6e8cb9defc3d5c62e8a75c9f4b623e5fe68f66e1bae9299121c4398`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `72a4352701c566e5a864b6c547ca8e3b8ba5edd9c2145974c4f54ddd91c0b777`
  - `cm-overlay` `2428f236b1e301daab04275439f9fecfff1d66fadf76df15aefa0638da77be4f`
  - `ffmpeg` `fd4fb21978726bc356a9d113d81de3a56ec028da8adf18a753b8eb4febef9203`
  - `ffprobe` `cb65a5d80f376c25ae2b293c56b263ee81488c50cf63c76d8449879d5f20fd34`
  - `llama-server` `9214af6070458059eeaf77c70ff30453d62a51b5fc7a38bdc16ce2484c5d6113`
  - `CaptureMann-0.2.9-arm64.dmg` `e85e9b097b304f5330bb207ffa64c506239688acfe185ee9e8419291be2f6f35`

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

- `installer.dmg` (1127 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr69835/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
