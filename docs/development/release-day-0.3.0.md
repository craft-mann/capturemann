# Release day, 0.3.0 (Linux): exact steps for the maintainer

Written the night of Nov 8 to 9, 2026 by the night orchestrator, after `release-day-0.2.8.md` and `release-day-0.2.9.md`.
"M" = only the maintainer does this (push, tag, sign, publish, accounts, key, hosting). Run commands from the repo root.
Text to publish: `release-0.3.0-github-text.md`. What happened during the night: `specs/plans/release-0.3.0-morning-report.md`.
0.3.0 is **Linux only** (x86_64 and aarch64): no Windows or macOS file, no winget, no website change for them, no Homebrew.

## Before the day: decisions that block the text

1. **[M] APT repository host** (plan decision O3). Options and recommendation in `linux-apt.md`, "Decisions for the
   maintainer": B (your own domain, static host) if you own a domain, otherwise A (GitHub release assets, flat layout).
   GitHub Pages does not fit (about 105 MB per `.deb`). The URL goes into every user's `.sources` file, so decide before the
   first publish. Until then the release text says "the APT repository comes later" (delete or fill the APT section of the text).
2. **[M] APT signing key.** Create it with the commands in `linux-apt.md` ("One-time: create the repository key"), on your
   own machine, never in the repository or in CI. Store the secret key, the passphrase and the revocation certificate.
   Put the fingerprint in the release text and on the website.
3. **[M] Retention** of older versions in the repository (the script keeps what you give it; with a first release there is nothing older).
4. The minisign key for the tarball and the `.deb` already exists (`packaging/linux/minisign.pub`, `sign-linux.sh`).
   If you do not have `~/.minisign/minisign.key` any more, make a new one and commit the new `.pub`; see the header of `sign-linux.sh`.

## 1. Check what the night left

1. `git log --oneline v0.2.9..HEAD | head -50` and read `specs/plans/release-0.3.0-morning-report.md` first.
2. The checks (CI runs them on the push too):

   ```sh
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets --locked -- -D warnings
   cargo test --workspace --locked
   cargo deny check
   python3 scripts/check_target_os.py
   cargo run -p cm-i18n -- check
   ```

3. The version bump is already in main (`release 0.3.0: workspace version 0.2.9 to 0.3.0`, `Cargo.toml` and `Cargo.lock`);
   `CHANGELOG.md`, `README.md`, `docs/project/status.md` and `CLAUDE.md` were updated in the "release 0.3.0" documentation commit.

## 2. Push and wait for CI

1. **[M]** Push `main` to `public` (`git push public main`). Wait for `ci.yml` and the security scan. The Linux workflow
   files (`linux-package.yml`, the `apt-repo` job) **have never run on GitHub**: expect to fix a path, a package name or a
   runner detail on the first run. The night built and checked the packages on the lab laptop and in the VM, not on GitHub.
2. **[M]** Run the **Linux package** workflow (Actions, Run workflow, ref `main`; both architectures) once the push is green.
   It is optional: the files of the night were built on the laptop (x86_64) and in the VM (aarch64) and are listed with their
   checksums in the release text. If you prefer the CI files as the release files, rebuild, then run the checks of section 3
   on them and replace the checksums in the text.

## 3. The release files and their checks

The files were rebuilt on Nov 9, 2026 from commit `6cdcb033` (the first build, from `5301d4b8`, said "early build" in the tarball README
and in the signature comment; that wording is gone). Later commits may change documents only:

- `capturemann-0.3.0-linux-x86_64.tar.gz`, `capturemann_0.3.0_amd64.deb` (laptop; `~/wt/release-out/6cdcb033-x86_64/`)
- `capturemann-0.3.0-linux-aarch64.tar.gz`, `capturemann_0.3.0_arm64.deb` (VM; `~/wt/release-out/6cdcb033-aarch64/`)
- each with `.sha256`, plus `SHA256SUMS.txt` and `BUILD-INFO.txt` in each folder.

They are on this Mac in `~/release-0.3.0-final/x86_64` and `~/release-0.3.0-final/aarch64` (sums checked; `~/release-0.3.0` holds the first build, do not publish it).
Check: `shasum -a 256 -c SHA256SUMS.txt` in the folder, and compare with the numbers in the release text. If you rebuild, rebuild
from the tag with `packaging/linux/release-build.sh --src <checkout> --commit v0.3.0` (about one hour on the laptop).

Sign (minisign, on your machine, the secret key never goes to CI):

```sh
packaging/linux/sign-linux.sh <folder with the files>      # writes <file>.minisig and verifies against minisign.pub
```

## 4. Tag and publish the GitHub release

1. **[M]** `git tag -a v0.3.0 -m "CaptureMann 0.3.0"` and `git push public v0.3.0` (never reuse a tag). The tag must be on the
   commit the files were built from, or a later commit that only changes documents: `git diff <build commit> v0.3.0 --stat` shows which.
2. **[M]** Draft first, as a pre-release (version 0.x):

   ```sh
   gh release create v0.3.0 --draft --prerelease -R craft-mann/capturemann \
     --title "CaptureMann 0.3.0: Linux catches up" --notes-file release-body.md \
     <the four packages, their .sha256 and .minisig files> SHA256SUMS.txt
   ```

   (`release-body.md` = the "Release body" section of `release-0.3.0-github-text.md`, with the placeholders filled: checksums,
   key fingerprints, APT host.) Download one package from the draft in a browser, check it, then publish.
3. Do not use `release.yml` for this release: it builds macOS.

## 5. The APT repository (after the decisions of "Before the day")

Follow `linux-apt.md`, "Every release: make the repository": download both `.deb`, then

```sh
CM_APT_GPG_KEY_FILE=~/secure/capturemann-apt-secret.asc CM_APT_GPG_PASSPHRASE=... \
  packaging/linux/make-apt-repo.sh --out apt-repo --url <your URL> --layout <pool|flat> <the two .deb files>
```

then upload `apt-repo/` to the host (option A: `gh release upload apt apt-repo/* --clobber` on a release with the fixed tag `apt`;
option B: `rclone sync` or `rsync`). Test it once on a clean Ubuntu 24.04 (a VM or a container): the install commands of the
user section of `linux-apt.md`, `sudo apt update`, `sudo apt install capturemann`, `capturemann --version`.
Then put the install lines and the key fingerprint in the release text and on the website.

## 6. After publishing

1. **[M]** Tell me (or the next session) the release is out: set `Status: released <date>` in `specs/plans/release-0.3.0-plan.md`,
   add the tag, date and file sums to `docs/project/status.md`, and edit `CLAUDE.md` "Current state" (0.3.0 released).
2. Move the open items to the 0.3.1 plan (`specs/plans/release-0.3.1-plan.md` already holds the non-Linux ones).
3. winget, Homebrew and the website download page do not change for 0.3.0 (Linux only).
