# L1 linux 0.3.0 regression report (full)

- **Date:** 2026-11-08T20:08:31Z to 2026-11-08T20:08:46Z
- **Who:** night orchestrator
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 5301d4b8, package folder `/home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261108T200830Z --who night orchestrator --verdict PARTIAL --allow screen --only linux.deb_install --package-dir /home/trung/wt/x64final/capturemann-0.3.0-linux-x86_64/lib/capturemann --installer /home/trung/lab/capturemann_0.3.0_amd64.deb --commit 5301d4b8`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `2d6346b93480161ec442ec26ebc099e9d4dbc0e7351944bd6a20cdabf32adf60`
  - `cm-mcp` `0baca1bbeb63be2a930c584db209039e45488366172ff1364f1e3477a30215ca`
  - `cm-daemon` `c546c08a0fd110d225cb636f5376fdc4fde0d1ae1e65a77b37846688e76727d0`
  - `cm-desktop` `b6e76b9e36b3ad2d464a217b1f4faf138dbdae05be24101de707d931905daa6c`
  - `cm-overlay` `fbe5ecfd0192d3026e69c5a9e0c8d0d521e363328009b2f33812d1c953503494`
  - `ffmpeg` `5da034620e64147d7e2ddf849ba258e34996e45849a7ea4777a3eef145e07ccc`
  - `ffprobe` `af190fa91ddd6d97b6e4426461526f3950509454a42c1e91d55b4c57cb78bc13`
  - `llama-server` `9b6a30cc527aa50e6f1c309f103f7fdb87c944a13d3382e733fee7c1d881ca5c`
  - `capturemann_0.3.0_amd64.deb` `30b2cd10316233755cc730271315c03cbcdb6c0d16aa4f54267b37fc128a7158`

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

- `linux.deb_install` (10835 ms): /home/trung/lab/capturemann_0.3.0_amd64.deb version 0.3.0; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.0; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
