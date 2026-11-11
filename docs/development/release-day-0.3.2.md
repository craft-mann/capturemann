# Release day, 0.3.2 (Windows, macOS, Linux): steps for the maintainer

Rewritten the night of Nov 11 to 12, 2026 from `release-day-0.3.1.md` for the new situation: **the candidate files were built and
regressed overnight from the commit the tag will point to** (roster and authorizations in
`specs/plans/release-0.3.2-final-morning-report.md`; its running log says how far the night got). "[M]" = only the maintainer does
this (push, tag, sign, publish, accounts, decisions). Run commands from the repo root. Text to publish: `release-0.3.2-github-text.md`
(its `<FILL>` items need the final files and their reports). Plan: `specs/plans/release-0.3.2-plan.md`; board:
`specs/plans/release-0.3.2-status.md`. Nothing in this file has been run on the release day yet.

**The gate (plan, "Release gate"):** CI green on all three systems and the Security scan on the release commit;
`exp4_ac4_cancel_deletes_partial_output` passed on five consecutive `windows-tests.yml` runs on that commit; a stored `cm-regress`
report per system (candidate files, then the final-file check of section 4); the release text with no `<FILL>` left that the
reports can fill. A report on candidate files says "candidate files of commit <sha>", never "final files".

## Before the day: decisions

1. **[M] winget.** Pull requests 472327 (`CaptureMann.CaptureMann` 0.2.8) and 477195 (0.3.1) to `microsoft/winget-pkgs` were both still
   open on Nov 11, 2026. If neither is merged on the release day, 0.3.2 ships **without winget** (wording in the release text, Install
   section); do not open a third pull request on top of two open ones unless the winget moderators asked for it. If one merged, see
   section 7.
2. **[M] macOS notarization.** The `Release` workflow needs the five signing secrets (`releasing.md`, "GitHub secrets"); without them it
   makes an ad hoc DMG and the text must say "not notarized". 0.3.1 used notarized DMGs built on the maintainer's Mac with `make release`
   and `make release CM_ARCH=x86_64`; that path is still open (section 3).
3. **[M] Plugin submission.** The release does not wait for Anthropic's review; the plugin installs from this repository's marketplace as
   soon as `main` is pushed. Submit after the release (section 7) or later; the steps are `plugin-submission-0.3.2.md`.
4. **[M] Windows signing.** 0.3.2 ships **unsigned** unless SignPath Foundation was set up before the day (`windows-signing-options.md`);
   the release text carries the unsigned-case paragraph (Known limits).
5. No Intel Mac is available: the Intel DMG is checked on Apple Silicon only and the text says so.

## 1. Check what the night left

1. Read `specs/plans/release-0.3.2-final-morning-report.md` (running log, gate table) and `git log --oneline v0.3.1..HEAD | head -60`.
2. Which commit is the release commit (the one the candidate files were built from and the reports name). Every report in
   `verification/` named `L1-*-0.3.2-*` must name it; a report on another commit is not evidence for this release.
3. Checks on the merged tree (CI runs them too): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   `cargo test --workspace --locked`; `cargo deny check`; `python3 scripts/check_target_os.py`; `python3 scripts/check_no_bare_command.py`;
   `cargo run -p cm-i18n -- check`; `python3 scripts/check_plugin_version.py` (the plugin manifest says 0.3.2); `make licenses` leaves
   `THIRD_PARTY_LICENSES.md` unchanged (or the regenerated file is committed before the tag). If the night ran these on the release
   commit and nothing changed since, the morning report says so and this step is a read.
4. **[M]** Push the local commits (docs and reports of the night) to `public/main`: `git push public main`. Wait for `ci.yml` and the
   Security scan on macOS, Windows and Linux. A docs-only commit on top of the release commit is fine for the tag **only if** the tag goes
   on the commit the files were built from (0.3.0 did this: tag on `af9b7600`, files from `6cdcb033`, and the text says both); simpler:
   tag the release commit itself and let the docs commits sit above it.
