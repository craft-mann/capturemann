# L1 linux 0.3.4 regression report (full)

- **Date:** 2026-11-16T03:25:06Z to 2026-11-16T03:25:21Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 54a5bc68, package folder `/home/trung/.local/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261116T032506Z --who cm-regress via regress.sh --verdict PARTIAL --only linux.deb_install --installer /home/trung/lab/capturemann_0.3.4_amd64.deb --commit 54a5bc68`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `44d4355b1ca76b996087e1ec194a6c5dc65d7f409790c76a6ebe3f41fee2a1f6`
  - `cm-mcp` `116f8fd8f8dfc12eec1371093d7740bdbae85ae382954b5e12c68d56a2a6d9e3`
  - `cm-daemon` `e4873e9c40700b6ed005e35e1b0ae003f9ca7aa78c998d38687f067ee47ecc06`
  - `cm-desktop` `1533f0540e8b1cd5cfd5b074f2c83db3259f030d95ac322dda590949cfbc032d`
  - `cm-overlay` `b5968e010932266d0f4a63c3edbb8a4a6a0a27284800062ca6349407d32fc0ac`
  - `ffmpeg` `a76f89f1a0d735d048058deab833575dc3dde864a2e157ca92ccd58cde51e5e2`
  - `ffprobe` `9798d4c115d3507fb66a80a8be0e23d1dcdc46bb309b29ea07dc124e2363320f`
  - `llama-server` `7032b54d6e7ffba4a2f5e9ce79cf5f8e41b9ac990cd1ab73e9e879e71ded1559`
  - `capturemann_0.3.4_amd64.deb` `e65131755c4c7515d8445fd5a1fba36450a81ac4ea9c8b7b982264dec7d0b9c6`

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

- `linux.deb_install` (10961 ms): /home/trung/lab/capturemann_0.3.4_amd64.deb version 0.3.4; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.4; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
