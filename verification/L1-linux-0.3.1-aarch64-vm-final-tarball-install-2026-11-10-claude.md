# L1 linux 0.3.1 regression report (full)

- **Date:** 2026-11-10T15:19:43Z to 2026-11-10T15:19:46Z
- **Who:** night orchestrator (Claude)
- **Machine:** Linux 6.8.0-147-generic aarch64; GPU: n/a
- **Build:** commit 5e1a0d9d, package folder `/home/dev/wt/pkg031-final/capturemann-0.3.1-linux-aarch64/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --only linux.tarball_install --installer /home/dev/wt/release-out/5e1a0d9d-aarch64/capturemann-0.3.1-linux-aarch64.tar.gz --package-dir /home/dev/wt/pkg031-final/capturemann-0.3.1-linux-aarch64/lib/capturemann --out /home/dev/wt/regress-out-tar --commit 5e1a0d9d --who night orchestrator (Claude) --verdict PARTIAL`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `e3dfed4fe06ce1824855a594a4245fda9f6765ee83f06e9c2081d378e33c9951`
  - `cm-mcp` `818b7049cc3154f9752db5fbc47517d761f6dfa554a51d98dd80166c6c4f7398`
  - `cm-daemon` `3db6f543acb4a36d15128fe949ac1bc63a46f570661911bf86757a5cc63fe77d`
  - `cm-desktop` `55d455a24d4c18232270ac7d007f3f505d97ff4c5b9b2f68abe3d7933de3aaec`
  - `cm-overlay` `a99e8a546ae7677a1adb0588cf7bbd7ed5fcd74b45253cb14a73aff8cc20ba2e`
  - `ffmpeg` `0421449aaab9174bc2ddb76431503b6b493718c56d44c56a6ec684b02f6ce5c2`
  - `ffprobe` `07c7716df41c96419554363524159a53a4311b091df39d7fc0b6e89ebd826a75`
  - `llama-server` `1eb906a779439097f94f47d0a96d3449db71ad9a127b444fe648e8a795986baf`
  - `capturemann-0.3.1-linux-aarch64.tar.gz` `d5875e59b44b519ee1ecde7c3d749a5efb6e8962e9235610d749e34055eedf3d`

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `linux.tarball_install` | PASS | the tarball installs into a temporary prefix, its programs run, --uninstall leaves nothing |

## Acceptance criteria and bug classes

| Item | Result | Cases |
| --- | --- | --- |
| PKG-tarball | PASS | `linux.tarball_install` |
| packaging/linux/install.sh | PASS | `linux.tarball_install` |

## Problems found

- None in the cases that ran.
## Not run

- Every selected case ran.

## Evidence of passing cases

- `linux.tarball_install` (2073 ms): /tmp/cmr1020130/1/prefix/lib/capturemann/capturemann --version: capturemann 0.3.1; documents checked in /tmp/cmr1020130/1/prefix/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
