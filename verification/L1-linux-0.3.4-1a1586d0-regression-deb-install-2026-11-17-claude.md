# L1 linux 0.3.4 regression report (full)

- **Date:** 2026-11-17T00:26:53Z to 2026-11-17T00:27:08Z
- **Who:** Claude lab agent
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 1a1586d0, package folder `/home/trung/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261117T002653Z --who Claude lab agent --verdict PARTIAL --allow screen --only linux.deb_install --installer /home/trung/lab/capturemann_0.3.4_amd64.deb --commit 1a1586d0`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `f43041d5cb52d020681f965474ec1126f602b0dec64a053ef8aace6aa9b47a0b`
  - `cm-mcp` `8ce8a4f44104d79cbd1ad25eec0d4911118c9851dc2b3a66ed4c982a8f4b8a12`
  - `cm-daemon` `099401423d010d6dea8eecd4625365935dcf05ed68e6ca51677ad2e74d9b5c9b`
  - `cm-desktop` `42b93047531255d5fa54dd0752a0eb4b75df8f2da5e7e2b2369872ddd63096c7`
  - `cm-overlay` `f87dbadf84f91e099f66977f0aefd55c6940e9a32eefbe66d884c0a10d074758`
  - `ffmpeg` `7708974b41c60115f01b1a4c259807076ef0fdfcd9031fc3faec390e265e8510`
  - `ffprobe` `0fc7f97e3e7c7e346bd67253328970f0bd72c4f39121b43a4dd5ffce6d164462`
  - `llama-server` `ddb3e933cfe04eddf8189f453833196ee0beda5d2b8b367c6bfe22924ba56fb7`
  - `capturemann_0.3.4_amd64.deb` `9946092603e019a29528a86b370691e669a7d957608e7eeb25920ada6adb542d`

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

- `linux.deb_install` (11287 ms): /home/trung/lab/capturemann_0.3.4_amd64.deb version 0.3.4; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.4; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
