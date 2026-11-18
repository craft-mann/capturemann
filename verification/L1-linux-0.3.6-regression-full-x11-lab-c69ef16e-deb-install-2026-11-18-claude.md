# L1 linux 0.3.6 regression report (full)

- **Date:** 2026-11-18T11:45:00Z to 2026-11-18T11:45:14Z
- **Who:** claude QA 0.3.6
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit c69ef16e, package folder `/home/trung/lab/pkg-deb/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261118T114500Z --who claude QA 0.3.6 --verdict PARTIAL --allow screen --only linux.deb_install --package-dir /home/trung/lab/pkg-deb/opt/capturemann/lib/capturemann --installer /home/trung/lab/capturemann_0.3.6_amd64.deb --commit c69ef16e`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `77ed535406a458168d16513535d5d2515c8277465d11262447507132996ee225`
  - `cm-mcp` `1b8bd3f0309a2c6f1bf351fc4e6a29b18f4f9b801c4abec2f51d87fd7f00481b`
  - `cm-daemon` `6112eb8b4286f31eecbc15c8ca97b1d2761d80371de1f08b773968acc7287694`
  - `cm-desktop` `fc9e6dd92a32f0489fb57ae8da281c52021cd5489853f7f309db6a4b602192af`
  - `cm-overlay` `5e5d59b7c9ebb1a7450175fb3cbe6de70727784656b6cf245f9946f9dcbb9b6b`
  - `ffmpeg` `0770905a03bba80f4963d292446fcf5875122fb35022c16df6513e4161ad967e`
  - `ffprobe` `21b16f1bf7970e49122bc39c5060b5c76960c723e342e68eed7b6a19e950fce9`
  - `llama-server` `b70e3c54c87ad0dd2206e57c9ae14e913e9748a650d68d13bad1dc4977088650`
  - `capturemann_0.3.6_amd64.deb` `13cde6c1626cb6e5af1dcb04923faabc3a125de39ceb3ba213586db0d2ed5f38`

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

- `linux.deb_install` (10906 ms): /home/trung/lab/capturemann_0.3.6_amd64.deb version 0.3.6; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.6; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
