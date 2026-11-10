# Release day, 0.3.1 (Windows, macOS, Linux): steps for the maintainer

Rewritten Nov 10, 2026 from `release-day-0.2.9.md` (Windows, macOS) and `release-day-0.3.0.md` (Linux, APT). "M" = only the
maintainer does this (push, tag, sign, publish, accounts, decisions). Run commands from the repo root. Text to publish:
`release-0.3.1-github-text.md` (its `<FILL>` items need the final files and their reports). Plan: `specs/plans/release-0.3.1-plan.md`.
**0.3.1 covers all three systems, so it is not tagged until CI is green on all three, `exp4_ac4` has passed on five consecutive
Windows test runs and a regression of the final files is stored for each system** (plan, "Freeze and release gate"). Nothing in this file
has been run on the release day yet.

## Before the day: decisions

1. **[M] winget.** Pull request 472327 (`CaptureMann.CaptureMann` 0.2.8) was still open on Nov 10, 2026. So 0.3.1 ships **without
   winget** again, with the wording of `release-0.2.9-winget-note.md` and "0.3.1" for the newer version; the update pull request
   follows 472327 (section 7).
2. **[M] macOS notarization.** If the five signing secrets are not set, `release.yml` makes an ad hoc DMG and the text must say "not
   notarized" (`releasing.md`, "GitHub secrets").
3. **[M] The Vietnamese-only guard** (`ai.drop_unsupported_lines`, on by default for Vietnamese transcripts) is decided and written in the
   text. Nine of the 12 drops of the 4 h file were not listened to: keep or soften the sentence in the text if you listen to more.
4. No Intel Mac is available: the Intel DMG is checked on Apple Silicon only and the text says so.

## 1. Gate before the tag

1. Checks on the merged tree (CI runs them too): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   `cargo test --workspace --locked`; `cargo deny check`; `python3 scripts/check_target_os.py`; `python3 scripts/check_no_bare_command.py`;
   `cargo run -p cm-i18n -- check`. The Mac cannot lint the Windows target, so after the push read the Windows clippy step of CI (the instant-editor merge broke it once).
2. **[M]** Push `main` to `public`; wait for `ci.yml` and the Security scan on macOS, Windows and Linux.
3. **Windows `exp4_ac4`**: `gh workflow run windows-tests.yml -r main`, repeated until
   `exp4_ac4_cancel_deletes_partial_output` has passed on **five consecutive runs** (the real-ffmpeg step of `ci.yml` is the fix). Not before the tag.
4. The reports of the night were made on the dev bundle and CI packages of earlier commits; they do not replace section 3.

## 2. Version and documents

The workspace version is already 0.3.1 (`Cargo.toml`, `Cargo.lock`). On the release day, when the date is known: `CHANGELOG.md` (turn
`## [Unreleased]` into `## [0.3.1] - <date>`; `release.yml` reads it), the README status paragraph and badge, `docs/project/status.md`,
the "Current state" of `CLAUDE.md`, `specs/plans/release-0.3.1-plan.md` (header `Status: released <date>`). `make licenses` must leave
`THIRD_PARTY_LICENSES.md` identical (no dependency changed since 0.3.0 except the vendored whisper-rs-sys patch; check `git diff`).

## 3. Build the final files (after the tag; pushing a tag starts no workflow)

1. **[M] Tag:** `git tag -a v0.3.1 -m "CaptureMann 0.3.1"` and `git push public v0.3.1` on the commit that passed the gate (never reuse a tag).
2. **Windows:** Actions, **Windows package**, ref `v0.3.1`. Then:
   ```sh
   mkdir -p ~/release-0.3.1/win && gh run download <run id> -R craft-mann/capturemann -n capturemann-windows-x64 -D ~/release-0.3.1/win
   packaging/windows/release-files.sh ~/release-0.3.1/win ~/release-0.3.1/win-out
   ```
   It recomputes both SHA-256 sums, writes `SHA256SUMS.txt` and renders the winget manifest (kept for section 7).
3. **macOS:** Actions, **Release**, tag `v0.3.1`: both DMGs on the Apple Silicon runner (Intel cross-built, checked under Rosetta), signed and
   notarized when the secrets exist, a **draft** release with the DMGs and `.sha256` files. Download a DMG in a browser on a Mac that is not the
   build machine and run the DMG checks of `releasing.md` (`shasum -a 256 -c`, `spctl`).
4. **Linux** (x86_64 on the ThinkPad in Ubuntu, aarch64 in the VM on this Mac):
   `packaging/linux/release-build.sh --src <checkout of v0.3.1> --commit v0.3.1 --jobs 3` (about an hour on the laptop, warm 20 min), then
   `packaging/linux/check-package.sh`. **[M]** sign: `packaging/linux/sign-linux.sh <folder>` (minisign; the secret key never goes to CI).