5. **Windows `exp4_ac4`:** the morning report lists the five consecutive green `windows-tests.yml` runs on the release commit. If fewer,
   `gh workflow run windows-tests.yml -r main` until five in a row have passed. Not before the tag.

## 2. Version and documents

The workspace version is 0.3.2 (`Cargo.toml`, `Cargo.lock`, commit `eb89c833`), the README badge says 0.3.2, the MCP tool count is 81
(README, `compare/`, `crates/cm-mcp/tests/stdio.rs`), `plugins/capturemann/.claude-plugin/plugin.json` says 0.3.2. On the release day,
when the date is known: `CHANGELOG.md` (turn `## [Unreleased]` into `## [0.3.2] - <date>`; `release.yml` reads it), the README status
paragraphs (the Windows section still describes 0.2.8 and the Linux section 0.3.0: say 0.3.2, name the new features, keep "not signed"),
`docs/project/status.md` (the Release 0.3.2 entry has `<date>` placeholders), the "Current state" of `CLAUDE.md` (0.3.2 "released"),
`specs/plans/release-0.3.2-plan.md` (header `Status: released <date>`). **The CHANGELOG date must be in the tagged commit** if the
`Release` workflow is used (it reads the section for the tag); otherwise a docs commit after the tag is fine.

## 3. Tag and build the final files (pushing a tag starts no workflow)

1. **[M] Tag:** `git tag -a v0.3.2 -m "CaptureMann 0.3.2"` on the release commit and `git push public v0.3.2` (never reuse a tag; a tag is
   moved only while nothing is public, and the "Protect release tags" ruleset has to be disabled for it, as for 0.2.5).
2. **Windows:** Actions, **Windows package**, ref `v0.3.2`. Then:
   ```sh
   mkdir -p ~/release-0.3.2/win && gh run download <run id> -R craft-mann/capturemann -n capturemann-windows-x64 -D ~/release-0.3.2/win
   packaging/windows/release-files.sh ~/release-0.3.2/win ~/release-0.3.2/win-out
   ```
   It recomputes both SHA-256 sums, writes `SHA256SUMS.txt` and renders the winget manifest (kept for section 7).
3. **macOS:** either Actions, **Release**, tag `v0.3.2` (both DMGs on the Apple Silicon runner, signed and notarized when the secrets exist,
   a **draft** release with the DMGs and `.sha256` files), or, as 0.3.1 did, on this Mac from a checkout of the tag: `make release` and
   `make release CM_ARCH=x86_64` (first `CM_ARCH=x86_64 make ffmpeg llama sherpa` for the Intel build), then rename
   `CaptureMann-0.3.2-<arch>.dmg` to `CaptureMann-v0.3.2-macos-<arch>.dmg` and write the `.sha256` files again with the new names. The
   night's `mac-release` agent left candidate DMGs in `~/release-0.3.2/macos`; they are candidates, not the final files, unless the
   morning report says they were built from the tagged commit with the release identity and you decide to use them as the final files
   (then they are the files the reports describe, and section 4 is a checksum comparison). Download a DMG in a browser on a Mac that is not
   the build machine and run the DMG checks of `releasing.md` (`shasum -a 256 -c`, `spctl`, `stapler validate`).
4. **Linux** (x86_64 on the ThinkPad in Ubuntu, aarch64 in the VM on this Mac; the lab switch is `scripts/testlab/lab.sh switch ubuntu`):
   `packaging/linux/release-build.sh --src <checkout of v0.3.2> --commit v0.3.2 --jobs 3` (about an hour on the laptop, warm 20 min), then
   `packaging/linux/check-package.sh`. The candidates are in `~/release-0.3.2/linux/{x86_64,aarch64}`. **[M]** sign the final files:
   `packaging/linux/sign-linux.sh <folder>` (minisign; the secret key never goes to CI; the public key line goes into the text).

