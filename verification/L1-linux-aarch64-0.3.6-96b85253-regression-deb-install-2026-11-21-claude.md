# L1 linux 0.3.6 regression report (full)

> **Scope:** FINAL files of commit 96b85253 built in the aarch64 VM (QEMU, headless, Ubuntu 24.04.5) with `release-build.sh --jobs 3`, **unsigned** (the maintainer signs with minisign). Tarball `capturemann-0.3.6-linux-aarch64.tar.gz` sha256 `314ed7da8cfbc45ce692a0a00354223eb4073aaf8b76d7fdd075846b42749d2d`; `capturemann_0.3.6_arm64.deb` sha256 `8703938a0782fbb8a3a2a96b9427d7b7b921e9ede6f9462e19ac4fa3d2a0e6db`. Every desktop-session case is skipped (no display). Verdict PARTIAL.

- **Date:** 2026-11-21T13:27:07Z to 2026-11-21T13:27:16Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 96b85253, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/dev/reg036/deb --only linux.deb_install --installer /home/dev/wt/release-out/96b85253-aarch64/capturemann_0.3.6_arm64.deb --commit 96b85253 --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `ecb554676e025df513cdaf216754e13b1908165ff36c85e6df6135ffb5b6ae4f`
  - `cm-mcp` `bafe27e21dab9191736b00f9274c18cbe8645416bb420a185f5f9ec1ea581ce2`
  - `cm-daemon` `d67e9a28bf95395ced8d51991159d361823988508f1dc029a261b7ba6ab8b01c`
  - `cm-desktop` `d94e483b0dbfc1d5b5268b8ee1598811da9ab74ae061ae725719b2fa29276b2f`
  - `cm-overlay` `c9d4d0bc7ef24bb469a3af192e5a4094b5fd6536e7f629218e7445d37609ef1b`
  - `ffmpeg` `e658b2ba0e2bdcdbf56c56f1f7aa8b90c1af2b79d6a45a479dd6fac2bfba0450`
  - `ffprobe` `b3c36d8b44372709f98741ffddb0d475e3cda167c63eb5848d7cd1956479800d`
  - `llama-server` `70feb12005ee3f8c5353947a52d849bf2b2e8edcd7c38f43729065b5e177bdf8`
  - `capturemann_0.3.6_arm64.deb` `8703938a0782fbb8a3a2a96b9427d7b7b921e9ede6f9462e19ac4fa3d2a0e6db`

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

- `linux.deb_install` (7829 ms): /home/dev/wt/release-out/96b85253-aarch64/capturemann_0.3.6_arm64.deb version 0.3.6; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.6; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
