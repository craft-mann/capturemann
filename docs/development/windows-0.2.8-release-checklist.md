# Windows 0.2.8: release checklist (for the maintainer)

Short form of Nov 5, 2026. **The exact steps (commands, file edits, the version-bump table, winget, checks) are in
[`release-day-0.2.8.md`](release-day-0.2.8.md); follow that file.** This one keeps the order, the reasons and what
only the maintainer does. Text to publish: `release-0.2.8-github-text.md`. General process: `releasing.md`.
Decisions already taken (Nov 5, 2026): no code signing in 0.x (O4), recording ships (O5), zip and NSIS installer (O7).

1. **Review `track/windows`** and the open points in the release text. *Maintainer only.*
2. **Bump the version to 0.2.8** in one commit (table in release-day step 1). The workspace version is the only source;
   a package built before the bump says 0.2.7.
3. **Merge `track/windows` into `main`**, wait for CI. *Maintainer only.*
4. **Tag** `v0.2.8` (never reuse a tag) and push it. *Maintainer only.*
5. **Build the Windows files** with the "Windows package" workflow on the tag. *Maintainer only.*
6. **Download the artifact and check the sums** with `packaging/windows/release-files.sh` (release-day step 3).
7. **Run the regression suite on the lab laptop** against that package (release-day step 4) and store the reports; they
   say PARTIAL, which is not a sign-off.
8. **Install once by hand** on a machine that is not the build machine; store a report if the text says anything about it.
9. **Create the GitHub release** as a draft pre-release, review, publish. *Maintainer only.*
10. **winget manifest** and pull request to `microsoft/winget-pkgs` (release-day step 6). *Maintainer only.*
11. **Update `docs/project/status.md` and `specs/plans/release-0.2.8-plan.md`** with the tag, commit and sums.
12. **Announce.** *Maintainer only.*

Rollback: see "Rollback and yank" in `releasing.md`; a bad Windows file is replaced by a new tag, never an edited asset.