## 4. Re-check the final files against the candidates

The candidate reports were made on files built from the same commit, so the final files only need proof that they are the same build:

1. Compare SHA-256 sums of each final file with its candidate (`shasum -a 256`, `certutil -hashfile <file> SHA256` on Windows). A signed or
   notarized macOS DMG and a CI-built Windows package are **not expected** to be byte-identical; the Linux tarballs may or may not be.
2. **Same sum:** the candidate report covers the file; write "final file, same SHA-256 as the candidate of commit <sha>" into the release
   text table.
3. **Different sum:** run a quick smoke on the final file and store it as a final-file report, named `L1-<system>-0.3.2-regression-smoke-final-*`:
   - Windows: `scripts/testlab/regress.sh windows --tier smoke --package-run <Windows package run id on the tag> --run latest --commit <sha>`
   - macOS: `scripts/testlab/regress.sh macos --tier smoke --installer <the final .dmg> --commit <sha>` (never `/Applications` or
     `target/bundle` while the app runs)
   - Linux: `scripts/testlab/regress.sh linux --tier smoke` on the installed final tarball (and the `.deb` install case), the aarch64 VM `smoke`.
   Then the table names the candidate reports (standard, full) plus the smoke report of the final file. If a smoke run fails, the release
   waits: find the cause, and a fix means a new release commit, a new tag (`v0.3.2` is not reused once pushed: use 0.3.3 or `v0.3.2` only if
   it was never public) and new candidates.
4. Hardware tiers (screen, sound, microphone, camera) run only with your consent and are not part of the gate; 0.3.1 ran macOS
   `hardware` with consent. Not run and listed in the text: Intel Mac hardware, Windows 10, SmartScreen after "Keep", mixed DPI, NVIDIA and
   AMD encoders, KDE and other distributions, a second monitor, a 16 GB Mac, the keyboard check of the shortcut fixes on Windows and
   Linux X11 (`tests/manual/SHORTCUTS-1.md`), Copy text on real screenshots and on the Windows and Linux recognizers (`tests/manual/ANN-5.md`),
   the plugin session (`tests/manual/PLUGIN-1.md`).

## 5. Fill the release text

In `release-0.3.2-github-text.md` replace every `<FILL ...>`: the release commit sha, the report names (section 4), the counts per tier,
the SHA-256 table (`win-out/SHA256SUMS.txt`, the two macOS `.sha256` files, the Linux `SHA256SUMS.txt`), the minisign public key line
(`packaging/linux/minisign.pub`), notarization, the winget sentence, the Homebrew line (tap commit after section 7), the plugin submission
sentence, the APT repository versions, the licences sentence (`git diff v0.3.1 -- THIRD_PARTY_LICENSES.md`), the CHANGELOG date. Delete a
claim that has no report; check no line says "passed" or "verified" without one. Save the "Release body" section as `release-body.md`
(not committed; it is the GitHub notes).

## 6. Publish the GitHub release (draft first)

If the `Release` workflow ran, it made the macOS draft; otherwise `gh release create v0.3.2 -R craft-mann/capturemann --draft --prerelease
--title "CaptureMann 0.3.2: copy text from a screenshot, a Claude plugin" --notes-file release-body.md` with the two DMGs and their
`.sha256`. Then add the rest and set the notes:

```sh
gh release upload v0.3.2 -R craft-mann/capturemann \
  ~/release-0.3.2/win/.../capturemann-0.3.2-windows-x64-setup.exe <its .sha256> \
  ~/release-0.3.2/win/.../capturemann-0.3.2-windows-x64.zip       <its .sha256> \
  <the four Linux packages with .sha256 and .minisig> ~/release-0.3.2/win-out/SHA256SUMS.txt
gh release edit v0.3.2 -R craft-mann/capturemann --prerelease \
  --title "CaptureMann 0.3.2: copy text from a screenshot, a Claude plugin" --notes-file release-body.md
```

