# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-07T08:11:04Z to 2026-11-07T08:11:06Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 26f81195, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel3/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel3/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261107T081103Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg --commit 26f81195`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2fbe9895c4965ae3d31a763329edbf29035f218921c5414f37b42740524812d1`
  - `cm-mcp` `01af3a1e1913b258559a9f9631b21dbba0a79352e8d59a4b4fbe22e858793340`
  - `cm-daemon` `a03c732bb18275db18adaeb08ec88e27b087b11773fc72ce37866a7ecb9aa474`
  - `cm-desktop` `b80c947bf324999687af977c487d86548ce53ebcc38f2edcc6d96e7f51f1fa11`
  - `cm-overlay` `8b3ab178640b61eb0d5004f1c9384e3d38b83930c9ba1a7d973a213588ef2187`
  - `ffmpeg` `e44c11c00832793d20df1e0defede5237ca3fb55f0bf4b115b0aba943a6086ce`
  - `ffprobe` `6191f65daf15e0031ea0b8acf3848625ff74dbd139ad5f2bc885170dc576dce7`
  - `llama-server` `e257b02c4ae603da159ac7b529dcd9e93ad2daba76695665aee6529c88f34c26`
  - `CaptureMann-0.2.9-arm64.dmg` `49cc6f09a9022f107b0fb74ce515971c5d1b9b49f19e5701666ec56054d6b857`

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

- `installer.dmg` (1182 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-0.2.9-arm64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr77798/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
