# L1 Linux 0.3.0 packaging: check-package.sh and the APT repository (aarch64 VM)

- **Date:** 2026-11-08
- **Who:** Claude (AI agent, packaging track), nothing run by a person
- **Machine:** the aarch64 Ubuntu 24.04.5 VM (QEMU, 4 CPUs, headless); the APT flow in a clean Ubuntu 24.04 arm64 `debootstrap --variant=minbase` root (`sudo chroot`), not a container
- **Scripts:** `packaging/linux/{check-package.sh,make-apt-repo.sh,test-apt-repo.sh}` at branch `track/linux-packaging` (commits bf50f24e, a3de9709, 9195617d)
- **Package under test:** `capturemann-0.2.9-linux-aarch64.tar.gz` and `capturemann_0.2.9_arm64.deb`, made by `build-linux.sh` and `build-deb.sh` of this branch with `CM_REQUIRE_SHERPA/SPEAKER_MODELS/LLAMA/FFMPEG=1`, from **old release binaries** (`CM_SKIP_BUILD=1`, the VM's earlier `target/release`; they print `capturemann 0.2.5`) plus the FFmpeg, llama-server and sherpa outputs of earlier VM builds. So this run proves the packaging scripts and checks, not the current code. A from-source build of this commit on aarch64 was not run here.
- **Status:** PARTIAL (aarch64 VM only; chroot, not Docker; stale program binaries; no key of the project used)

| Step | Result | Note |
| --- | --- | --- |
| `build-linux.sh` with all REQUIRE flags (FFmpeg, llama-server, sherpa, models) | PASS | tarball 136 MB |
| `build-deb.sh` | PASS | `.deb` 97 MB, now ships `LICENSE` |
| `check-package.sh` on the tarball | PASS | 8 ELF files all aarch64, newest glibc symbol 2.39, no bundled system/GPU libraries, LICENSE/NOTICE/THIRD_PARTY_LICENSES, FFmpeg source offer + LICENSES, llama.cpp MIT text, speaker model licences, `desktop-file-validate`, programs start, `ldd` resolves |
| `check-package.sh` on the `.deb` | PASS | same plus Debian `copyright` (DEP-5) and `changelog.gz`, control fields |
| `check-package.sh` on the 0.2.9 x86_64 `.deb` from the lab (run on the T490, unpacked only, nothing installed) | FAIL, as intended | "missing LICENSE" (the 0.2.9 `.deb` lacks it); the 0.2.9 x86_64 tarball passed |
| `make-apt-repo.sh` pool and flat, throwaway key | PASS | its own checks: signatures verify with the exported public key alone, Release hashes, package hashes |
| `test-apt-repo.sh --layout pool`: add key and deb822 source, `apt-get update --error-on=any` clean, `apt install capturemann`, `apt upgrade` 0.2.9 to 0.2.9+aptcheck2, index changed after signing refused (BADSIG), `apt remove` (no `/opt/capturemann`, no `/usr/bin` links), `apt purge`, source stays | PASS | chroot has no `gpg`: the binary `Signed-By` keyring is enough |
| `test-apt-repo.sh --layout flat` (`Suites: ./`) | PASS | same steps |
| Docker run of the same script (CI job `apt-repo`, both architectures) | NOT RUN | no Docker here; the workflow has not run on GitHub |
| Repository signed with the real project key, any publishing | NOT DONE | the key does not exist; nothing is published |
