# L1 linux 0.3.7 regression report (full)

- **Date:** 2026-11-25T04:38:43Z to 2026-11-25T04:38:53Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit cc1f281c, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/dev/reg037/deb --only linux.deb_install --installer /home/dev/wt/release-out/cc1f281c-aarch64/capturemann_0.3.7_arm64.deb --commit cc1f281c --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `9f1fa0e91113c8013b8fb37a98cde3892c53208f41e70cf890e9eadd9b842c29`
  - `cm-mcp` `42c267f642ba9ce9109cbc9a591617b04694bcf37bb9a0e35be6bee47d54a33a`
  - `cm-daemon` `9effdc9b085d03c0fc14d46d17ffa355b093278a27d08e0a28ca5fb4c49b8610`
  - `cm-desktop` `5dc36d8ca8323d38e64e13d98a8eed8e369f006d29d0e2eb23472ccea3e93b82`
  - `cm-overlay` `6575bb8571611cabb99e8e2b2970a2d99b415a826e8295470daff444ed322d68`
  - `ffmpeg` `b40429eeeb88f9992c195e19a95128523bf1b92d3d97a1a50f8fe6aa4e73920c`
  - `ffprobe` `4ef04c4b7ecc24cce38de39733674e9bb1c46fd6c963a850bacba1da9c0c8a44`
  - `llama-server` `e5347cca2583cb287df35af86bfd05f9e8c63e81650c431b3196c84dbfe8f7ff`
  - `capturemann_0.3.7_arm64.deb` `25997e06f037bffd381d790ec55fc209310b5ecb7eb23676902e287dad5cf91e`

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

- `linux.deb_install` (8532 ms): /home/dev/wt/release-out/cc1f281c-aarch64/capturemann_0.3.7_arm64.deb version 0.3.7; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.7; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
