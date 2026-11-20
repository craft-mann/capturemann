# L1 linux 0.3.6 regression report (full)

- **Date:** 2026-11-20T15:26:11Z to 2026-11-20T15:26:28Z
- **Who:** claude QA 0.3.6 final
- **Machine:** Linux 7.0.0-43-generic x86_64; GPU: Intel Corporation WhiskeyLake-U GT2 [UHD Graphics 620]
- **Build:** commit 57f50732, package folder `/home/trung/lab/q36f/pkg-deb/opt/capturemann/lib/capturemann`
- **FFmpeg:** ffmpeg version 9.0.2 Copyright (c) 2000-2026 the FFmpeg developers
- **Suite:** `cm-regress --tier full --out /home/trung/lab/regress-out/20261120T152611Z --who claude QA 0.3.6 final --verdict PARTIAL --allow screen --only linux.deb_install --package-dir /home/trung/lab/q36f/pkg-deb/opt/capturemann/lib/capturemann --installer /home/trung/lab/capturemann_0.3.6_amd64.deb --commit 57f50732`, tier `full`
- **Result:** 1 passed, 0 failed, 0 skipped of 1 cases
- **SHA-256 of the files under test:**
  - `capturemann` `8359808ec816df50c7fa983276334486438bbd8a7dfa634c089358459cc904f7`
  - `cm-mcp` `8bec8ead274aa13d941d3c575ced368f1e007352de1b79190693fd15258d70fd`
  - `cm-daemon` `a68f96d3ceb1d9eb019ec369c888de5aff580c53f21d29d0ec2ccba127abcfca`
  - `cm-desktop` `4930049bf83b045e48c2864a8081f41454bff4d8112b66f695029fcf6a746315`
  - `cm-overlay` `33cb287f06bccef931132b6b0db461c2b754256baa5c8a65aafbbbeffbf30a9c`
  - `ffmpeg` `5ad1e745fa8e7660516df437800cecdc7adc194d6c921f7beae3ea6ed0f2380c`
  - `ffprobe` `bea15d513c2119fe839fc68b0d19937e5912b6e5d66f98eeffab67be65742cf8`
  - `llama-server` `5854ad4a58a3bd9aead8151729a1ba10d7c291069d6203a823a3f098c61a7bd0`
  - `capturemann_0.3.6_amd64.deb` `94d90eb2075c524f47860c68f9cfbd0ca35c2e3e9d210f005c747e520deef22f`

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

- `linux.deb_install` (12460 ms): /home/trung/lab/capturemann_0.3.6_amd64.deb version 0.3.6; /opt/capturemann/lib/capturemann/capturemann --version: capturemann 0.3.6; documents checked in /usr/share/doc/capturemann

## Verdict

PARTIAL. An automated run on one machine; it covers what `docs/development/regression.md` lists and does not replace the manual scripts in `tests/manual/`. Nothing here is "verified" until a person has read it and says so.
