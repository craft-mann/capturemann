# Windows 0.2.8: release checklist (for the maintainer)

Draft of Nov 5, 2026. "Maintainer only" marks steps an agent must not do (merging, pushing, tags, publishing,
accounts). General process: `docs/development/releasing.md`. Text: `windows-0.2.8-release-notes-draft.md`.
Decisions already taken (Nov 5, 2026): no code signing in 0.x (O4), recording ships (O5), zip and NSIS installer (O7).

1. **Review `track/windows`** and the open points in the release notes draft (what is not there or limited).
   *Maintainer only.*
2. **Bump the version to 0.2.8** in the root `Cargo.toml` (`[workspace.package]`), run `cargo update -w`, add the
   `## [0.2.8]` section to `CHANGELOG.md`, run `make licenses` if `Cargo.lock` changed. (The build names its files from
   the workspace version.) Run fmt, clippy `-D warnings`, `cargo test --workspace --locked` and `python3
   scripts/check_target_os.py`.
3. **Merge `track/windows` into `main`** and wait for CI (`ci.yml`, Security scan) to pass. *Maintainer only.*
4. **Tag** `v0.2.8` (never reuse a tag) and push it. *Maintainer only.* The macOS part of 0.2.8 (if shipped in the same
   release) follows `releasing.md`.
5. **Build the Windows files:** Actions, "Windows package", Run workflow, on the tag or the merge commit. It builds the
   zip and the installer, builds the LGPL FFmpeg and `llama-server`, and runs the smoke steps. The files are workflow
   artifacts only; the run creates no release. *Maintainer only (needs the repository's Actions).*
6. **Download the artifact** `capturemann-windows-x64`: `capturemann-0.2.8-windows-x64.zip`,
   `capturemann-0.2.8-windows-x64-setup.exe` and their `.sha256` files.
7. **Check the sums:** `certutil -hashfile <file> SHA256` (or `shasum -a 256`) and compare with the `.sha256` files.
   Write the two sums into the release text in place of `<sha256 zip>` and `<sha256 installer>`.
8. **Install once by hand** on a Windows machine that is not the build machine (zip: unpack and run `cm-desktop.exe`;
   installer: per user), start the app, take a screenshot, make a 10 s recording, uninstall. Store a report in
   `verification/` if anything is said about it in the notes.
9. **Create the GitHub release** `v0.2.8` as a draft with both files and both `.sha256` files, the text from
   `windows-0.2.8-release-notes-draft.md` (including the SmartScreen paragraph), marked pre-release while the version
   is 0.x. Review the draft, then publish. *Maintainer only.*
10. **winget manifest:**
    `packaging/windows/winget/render.sh 0.2.8 https://github.com/craft-mann/capturemann/releases/download/v0.2.8/capturemann-0.2.8-windows-x64-setup.exe <sha256 installer> out/`
    then on Windows `winget validate --manifest <folder>` and `winget install --manifest <folder>`, and open the pull
    request with the folder `manifests/c/CaptureMann/CaptureMann/0.2.8/` to `microsoft/winget-pkgs`. *Maintainer only.*
    After it is merged, add the winget line to the release notes.
11. **Update `docs/project/status.md`** (the "Windows 0.2.8 track" section becomes "Released <date>" with the tag, the
    commit and the file sums) and `specs/plans/release-0.2.8-plan.md`. Commit to `main`.
12. **Announce** (README download link and website, if used; the Homebrew cask is macOS only). *Maintainer only.*

Rollback: see "Rollback and yank" in `releasing.md`; a bad Windows file is replaced by a new tag, never an edited asset.
