# Release day, 0.2.9 (Windows, macOS, Linux): exact steps for the maintainer

Prepared Nov 6, 2026 from `release-day-0.2.8.md` (Windows), `releasing.md` (macOS DMGs, Homebrew, Linux files) and
`specs/plans/release-0.2.9-plan.md`. Text to publish: `release-0.2.9-github-text.md` (it has `<FILL: ...>` placeholders
that need the stored regression reports). "M" = only the maintainer does this (merge, push, tag, sign, publish, accounts,
decisions). Run commands from the repo root. **One tag, `v0.2.9`, for all three platforms; it goes out only when each platform
has a stored regression run on its final files** (decision of Nov 6, 2026). Nothing in this file has been run.

## Before the day: decisions and gates

1. **Gate:** items 1 to 5 and 7 of the 0.2.9 plan are merged and CI is green on `main`; the regression drivers (item 6) are
   merged. If a platform's run fails, read the failure and fix the product or the suite (never the evidence), rebuild and
   rerun; the text says what ran.
2. **[M]** Alert 8 (transformers, no patched version yet): dismiss it in GitHub code scanning as "not used in shipped code" or
   leave it open; only the maintainer can dismiss. The text's Security paragraph has a `<FILL>` for the alert state.
3. **[M]** Is the macOS build notarized? If the five signing secrets are not set, `release.yml` makes an ad hoc DMG and the
   notes must say "not notarized" (`releasing.md`, "GitHub secrets").
4. **[M]** winget: a 0.2.9 update to the existing package (pull request to `microsoft/winget-pkgs`; 472327 was the 0.2.8 first
   submission, check its state) or wait. Same for the Homebrew tap.
5. No Intel Mac is available (plan decision): the Intel DMG is checked on Apple Silicon only and the text says so.

## 1. Version bump (done, one commit)

Commit `3730002e` on the 0.2.9 branch changes: `Cargo.toml` (`[workspace.package] version = "0.2.9"`), `Cargo.lock`
(`cargo update -w --offline`: only the workspace crates move, no other crate), the `README.md` version badge, and the
`packaging/windows/installer.nsi` header comment. The workspace version is the only source (diagnostics, About, `build-windows.ps1`,
the installer and the macOS bundle read `CARGO_PKG_VERSION`). Not edited on purpose:

- `THIRD_PARTY_LICENSES.md`: no dependency changed, so `make licenses` must leave it identical; check `git diff
  THIRD_PARTY_LICENSES.md` is empty after it, and commit only if it differs.
- `CHANGELOG.md`: add the `## [0.2.9] - YYYY-MM-DD` section from the end of `release-0.2.9-github-text.md` under
  `## [Unreleased]` when the date is known (`release.yml` reads it).
- The README "Status" paragraph still describes 0.2.8 as the latest release: rewrite it on the release day, with the
  platforms and limits as stated in the final text. `docs/project/status.md` and the `CLAUDE.md` "Current state" say "0.2.9
  in preparation"; they become "published" only after step 9.
- Test fixtures that contain the string `0.2.8` (`crates/cm-regress/src/report.rs`, `hygiene.rs`) are sample data, not
  the product version; leave them.

Checks (CI runs them on the merge too): `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D
warnings`, `cargo test --workspace --locked`, `cargo deny check`, `python3 scripts/check_target_os.py`,
`cargo run -p cm-i18n -- check`.

## 2. Merge and tag

1. **[M]** Merge the 0.2.9 branch into `main`; push (`public` is the public remote); wait for `ci.yml` and Security scan.
2. **[M]** `git tag -a v0.2.9 -m "CaptureMann 0.2.9"` and `git push public v0.2.9` (never reuse a tag; if one must move,
   disable the tag ruleset for a moment only).

## 3. Build the files (three workflows, all manual; pushing a tag starts none of them)

1. **Windows:** Actions, **Windows package**, ref `v0.2.9`. Download and check:
   ```sh
   mkdir -p ~/release-0.2.9/win && gh run download <run id> -R craft-mann/capturemann -n capturemann-windows-x64 -D ~/release-0.2.9/win
   packaging/windows/release-files.sh ~/release-0.2.9/win ~/release-0.2.9/win-out
   ```
   It recomputes both SHA-256 sums, writes `SHA256SUMS.txt` and renders the winget manifest into
   `win-out/winget/manifests/c/CaptureMann/CaptureMann/0.2.9/`.
2. **macOS:** Actions, **Release**, tag `v0.2.9`. It builds both DMGs on the Apple Silicon runner (the Intel one is
   cross-built and checked under Rosetta), signs and notarizes them if the secrets exist, and opens a **draft** release with
   the DMGs and `.sha256` files (`releasing.md` steps 6 to 8). On the draft, download a DMG in a browser (quarantine flag) on
   a Mac that is not the build machine and run the DMG checks in `releasing.md` (the section with `shasum -a 256 -c` and `spctl`).
3. **Linux:** Actions, **Linux package**, the tag commit. It builds `capturemann-0.2.9-linux-{x86_64,aarch64}.tar.gz` and
   `capturemann_0.2.9_{amd64,arm64}.deb` (artifacts for 14 days; no release is created). Download both artifacts, run
   `sha256sum -c`, then **[M]** sign on your own machine: put the tarballs, `.deb` files and `.sha256` files in one folder and
   run `packaging/linux/sign-linux.sh <folder>` (writes `.minisig` next to each file and verifies it against
   `packaging/linux/minisign.pub`; the secret key never goes to CI).

