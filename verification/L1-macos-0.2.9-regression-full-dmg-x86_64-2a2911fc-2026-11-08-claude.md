# L1 macos 0.2.9 regression report (full)

- **Date:** 2026-11-08T09:54:58Z to 2026-11-08T09:55:02Z
- **Who:** cm-regress via regress.sh
- **Machine:** Darwin 25.6.0 arm64; GPU: n/a
- **Build:** commit 2a2911fc, package folder `/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel8x/CaptureMann.app/Contents/MacOS`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --package-dir /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/1f2d4a7d-d900-4b93-afe1-037068344dda/scratchpad/rel8x/CaptureMann.app/Contents/MacOS --daemon /Users/trungtran/source/repos/capturemann/target/release/cm-daemon --out /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T//cm-regress-mac-20261108T095457Z --who cm-regress via regress.sh --verdict PARTIAL --only installer.dmg --installer /Users/trungtran/source/repos/capturemann/target/bundle-x86_64/CaptureMann-0.2.9-x86_64.dmg --commit 2a2911fc`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `f388b8d01ff0945faaa4e6f8754bef19fabc8b331d25f2d038702825077af19e`
  - `cm-mcp` `ee10d5fb637d583149ce8169f29eefc78091f611ab09094d42cbc20efa542163`
  - `cm-daemon` `1789f697c37fde5756753cbc42c22cee7f61b0864a13fc9ce2f3e776120279b6`
  - `cm-desktop` `0b556c33b967707bb7df59786f3a7ff6cd983322aff7897d169d66b86a2ed847`
  - `cm-overlay` `dc7d04e0186efda4f8a18a64fd8bd0497c70a7b2aef365b0ea4b270d7a77dc9d`
  - `ffmpeg` `12eac6023ace35e7cfc08d491ae6abe3779e7ddd0fba671e20ffa37ef77ced01`
  - `ffprobe` `1bad0e2cabaa266806f845303f4917ce421060919422d4b809e3b08242ed50cb`
  - `llama-server` `777c45e166ccb8333eeba6fbe6de72754dd3c2698e7736f2f2ffd2027cc40424`
  - `CaptureMann-0.2.9-x86_64.dmg` `dc3e44fb1cf605567d7259b18d5f8537cadf8c6d5e2254666e8ac155a2978d97`

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

- `installer.dmg` (1620 ms): mounted /Users/trungtran/source/repos/capturemann/target/bundle-x86_64/CaptureMann-0.2.9-x86_64.dmg (CaptureMann.app); copied the app to /var/folders/xm/d48xc26519j40jmsdcwh6v040000gp/T/cmr51082/1/dmg/CaptureMann.app; ejected the image; the code signature verifies (deep, strict); bundle id dev.capturemann.app, version 0.2.9; installed: capturemann 0.2.9; removed the copy; nothing left

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
