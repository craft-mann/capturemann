# Release day, 0.3.1 (Windows, macOS, Linux): steps for the maintainer

Written the night of Nov 9 to 10, 2026 after `release-day-0.3.0.md` and `release-day-0.2.9.md`. "M" = only the maintainer does
this. Text: `release-0.3.1-github-text.md`. What the night did: `specs/plans/release-0.3.1-morning-report.md`.
**0.3.1 is the first release that covers all three systems, so it is not published until CI is green on all three and the
regression of the final files is stored for all three** (plan, "Freeze and release gate").

1. **Read** the morning report and `git log --oneline 97ea4b8a..HEAD` (the night's commits are not pushed).
2. **Checks** on the merged tree: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`;
   `cargo test --workspace --locked`; `cargo deny check`; `python3 scripts/check_target_os.py`; `cargo run -p cm-i18n -- check`.
3. **[M] Push** `main` to `public` and wait for `ci.yml` and the security scan on macOS, Windows and Linux.
4. **Windows exp4_ac4**: run **Windows tests** (`gh workflow run windows-tests.yml -r main`) until
   `exp4_ac4_cancel_deletes_partial_output` has passed on **five consecutive runs** (plan section 5). Do not tag before.
5. **Final files**: Windows (`Windows package` workflow, then `scripts/testlab/regress.sh windows --tier standard|full --package-run <id> --run <tests id> --commit <sha>`),
   macOS (`make release`, both architectures; `scripts/testlab/regress.sh macos --tier full --allow screen,mic,camera,sound --package-dir <...>`),
   Linux (`packaging/linux/release-build.sh --src <checkout> --commit <sha> --jobs 3` on the laptop and in the VM, then the Linux
   regression and `check-package.sh`). Store each report in `verification/`; the night's reports were made on the dev bundle and
   the CI package of an earlier commit, so they do not replace these.
6. **Heavy estimate test** on Windows and Linux (`estimate_vs_real.rs`, plan section 5) and store the report in `verification/benchmarks/`.
7. **[M] Sign** (minisign for Linux files, `packaging/linux/sign-linux.sh`), checksums (`packaging/windows/release-files.sh`).
8. **[M] Tag** `git tag -a v0.3.1 -m "CaptureMann 0.3.1"` and `git push public v0.3.1`, on the commit the files were built from.
9. **[M] Draft pre-release** (`gh release create v0.3.1 --draft --prerelease -R craft-mann/capturemann --title ... --notes-file <body> <files>`), fill the
   `<FILL>` items of the text, download one file per system, then publish.
10. **[M] APT**: `make-apt-repo.sh` with both `.deb`, upload to release `apt` (`--clobber`), test on a clean Ubuntu 24.04.
11. **[M] winget** PR, Homebrew cask, website download page; then set `Status: released` in the plan, update `docs/project/status.md` and the
    "Current state" of `CLAUDE.md`.
