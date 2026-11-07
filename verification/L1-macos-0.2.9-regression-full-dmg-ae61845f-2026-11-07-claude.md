# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-07T11:51:31Z to 2026-11-07T11:51:34Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit ae61845f, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel4/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel4/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261107T115131Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg --commit ae61845f`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `dffdae9641a8f05678f5e088d456143bc2ea8ae099b929f1deb8558fb6f17e8d`
  - `cm-mcp` `81b30b575f0f23c0f89166fe65bbec932e2dee956ad7d3cf4a99e65bebc1737f`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `12d3b0240b69620527c93db071534c5ad96f34b33278863e3af8b53a840fec9f`
  - `cm-overlay` `e47924dbfdacbd05bcfe763dba110428122a463ec467a453de5ceea15ac85727`
  - `ffmpeg` `913f96cbf30474c1428cfc0a11b4a07a5b2b4638803b02e6985ac6e02c7840f7`
  - `ffprobe` `e7df61cc35cd8e9c983ca092ef98d93ec240a48d05a37f3b633e354083486058`
  - `llama-server` `46f9de90cd1b805fe2b724159578d2d95970df60d4ad393b52580224eace83e0`
  - `CaptureMann-0.2.9-arm64.dmg` `a728b522308767e390c238fb272040bf5f8bb8d0fdd1f9381aa750aa87958956`

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

- `installer.dmg` (1222 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr77024/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
