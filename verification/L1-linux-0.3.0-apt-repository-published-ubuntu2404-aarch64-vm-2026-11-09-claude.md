# L1 Linux 0.3.0: the published APT repository (aarch64 VM)

- **Date:** 2026-11-09
- **Who:** Claude (AI agent) for the repository steps; the maintainer made the key and signed the index on their own Mac
- **Machine:** the aarch64 Ubuntu 24.04 VM (QEMU, headless), a plain `apt` on a computer that had `capturemann` removed first
- **Repository:** the assets of the GitHub release tagged `apt` (`https://github.com/craft-mann/capturemann/releases/download/apt/`), flat layout, made by `packaging/linux/make-apt-repo.sh` (`6cdcb033`) with a throwaway key in the VM, then signed (`InRelease`, `Release.gpg`) by the maintainer with the project key `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D`; the signatures verify with the exported public key (`gpgv`, on the Mac)
- **Packages:** `capturemann_0.3.0_amd64.deb` (`d2b928d3...`) and `capturemann_0.3.0_arm64.deb` (`4e024e03...`)
- **Status:** PARTIAL (one architecture installed from it; no upgrade or removal through it; amd64 not installed)

| Step | Result | Note |
| --- | --- | --- |
| Download `capturemann-archive-keyring.gpg` and `capturemann.sources` with `curl` | PASS, after retries | GitHub answered some release downloads with a 504 that succeeded when retried |
| `apt update` with `Suites: ./` | FAIL | apt asks for `<URL>/./InRelease`; GitHub answers a `/./` path with a 504 every time (also with `curl --path-as-is`), the plain path gives 200 |
| `apt update` with `Suites: /` | PASS | `InRelease` fetched, no warning on the second run; `apt-cache policy` shows candidate 0.3.0 |
| `apt install capturemann` | PASS | `capturemann --version` prints `capturemann 0.3.0`; `dpkg -s` Architecture arm64 |
| An amd64 install, upgrade, `apt remove`/`purge` from this repository | NOT RUN | |
| `test-apt-repo.sh --layout flat` with `Suites: /` (CI job `apt-repo`) | NOT RUN | the script and the sources file now write `Suites: /`; the job has not run on GitHub |

Found: the flat layout must use `Suites: /` on GitHub release assets (changed in `make-apt-repo.sh`, `linux-apt.md` and the published `capturemann.sources`). A copy of the old `capturemann.sources` (`Suites: ./`) may be cached for a while by the download network.
