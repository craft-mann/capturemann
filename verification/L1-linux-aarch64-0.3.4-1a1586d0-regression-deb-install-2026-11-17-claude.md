# L1 linux 0.3.4 regression report (full)

- **Date:** 2026-11-17T03:23:39Z to 2026-11-17T03:23:48Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 1a1586d0, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/dev/reg034/deb --only linux.deb_install --installer /home/dev/wt/release-out/1a1586d0-aarch64/capturemann_0.3.4_arm64.deb --commit 1a1586d0 --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `c018eca93bdb15e9edea0089eecab25fba5a25ed889690cfee6546c98e641bd9`
  - `cm-mcp` `83841b5cf554163f51195d08929099022c0cf5b59af6325f6ff8a47a82a4da5a`
  - `cm-daemon` `596c46adb6c7428061aec3a4ec63e9715d855f7afca53650606c2f8658f0e0dc`
  - `cm-desktop` `a9b2b7ce44993ee3046e0b7f848ec51a3cd524ba401204e636252e651a65da41`
  - `cm-overlay` `b3a659b5d8fe4c2d253f2d0e37e4adf1f98f778f70398066a66fec6ed53af7f0`
  - `ffmpeg` `ee2e38d746e0faa716d4d92f57640b6a29aaec48a260ef88d056170e54619b2a`
  - `ffprobe` `782af3ee9e237373ab89ba65e81dc28908c45683ee8bfe3d84d92af0612b7244`
  - `llama-server` `29e706b9a81bc50b56bc9a9daaffc4a6daabe864d21a6043162aa2e240650ece`
  - `capturemann_0.3.4_arm64.deb` `c4f5ee48fabc281c547799ded898a86086f052b33f610cd9c4cd086ef5fdc3fd`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `linux.deb_install` | PASS | the .deb installs, its programs find their libraries and run, remove and purge leave nothing |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| PKG-deb | PASS | `linux.deb_install` |
| packaging/linux/build-deb.sh | PASS | `linux.deb_install` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `linux.deb_install` (7902 ms): /home/dev/wt/release-out/1a1586d0-aarch64/capturemann_0.3.4_arm64.deb version 0.3.4; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.4; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