Review the draft (notes; files: 2 DMGs, 2 Windows files, 4 Linux packages, their checksum and signature files, `SHA256SUMS.txt`), download
one file per system from the draft in a browser and check its `.sha256` (and the `.minisig` for a Linux file), then **[M] publish**. **A bad
file is replaced by a new tag (0.3.3), never an edited asset**; rollback steps: `releasing.md`, "Rollback and yank" (a draft is deleted with
its tag; a published broken release is turned back into a draft or marked pre-release with a note at the top; assets others may have
checksums for are deleted only when dangerous).

## 7. After the release is public

- **APT [M]:** `make-apt-repo.sh` with the `.deb` files of every version the repository should carry (0.3.0, 0.3.1 and 0.3.2; the script keeps
  what you give it), upload to release `apt` with `--clobber` (`release-day-0.3.0.md`, section 5; `linux-apt.md`), then on a clean Ubuntu
  24.04 (VM or container) the four install lines of the release text, `apt update`, `apt install capturemann`, `capturemann --version` says
  0.3.2. A report of that run in `verification/` closes the "APT on a clean Ubuntu" item that 0.3.1 left open.
- **winget [M]:** the two open pull requests (472327 for 0.2.8, 477195 for 0.3.1) are the blocker. Check their state
  (`gh pr view 472327 -R microsoft/winget-pkgs`, same for 477195). (a) Both still open: leave them, do not open a third; the release text
  says 0.3.2 is not in winget. (b) 472327 merged, 477195 open: push the 0.3.2 manifest as a new commit on the 477195 branch or close 477195
  and open `New version: CaptureMann.CaptureMann version 0.3.2` from the rendered manifest (`winget validate --manifest
  <win-out>\winget\manifests\c\CaptureMann\CaptureMann\0.3.2` on the ThinkPad first), as the moderators prefer. (c) Both merged: open the
  0.3.2 pull request. Record the state in the release text and the README.
- **Homebrew cask [M]:** in a checkout of `craft-mann/homebrew-tap`: `scripts/update-homebrew-cask.sh 0.3.2 <tap checkout>`, commit, push (the
  release must be public first; the cask expects `CaptureMann-v0.3.2-macos-<arch>.dmg`). **Before any install test run `brew list` and look
  in `/Applications`**; test from a throwaway tap with `--appdir=/tmp/cm-brew-test`, as the 0.3.1 report did. Put the tap commit in the text.
- **Claude plugin [M]:** after `main` is public, `claude plugin marketplace add craft-mann/capturemann` and
  `claude plugin install capturemann@capturemann-plugins` on one computer against the installed 0.3.2 app (`tests/manual/PLUGIN-1.md`, 30
  minutes, free read-only tools only), report in `verification/`. Then the directory submission if you choose to do it now:
  `plugin-submission-0.3.2.md` (step by step; expect a reviewer hold because the server is the app's own binary; Variant B if the portal
  blocks `${user_config}`). Update the plugin sentence in the release text and the README either way.
- **Website and announcement [M]:** download links at <https://www.craftmann.app/capturemann/>.
- **Housekeeping (the next session can do most of it; you push):** `CHANGELOG.md` date (if not in the tag), `docs/project/status.md`
  (replace `<date>`, add the tag commit, run ids, tap commit, winget state), `CLAUDE.md` "Current state" (0.3.2 released, 0.3.3 planned),
  README (status paragraphs, Windows and Linux sections to 0.3.2, the plugin section's version), `specs/plans/release-0.3.2-plan.md` header
  (`Status: released <date>`), move the open items to `specs/plans/release-0.3.3-plan.md`, delete `~/release-0.3.2` and the night's
  worktrees and their `target/` folders (the disk filled up on Nov 5, 2026), `git worktree prune`.
- Rollback: `releasing.md`, "Rollback and yank".