## 4. Regression on the final files (the gate; hardware runs only with the maintainer's consent)

Each run uses the files built from the tag, not an earlier candidate. Store each report in `verification/`, PARTIAL unless a
person looked; the release text table is filled from them.

- **Windows (ThinkPad T490, Windows 11):** `scripts/testlab/regress.sh windows --tier standard --allow screen --package-run <run id>`,
  then `--tier hardware --allow screen,sound,mic,camera,second_monitor --installer <setup.exe> --soak-minutes 3`
  (the lab has no Samsung monitor: those cases stay skipped).
- **macOS (the Mac, `-dev` bundle id, never `/Applications` or `target/bundle`):** `scripts/testlab/regress.sh macos --tier standard`
  and, with consent for screen, microphone and camera, `--tier hardware --allow screen,sound,mic,camera`. Screen Recording
  needs the grant of the bundle that runs the test (`docs/development/building.md`). The Intel DMG is not run on Intel hardware.
- **Linux (the same ThinkPad booted into Ubuntu 24.04, X11 and GNOME Wayland):** `scripts/testlab/regress.sh linux --tier
  smoke|standard|hardware` against the `.deb` and the tarball (install, uninstall, tarball cases); recording cases skip
  without `CM_LINUX_RECORDING=1`. The ThinkPad has two systems, so Windows and Linux runs take turns.
- Manual steps for the fixes, with a stored report each if the text says anything about them: `tests/manual/I18N-IME.md`
  (Telex and VNI in the annotation editor on Windows and macOS), Reveal after an export (Explorer and Finder select the file),
  the Speakers tab at 720 x 480 and the laptop size.
- Not run, listed in the text: Windows 10, SmartScreen, mixed DPI, NVIDIA and AMD encoders, Intel Mac hardware, Linux
  recording behind `CM_LINUX_RECORDING=1`.

## 5. Fill the release text

Open `release-0.2.9-github-text.md`, replace every `<FILL: ...>` from the stored reports and the real checksums
(`win-out/SHA256SUMS.txt`, the two macOS `.sha256` files, the Linux `.sha256` files), delete a claim that has no report, and
check no line says "passed" or "verified" without one. Save the "Release body" section as `release-body.md`.

## 6. Publish the GitHub release (draft first)

The `Release` workflow already made a draft with the DMGs. Add the Windows and Linux files to the same draft and replace
its notes:

```sh
gh release upload v0.2.9 -R craft-mann/capturemann \
  ~/release-0.2.9/win/.../capturemann-0.2.9-windows-x64-setup.exe  <its .sha256> \
  ~/release-0.2.9/win/.../capturemann-0.2.9-windows-x64.zip        <its .sha256> \
  ~/release-0.2.9/win-out/SHA256SUMS.txt \
  <the four Linux files, their .sha256 and .minisig files>
gh release edit v0.2.9 -R craft-mann/capturemann --prerelease --title "CaptureMann 0.2.9: fixes on Windows, macOS and Linux" --notes-file release-body.md
```

Review the draft (the notes, the file list: 2 DMGs, 2 Windows files, 4 Linux files, their checksum and signature files),
download the Windows installer in a browser on a Windows PC (the first SmartScreen look), then **[M] publish**. A bad file is
replaced by a new tag (0.2.10), never an edited asset (`releasing.md`, "Rollback and yank").

## 7. winget (after the release is public)

On a Windows PC: `winget validate --manifest <win-out>\winget\manifests\c\CaptureMann\CaptureMann\0.2.9`, then
`winget install --manifest <that folder>` (needs `winget settings --enable LocalManifestFiles`). **[M]** Copy the folder to
`manifests/c/CaptureMann/CaptureMann/0.2.9/` in a fork of `microsoft/winget-pkgs` and open the pull request
(`New version: CaptureMann.CaptureMann version 0.2.9`). Add the `winget upgrade` line to the release body after it merges.

## 8. Homebrew cask (macOS, after the release is public)

In a checkout of `craft-mann/homebrew-tap`: `scripts/update-homebrew-cask.sh 0.2.9 <path to the tap checkout>`, commit and
push the tap. The script reads the checksums from the release's `.sha256` assets, so the release must be public first.
**Before any install test, run `brew list` and look in `/Applications`** (the Nov 4 test removed the maintainer's app); test
from a throwaway tap (`brew tap-new --no-git local/cmtest`, copy the cask in) with `HOMEBREW_NO_INSTALL_CLEANUP=1` and
`--appdir=/tmp/cm-brew-test`.

## 9. After publishing

1. **[M]** Commit to `main`: `docs/project/status.md` ("Release 0.2.9 published", tag, commit, file sums, links to the
   reports), the `CLAUDE.md` "Current state" paragraph, the README status paragraph and badge, `CHANGELOG.md` with the real
   date, `specs/plans/release-0.2.9-plan.md` (header `Status: released <date>`). Push.
2. **[M]** Update the website download links (<https://www.craftmann.app/capturemann/>) and announce.
3. Clean up: `~/release-0.2.9`, worktrees of merged agent branches and their `target/` folders.
4. Rollback: `releasing.md`, "Rollback and yank".
