# APT repository 0.3.3 on a clean Ubuntu (aarch64 VM)

- **Date:** 2026-11-13 (after the release was published)
- **Who:** Claude Code session, at the maintainer's request; the repository was signed by the maintainer with the APT key (fingerprint `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D`)
- **Machine:** the Ubuntu 24.04 aarch64 VM on the maintainer's Mac (headless); it also holds a per-user tarball install from the regression run
- **What was done:** the four install lines of the release text (keyring and `capturemann.sources` from release `apt`, `apt update`, `apt install capturemann`)
- **Results:** `apt update` read `InRelease` and `Packages` from the release assets without a warning; `apt-cache policy capturemann` showed candidate 0.3.3; the install succeeded; `capturemann --version` printed `capturemann 0.3.3`; `dpkg -l` showed `0.3.3 arm64`. Before the upload, `gpgv` verified `InRelease` and `Release.gpg` with the published keyring, the keyring and `capturemann.sources` were byte-identical to the published ones, and the two 0.3.3 `.deb` files matched the files of the release.
- **Not run:** amd64 (the repository also lists it), an upgrade from 0.3.2 with `apt upgrade`, a non-VM computer, the Wayland session.
- **Verdict:** PARTIAL (one computer, one architecture).
