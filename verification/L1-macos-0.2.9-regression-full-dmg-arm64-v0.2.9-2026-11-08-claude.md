# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-08T04:17:13Z to 2026-11-08T04:17:15Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit e6ce0fd1, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel7/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel7/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T041712Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg --commit e6ce0fd1`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `573b75abff701d10148519bf648c99c9b3e6670ea92beba5887e02e770f84493`
  - `cm-mcp` `9ffc77152932b5c5d093f73fff07fef47831c1addb0c3fb8bd31b81827602a3e`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `853537d99103f40f5515ded0e84760fe10c533b7ea148f37324f8aa29b885367`
  - `cm-overlay` `4486406a446f5993cae5934001f1d4cd07632cb932376829b4200cc9fcbb9334`
  - `ffmpeg` `5be428ad96cd6ef422378a9c0b591fe8e5c7370ecfbced03c57a1b06bf16e650`
  - `ffprobe` `6c3047f33dd80cd680f5e7661c5bc30f221527bfc3ac4ab87ce2fb0c1392bcef`
  - `llama-server` `4c85a69cdf9750b2fbdc38a2ed204a124fa378d6b7d5b7bafdbcdb4a9bb50aa0`
  - `CaptureMann-0.2.9-arm64.dmg` `f2234e0c56f46d115639b97b0307793e4bf82dbfe7cdadb998312533c6ce0d58`

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

- `installer.dmg` (1281 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr37175/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
