# Release day, 0.3.3 (Windows, macOS, Linux): steps for the maintainer

Written on Nov 12, 2026 from `release-day-0.3.2.md`, which is the pattern: the same steps with the version changed, plus the checks
that belong to the annotation pack. "[M]" = only the maintainer does this (push, tag, sign, publish, accounts, decisions). Run
commands from the repo root. Text to publish: `release-0.3.3-github-text.md` (its `<FILL>` items need the final files and their reports;
the lines marked OPTIONAL are deleted unless the feature is in the tagged commit). Plan: `specs/plans/release-0.3.3-plan.md`. Nothing in
this file has been run on a release day yet. **Blanks to fill on the day are written `<FILL: ...>`, `<date>`, `<sha>`, `<run id>`.**

**The gate:** CI green on all three systems and the Security scan on the release commit (run the Trivy gate and `cargo deny check`
locally before the merge to `main`); a stored `cm-regress` report per system on the candidate files (standard and full, including the new
cases `annotate.pack` and `import.images`, which must be PASS, not SKIPPED: a SKIPPED result means the package under test is older than
0.3.3); the editor script `tests/manual/ANN-6.md` and the import script `tests/manual/IMP-6.md` run by the maintainer on macOS at least,
with reports in `verification/`; the release text with no `<FILL>` left that the reports can fill. A report on candidate files says
"candidate files of commit <sha>", never "final files".

## Before the day: decisions

1. **[M] Cut list.** Stamps (ANN-4, key U) and clipboard paste of a picture (IMP-6 stretch goal) are the first things cut. Look at what
   `main` contains and delete the OPTIONAL lines of the release text, the stamp lines of `tests/manual/ANN-6.md` (section E becomes
   "not built") and of the plugin skill (`plugins/capturemann/skills/screenshot-and-redact/SKILL.md`, `docs/user/user-guide.md`) for
   anything that was cut. The op `stamp` is frozen in the spec, so if only the editor part is missing, say "the `stamp` op works through
   `annotate`; the editor has no stamp tool yet".
2. **[M] winget.** Pull requests 472327 (0.2.8) and 477195 (0.3.1) to `microsoft/winget-pkgs` were still open on Nov 12, 2026. Check them
   (`gh pr view 472327 -R microsoft/winget-pkgs`, same for 477195). If neither is merged, 0.3.3 ships without winget (wording in the text);
   do not open a third pull request on top of two open ones unless the moderators asked.
3. **[M] macOS notarization.** As 0.3.2: the `Release` workflow needs the five signing secrets (`releasing.md`); the DMGs of 0.3.2 were built
   on the maintainer's Mac with `make release` and `make release CM_ARCH=x86_64` and notarized. Without notarization the text says so.
4. **[M] Plugin.** The plugin version follows the app version (`python3 scripts/check_plugin_version.py`); the skill text changed in 0.3.3.
   The directory submission is still held until the organization is Verified (`plugin-submission-0.3.2.md`); update the plugin sentence only
   if that changed.
5. **[M] Windows signing.** 0.3.3 ships unsigned unless SignPath Foundation was set up (`windows-signing-options.md`).
6. No Intel Mac is available: the Intel DMG is checked on Apple Silicon only and the text says so.

## 1. Check the tree

1. `git log --oneline v0.3.2..HEAD | head -60`. Which commit is the release commit (the one the candidate files are built from)? Every
   report in `verification/` named `L1-*-0.3.3-*` must name it.
2. Checks on the merged tree (CI runs them too): `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   `cargo test --workspace --locked`; `cargo deny check`; `python3 scripts/check_target_os.py`; `python3 scripts/check_no_bare_command.py`;
   `python3 scripts/check_md_links.py`; `cargo run -p cm-i18n -- check` (the new editor strings, with the Vietnamese drafts marked for
   review); `python3 scripts/check_plugin_version.py`; `make licenses` leaves `THIRD_PARTY_LICENSES.md` unchanged (or the regenerated file
   is committed before the tag).
3. **Compatibility fixture.** The test that a 0.3.3 manifest (step `style`, `start`, `text`, plus an unknown field) survives a round trip
   through the library code and that a 0.3.2-style step draws as before (ANN-6.AC5, AC6) is in `cargo test --workspace`; confirm it ran.
   Then once by hand: open a capture saved by 0.3.3 with a callout and labelled steps in the **0.3.2 app** (keep a 0.3.2 DMG or build for the
   check), and confirm the warning "Kept an annotation this version cannot show" and that a save in 0.3.2 loses only the step label, style and
   start. The release text promises exactly this.
4. **[M]** Push local commits to `public/main` and wait for `ci.yml` and the Security scan. Tag the release commit itself and let docs
   commits sit above it (or tag the commit the files were built from and say both in the text, as 0.3.0 did).
5. Windows `exp4_ac4`: the five consecutive green `windows-tests.yml` runs rule of 0.3.2 applies if it is still in force (check the plan).

## 2. Version and documents

Bump the workspace version to 0.3.3 (`Cargo.toml`, `Cargo.lock`), the README badge, `plugins/capturemann/.claude-plugin/plugin.json` and the
marketplace entry. The MCP tool count stays 81. When the date is known: `CHANGELOG.md` (`## [Unreleased]` becomes `## [0.3.3] - <date>`; the
section must be in the tagged commit if the `Release` workflow is used), the README status paragraphs (editor and import rows: the import row
says "Import MP4, MOV, MKV and other formats" and gains the pictures), `docs/project/status.md` (Release 0.3.3 entry with `<date>`), the
"Current state" of `CLAUDE.md`, the header of `specs/plans/release-0.3.3-plan.md` (`Status: released <date>`), the Status lines of ANN-4,
ANN-6, ANN-7 and IMP-6 (`In progress (...)` or `Ready` only as the wording rule allows; "verified" only with a report).

