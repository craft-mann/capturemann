# L1 linux 0.3.5 regression report (full)

> **Scope:** candidate files of commit 9dcfb5c3, built in the aarch64 VM with `release-build.sh --jobs 2`, **unsigned** (the maintainer signs with minisign). Tarball `capturemann-0.3.5-linux-aarch64.tar.gz` sha256 `07aed3c823330fd794dcf16732487ef91485fccd3fe6d409de93b51a5badc0d4`; `capturemann_0.3.5_arm64.deb` sha256 `6a859036a70d8a85043b408b9488ffcc55ebc035851df1af633e662396373cb9`. Headless VM: every desktop-session case is skipped. Not a report on final files.

- **Date:** 2026-11-18T01:57:00Z to 2026-11-18T01:57:09Z
- **Who:** cm-regress in the aarch64 VM (headless, no desktop session)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 9dcfb5c3, package folder `/home/dev/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/dev/reg035/deb --only linux.deb_install --installer /home/dev/wt/release-out/9dcfb5c3-aarch64/capturemann_0.3.5_arm64.deb --commit 9dcfb5c3 --who cm-regress in the aarch64 VM (headless, no desktop session) --verdict PARTIAL`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `98351cdc061801b35ed8291297aebd7d0e7af7811ce7a35a7789ff900797a338`
  - `cm-mcp` `e3d07bb4a11c65ef547c7d0dbf8196c281cfbb893e926c4cbd08601d2f57429f`
  - `cm-daemon` `04c1a569709d8239eae57fc2d169f9953e094057e0e7ea1825d0f5f22b936498`
  - `cm-desktop` `aa163d927805ceb5d3ecc21510a4e119d6c596a56de50742f67891c560eb0c76`
  - `cm-overlay` `2a269e37fadf1a04c3aba07834e5deaf3bc8360f97d5307e3abde180941885e6`
  - `ffmpeg` `c06cd2f498b13a2c74268e6da8f693b42073c6dce3948e27802282b9b5a5a36b`
  - `ffprobe` `5d9bd78ee71cf51e815e1aca37e060ee9c87a1204e269673a885e5e6c9f45478`
  - `llama-server` `d75b3aaa87ea5be921491f811a9c6b4af51e2ea4c8687faafd87ac4ab9d0d343`
  - `capturemann_0.3.5_arm64.deb` `6a859036a70d8a85043b408b9488ffcc55ebc035851df1af633e662396373cb9`

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

- `linux.deb_install` (7674 ms): /home/dev/wt/release-out/9dcfb5c3-aarch64/capturemann_0.3.5_arm64.deb version 0.3.5; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.5; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
