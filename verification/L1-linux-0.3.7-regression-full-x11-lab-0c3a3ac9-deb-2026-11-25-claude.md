# L1 linux 0.3.7 regression report (full)

- **Date:** 2026-11-25T03:46:38Z to 2026-11-25T03:46:53Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 0c3a3ac9, package folder `/home/trung/lab/pkg037/capturemann-0.3.7-linux-x86_64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261125T034637Z --who cm-regress via regress.sh --verdict PARTIAL --only linux.deb_install,linux.tarball_install --package-dir /home/trung/lab/pkg037/capturemann-0.3.7-linux-x86_64/lib/capturemann --installer /home/trung/lab/capturemann_0.3.7_amd64.deb --commit 0c3a3ac9`, tier `full`
- **Result:** 1 passed, 0 failed, 1 skipped of 2 cases
- **SHA-256 of the files under test:**
  - `capturemann` `8df8cbc20a8dfbb891d0ec329e6475c3759240b75e1c6f90dc07e58f996214c9`
  - `cm-mcp` `7b24535437110e9c5fbe06d7854e792a1c9e25a82b07fe92a8a497f80d0b702a`
  - `cm-daemon` `1b1921c43ec7b46740423ac8f590c7189361bb17918926f014b62a85f96ed57b`
  - `cm-desktop` `6ac43d820dfdb85ba3b60b6f5bacad2ea433457091e29f89feab6d17f02b86ac`
  - `cm-overlay` `b644209e9a073a3829b1f872d3867a7978e82566cf79ebf608776aa460611b6a`
  - `ffmpeg` `f96f435536da2e5bb56c30ba6e46e043e99903ec0f9a98002d2f538603230027`
  - `ffprobe` `509a0847f8ace31618c9c2ec289cb50b77da5f47d78718da0a526a47fb355c02`
  - `llama-server` `5afd310d794fe33e227ddeda02aec42bba863d45f26533fcae550458ef586b75`
  - `capturemann_0.3.7_amd64.deb` `14dbf66d6f7d3381b2ef4efa41b108cf3a2317c43fe76cfa9c304e0543b13e26`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `linux.deb_install` | PASS | the .deb installs, its programs find their libraries and run, remove and purge leave nothing |
| `linux.tarball_install` | SKIP | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing: --installer is /home/trung/lab/capturemann_0.3.7_amd64.deb, this case wants a file ending in .tar.gz or .tgz |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| PKG-deb | PASS | `linux.deb_install` |
| PKG-tarball | not run | `linux.tarball_install` |
| packaging/linux/build-deb.sh | PASS | `linux.deb_install` |
| packaging/linux/install.sh | not run | `linux.tarball_install` |

## Problems found

- None in the cases that ran.
## Not run

- --installer is /home/trung/lab/capturemann_0.3.7_amd64.deb, this case wants a file ending in .tar.gz or .tgz (1): linux.tarball_install

## Evidence of passing cases

- `linux.deb_install` (10760 ms): /home/trung/lab/capturemann_0.3.7_amd64.deb version 0.3.7; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.7; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