## 3. Candidates, tag and the final files

1. **Candidates.** Build the candidate files from the release commit on each system as for 0.3.2 (`release-day-0.3.2.md`, section 3 and
   `specs/plans/release-0.3.2-final-morning-report.md` for the roster). Windows: the **Windows package** workflow; macOS: `make release`
   and `make release CM_ARCH=x86_64`; Linux: `packaging/linux/release-build.sh --src <checkout> --commit <sha> --jobs 3` on the ThinkPad
   (x86_64) and in the aarch64 VM, then `packaging/linux/check-package.sh`.
2. **Regression per system** on the candidate files: `scripts/testlab/regress.sh windows --tier standard --allow screen`, the macOS and Linux
   equivalents (`docs/development/regression.md`), then `--tier full`. Store the reports as `L1-<system>-0.3.3-regression-*`. In the standard
   tier look for `annotate.pack` and `import.images`: both must be PASS. Mac check of the CLI/MCP cases from a build tree:
   `cargo build -p cm-daemon -p cm-cli -p cm-mcp -p cm-regress && target/debug/cm-regress --dev --package-dir target/debug --tier standard`.
3. **Manual scripts [M]:** `tests/manual/ANN-6.md`, `tests/manual/IMP-6.md` and `tests/manual/TEXT-FIELDS.md` (the new text field look) on the macOS candidate (use `make run-bundle-dev`; never
   `/Applications` while the release app runs), reports `verification/ANN-6-<date>-maintainer.md`, `IMP-6-<date>-maintainer.md` and `TEXT-FIELDS-<date>-maintainer.md`. Run on
   Windows 11 and Linux if time allows; otherwise the text lists it under "Not run".
4. **[M] Tag:** `git tag -a v0.3.3 -m "CaptureMann 0.3.3"` on the release commit and `git push public v0.3.3` (never reuse a tag; pushing a
   tag starts no workflow). Then build the final files from the tag exactly as in `release-day-0.3.2.md`, section 3 (Windows package run on
   `v0.3.3` and `packaging/windows/release-files.sh`; the macOS DMGs; Linux `release-build.sh` and **[M]** `packaging/linux/sign-linux.sh`).
5. **Candidates against final files:** compare SHA-256 sums; same sum means the candidate reports cover the file; different sums need a
   smoke run on the final file, stored as `L1-<system>-0.3.3-regression-smoke-final-*` (commands in `release-day-0.3.2.md`, section 4). A
   failed smoke run stops the release.
6. Not run and listed in the text: Intel Mac hardware, Windows 10, SmartScreen after "Keep", mixed DPI, NVIDIA and AMD encoders, KDE and
   other distributions, a second monitor, a 16 GB Mac, `ANN-6.md` and `IMP-6.md` on the systems where you did not run them, the Linux `.deb`
   through APT on a clean computer, a phone photo from a real phone on Windows and Linux.

## 4. Fill the release text and publish (draft first)

1. In `release-0.3.3-github-text.md` replace every `<FILL ...>` (commit sha, report names, counts per tier, the SHA-256 table, the minisign key
   line, notarization, the winget sentence, the Homebrew line, the plugin sentence, the APT versions, the licences sentence from a diff of
   `Cargo.lock` and `THIRD_PARTY_LICENSES.md` against `v0.3.2`, the CHANGELOG date) and delete the OPTIONAL lines of anything not built.
   Check no line says "passed" or "verified" without a stored report. Save the "Release body" section as `release-body.md` (not committed).
2. Create the draft: `gh release create v0.3.3 -R craft-mann/capturemann --draft --prerelease --title "CaptureMann 0.3.3: callouts, spotlight and step labels, import pictures, clearer text fields" --notes-file release-body.md`
   (or use the macOS draft the `Release` workflow made), upload the files as in `release-day-0.3.2.md` section 6 (2 DMGs, 2 Windows files,
   4 Linux packages, their `.sha256` and `.minisig`, `SHA256SUMS.txt`), download one file per system in a browser and check it, then **[M] publish**.
   A bad file is replaced by a new tag (0.3.4), never an edited asset; rollback is in `releasing.md`, "Rollback and yank".

## 5. After the release is public

- **APT [M]:** `make-apt-repo.sh` with the `.deb` files of 0.3.0, 0.3.1, 0.3.2 and 0.3.3, upload to release `apt` with `--clobber`, then on a clean
  Ubuntu 24.04 install from the repository and check `capturemann --version` says 0.3.3 (`release-day-0.3.2.md` section 7, `linux-apt.md`).
- **winget [M]:** check the two open pull requests and follow the three cases of `release-day-0.3.2.md` section 7.
- **Homebrew cask [M]:** `scripts/update-homebrew-cask.sh 0.3.3 /Users/trungtran/source/repos/homebrew-tap` (never a temporary clone), commit, push,
  verify the push through the API; before any install test run `brew list` and look in `/Applications`.
- **Claude plugin [M]:** `tests/manual/PLUGIN-1.md` against the installed 0.3.3 app (the skill text changed); report in `verification/`.
- **Housekeeping:** `CHANGELOG.md` date (if not in the tag), `docs/project/status.md`, `CLAUDE.md` "Current state", the README, the plan header, then
  move the open items (Magnify, in-app update, windowed audio, headless daemon, WebP and HEIC import) to `specs/plans/release-0.3.4-plan.md`, remove the
  blue "new" dots from the editor in 0.3.4 as decided, delete `~/release-0.3.3` and the worktrees of merged agents with their `target/` folders,
  and `git worktree prune`.
- Rollback: `releasing.md`, "Rollback and yank".
