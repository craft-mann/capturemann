# L1 linux 0.3.1 regression report (full)

- **Date:** 2026-11-11T01:02:10Z to 2026-11-11T01:02:25Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5e1a0d9d, package folder `/home/trung/lab/pkg-0.3.1/capturemann-0.3.1-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261111T010210Z --who cm-regress via regress.sh --verdict PARTIAL --only linux.deb_install --package-dir /home/trung/lab/pkg-0.3.1/capturemann-0.3.1-linux-x86_64/lib/capturemann --installer /home/trung/lab/capturemann_0.3.1_amd64.deb --commit 5e1a0d9d`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `13f8eef07abddbdbeced18d24d89935f03dfd08d0a61da60846fd5b88c67929b`
  - `cm-mcp` `3d326b4d7d5c4a390d50f0bf1bbfacdf95778b9b520e01683d428b1c1e8f4cd2`
  - `cm-daemon` `e571a30b686af90148cec0c072fa56116c33e7596d672436e5cfeddfcaf8b298`
  - `cm-desktop` `3ece2fd78d2e709d87bd4d6508580e50a384d5a2a125e7fa3c5b3b75d37344cc`
  - `cm-overlay` `567bf63e28a789b1187e8fbe24a835b37a9e6063390be9115a3573c91c4bf86e`
  - `ffmpeg` `938493953ab257b6e930a3da39c5c813111a447f1eff5c162378e3b774429ba6`
  - `ffprobe` `cac1da599bda11125bb6b86baa4ae6aa2f8221fbeff519f68ae6c9fb0d6d7760`
  - `llama-server` `6b75848a8a8e358a2e0421f8de74b4b111b93acc6d327d8e0450ce6131c64727`
  - `capturemann_0.3.1_amd64.deb` `b9f8195a79e0fdf4b45df65c4cc4bc3d990db0459d272f7bad9ee1282a1b7f8d`

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

- `linux.deb_install` (10881 ms): /home/trung/lab/capturemann_0.3.1_amd64.deb version 0.3.1; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.1; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
