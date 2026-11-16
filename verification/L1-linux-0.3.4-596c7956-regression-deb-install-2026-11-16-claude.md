# L1 linux 0.3.4 regression report (full)

- **Date:** 2026-11-16T14:28:28Z to 2026-11-16T14:28:44Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 596c7956, package folder `/home/trung/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261116T142828Z --who cm-regress via regress.sh --verdict PARTIAL --allow screen --only linux.deb_install --installer /home/trung/lab/capturemann_0.3.4_amd64.deb --commit 596c7956`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `0c29e0307e59be6f1b016a5b4d02ab6f486a904f091f471b1e8c5d8c02982b07`
  - `cm-mcp` `f153a8b4cd736364478b6279145011aa1119e7f3e88e5925612d9b0fd591998b`
  - `cm-daemon` `6deecf3392ba649c459a544d6e4750792c233b42a1628a9ceb0b2d7d6a886dc7`
  - `cm-desktop` `dd79a10d73fda9fc973ea484ccd80e63044a5eaa04b4ef033d420491a7236d11`
  - `cm-overlay` `b76dce3f8e5fb5db71eadb4880568ca710dd950f52b5859276efb3151ac0553a`
  - `ffmpeg` `6899a667eaea231e9a3512112d31b7e70373537ac132ebec91e57b09e015a1d4`
  - `ffprobe` `2ac4f2794ce5f7f231a3459644fbaac5a28d0a9cd7a2c21e24fe35356429527e`
  - `llama-server` `e18b9918b4fe200ca481fed4878d234933cf6604dbcd899f77983ebb714f8c49`
  - `capturemann_0.3.4_amd64.deb` `2dac7de94cd4654c431ba577a22ecd8fa6471126cfa9aa88b632c849b7b5bcb4`

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

- `linux.deb_install` (11358 ms): /home/trung/lab/capturemann_0.3.4_amd64.deb version 0.3.4; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.4; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
