# L1 linux 0.2.9 regression report (full)

- **Date:** 2026-11-06T17:30:00Z to 2026-11-06T17:30:05Z
- **Who:** cm-regress via regress.sh
- **Machine:** Linux 7.0.0-42-generic x86_64; GPU: n/a
- **Build:** commit unknown, package folder `/home/trung/lab/pkg-deb/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261106T172959Z --who cm-regress via regress.sh --verdict PARTIAL --only linux.tarball_install --package-dir /home/trung/lab/pkg-deb/opt/capturemann/lib/capturemann --installer /home/trung/lab/capturemann-0.2.9-linux-x86_64.tar.gz`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `cf059d0f373a147816b10fac2599fe79a7ae1d36474bf179a709841ed731b0c9`
  - `cm-mcp` `fdaf0f0b9ae5e835d70857cb499a5512ecb3666652dfd60b310006250b582ead`
  - `cm-daemon` `0df2ab34585d65eb62e1813de5ac926a8be8641a9fd1cea78d6b207f3254fafb`
  - `cm-desktop` `8b95538f254b532d2c58a2bf5d505e6d0318582748b84ee763952741def78e84`
  - `cm-overlay` `ffe785f5cee771d7b0c5838fbe268ea4154bcd82f554feba0fa5dd03eb640435`
  - `ffmpeg` `1b928d7b7170b240a6efbafae9b1416812b85a921d54604a7a4bd5f06a91c400`
  - `ffprobe` `45dbb102c7babc4d45eef0b4365fbd2b08fabf55f448f6c9278a65d07780eae7`
  - `llama-server` `d693fb41e91a3329006276137757a600c8861e7a501468898dfa6fcb2ec10132`
  - `capturemann-0.2.9-linux-x86_64.tar.gz` `5b3b1f674f7c493f88209c4c36910385182f294ea1be962ae63195945d0ce485`

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

- `linux.tarball_install` (2614 ms): /tmp/cmr18389/1/prefix/lib/capturemann/capturemann --version: capturemann 0.2.9

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