## 4. Regression on the final files (the gate; hardware runs only with the maintainer's consent)

Store each report in `verification/` (PARTIAL unless a person looked); the release text table is filled from them.

- **Windows (ThinkPad, Windows 11):** `scripts/testlab/regress.sh windows --tier standard --package-run <package run id> --run <tests run id> --commit <sha>`,
  then `--tier full`, then `--only ai.transcribe,ai.speakers,installer.silent` with `--whisper-model`, `--speech`, `--speaker-models`, `--installer`
  (see the header of `regress.sh`); `--tier hardware --allow screen,sound,mic,camera --soak-minutes 3` with consent.
- **macOS (the dev-signed copy, never `/Applications` or `target/bundle` while the app runs):** `scripts/testlab/regress.sh macos --tier standard`,
  `--tier full`, and with consent `--tier hardware --allow screen,sound,mic,camera`. Run `make release` for both architectures and check the DMG.
- **Linux (ThinkPad in Ubuntu, X11):** `scripts/testlab/regress.sh linux --tier standard|full` on the installed tarball and on the `.deb`; the
  aarch64 VM `full`. Heavy estimate test (`estimate_vs_real.rs`, plan section 5) on Windows (not yet run) and Linux; store in `verification/benchmarks/`.
- Manual scripts that the text mentions: `tests/manual/TRN-PROGRESS.md`, `VOC-2.md`, `AI-GUARD.md`, `EXP-5.md` (Windows and Linux are not run).
- Not run, listed in the text: Intel Mac hardware, Windows 10, SmartScreen, mixed DPI, NVIDIA and AMD encoders, KDE and other distributions,
  a second monitor, long recordings on Windows and Linux, a 16 GB Mac.

## 5. Fill the release text

In `release-0.3.1-github-text.md` replace every `<FILL ...>`: the final-file report names (section 4), the SHA-256 table (`win-out/SHA256SUMS.txt`,
the two macOS `.sha256` files, the Linux `SHA256SUMS.txt`), the minisign public key line, and the winget and Homebrew lines. Delete a claim that has
no report; check no line says "passed" or "verified" without one. Save the "Release body" section as `release-body.md`.

## 6. Publish the GitHub release (draft first)

The `Release` workflow made the macOS draft. Add the Windows and Linux files to it and replace its notes:

```sh
gh release upload v0.3.1 -R craft-mann/capturemann \
  ~/release-0.3.1/win/.../capturemann-0.3.1-windows-x64-setup.exe <its .sha256> \
  ~/release-0.3.1/win/.../capturemann-0.3.1-windows-x64.zip       <its .sha256> \
  <the four Linux packages with .sha256 and .minisig> ~/release-0.3.1/win-out/SHA256SUMS.txt
gh release edit v0.3.1 -R craft-mann/capturemann --prerelease \
  --title "CaptureMann 0.3.1: export sizes, MP3 import, transcript progress" --notes-file release-body.md
```

Review the draft (notes; files: 2 DMGs, 2 Windows files, 4 Linux packages, their checksum and signature files), download the Windows installer
in a browser on a Windows PC (the SmartScreen look), then **[M] publish**. A bad file is replaced by a new tag (0.3.2), never an edited asset.

## 7. After the release is public

- **APT [M]:** `make-apt-repo.sh` with both `.deb` files, upload to release `apt` (`--clobber`), test on a clean Ubuntu 24.04 (`release-day-0.3.0.md`, APT section).
- **winget [M]:** wait for 472327 (0.2.8); then `winget validate --manifest <win-out>\winget\manifests\c\CaptureMann\CaptureMann\0.3.1`, copy to a fork of
  `microsoft/winget-pkgs`, pull request `New version: CaptureMann.CaptureMann version 0.3.1`.
- **Homebrew cask [M]:** in a checkout of `craft-mann/homebrew-tap`: `scripts/update-homebrew-cask.sh 0.3.1 <tap checkout>`, commit, push (the release must be
  public first). **Before any install test run `brew list` and look in `/Applications`**; test from a throwaway tap with `--appdir=/tmp/cm-brew-test`.
- **Website and announcement [M]:** download links at <https://www.craftmann.app/capturemann/>.
- Update `status.md`, `CLAUDE.md`, README, `CHANGELOG.md` (section 2) with the real date and commit; clean up `~/release-0.3.1` and old worktrees.
- Rollback: `releasing.md`, "Rollback and yank".
