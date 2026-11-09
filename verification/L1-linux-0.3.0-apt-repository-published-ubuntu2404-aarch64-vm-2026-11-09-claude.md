# L1 Linux 0.3.0: the published APT repository (aarch64 VM)

- **Date:** 2026-11-09
- **Who:** Claude (AI agent) for the repository steps; the maintainer made the key and signed the index on their own Mac
- **Machine:** the aarch64 Ubuntu 24.04 VM (QEMU, headless), a plain `apt` on a computer that had `capturemann` removed first
- **Repository:** the assets of the GitHub release tagged `apt` (`https://github.com/craft-mann/capturemann/releases/download/apt/`), flat layout, made by `packaging/linux/make-apt-repo.sh` (`6cdcb033`) with a throwaway key in the VM, then signed (`InRelease`, `Release.gpg`) by the maintainer with the project key `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D`; the signatures verify with the exported public key (`gpgv`, on the Mac)
- **Packages:** `capturemann_0.3.0_amd64.deb` (`d2b928d3...`) and `capturemann_0.3.0_arm64.deb` (`4e024e03...`)
- **Status:** PARTIAL (one architecture installed from it; no upgrade or removal through it; amd64 not installed)

| Step | Result | Note |
| --- | --- | --- |
| Download `capturemann-archive-keyring.gpg` and `capturemann.sources` with `curl` | PASS, after retries | the first download of an asset from GitHub's release host takes about 11 s and often ends in a 504; the 504 is cached for a short time (answers in 0.1 s), later requests give 200/206 in 0.3 s |
| `apt update` with `Suites: ./`, first run, assets not yet requested | PARTIAL | `InRelease` fetched; `Packages` ignored (504) until the host had it; two minutes later `apt update` ends clean ("Hit") |
| `apt update` with `Suites: /` | PASS | same behaviour once warm; it is **not** better than `./` (an earlier note here said GitHub rejects `/./`; wrong: a never-requested `/./Packages.xz` returned 200 after 11 s, then 0.3 s) |
| `apt install capturemann` | PASS | `capturemann --version` prints `capturemann 0.3.0`; `dpkg -s` Architecture arm64 |
| An amd64 install, upgrade, `apt remove`/`purge` from this repository | NOT RUN | |
| `test-apt-repo.sh --layout flat` (CI job `apt-repo`) | NOT RUN | the job has not run on GitHub |

Found: GitHub release downloads answer the first request for an asset slowly (about 11 s) and sometimes with a 504; a user's first `apt update` can fail and succeed on a repeat. `apt` does not retry by default, so the install lines use `curl --retry` and `apt -o Acquire::Retries=5`. The assets were requested once each after publishing to warm them. A stable host (S3 + CloudFront, 0.3.1 plan item 25) does not have this.
