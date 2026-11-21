# Release day, 0.3.6 (Windows, macOS, Linux): steps

Written on Nov 21, 2026 from `release-day-0.3.5.md` (the pattern; read its sections 4 and 5 and "Rollback" for the generic steps and lessons). "[M]" = only the maintainer does this (tag, sign, notarize, publish, accounts, decisions). "[A]" = an agent can do it from this file. Text to publish: `release-0.3.6-github-text.md` (its `<FILL: ...>` items). Plan: `specs/plans/release-0.3.6-plan.md`. Version is 0.3.6 in `Cargo.toml`, `Cargo.lock` and the plugin; MCP tools 90.

## What is already done (Nov 21, 2026)

- **Pushed:** `public/main` at `96b85253` (and docs commits above it). CI green on `96b85253`, the Security scan green, the Windows package workflow green (run 44898477660, files are workflow artifacts).
- **Full gate:** run once on `77d50772` (fmt, `cargo deny`, workspace clippy, tests 5555 plus the i18n fix, Trivy vulnerability and secret scan). Later commits: Quick Capture fixes (Ctrl+1, name-screen Esc, drag on Windows, value menu arrows) and CI/test fixes; clippy and the quick tests ran on each, the whole workspace was run by CI on `96b85253`.
- **macOS DMGs [M, done]:** `target/bundle/CaptureMann-0.3.6-arm64.dmg` (`7cdc5fde...`) and `target/bundle-x86_64/CaptureMann-0.3.6-x86_64.dmg` (`c52e6256...`), notarized, stapled, `verify-release.sh` passed for both; `cm-regress` on the arm64 app: `L1-macos-0.3.6-dmg-96b85253-regression-*`.
- **QA passes:** macOS `77d50772`, Windows `8a746f6b`, Linux `57f50732` and the recheck `103290cd` (reports named in the text).
- **CI lessons of Nov 21:** the speaker model is fetched from the mirror release `models-sherpa-2026-11-21` (k2-fsa replaced the upstream file); Windows CI drops the cached whisper.cpp CMake build; Chocolatey ffmpeg is pinned to 9.0.2. Never run `packaging/sherpa/fetch-models.sh` in the repo checkout.

## Still open before the tag

1. **[A] Windows final regression** on the installed files of the Windows package run 44898477660 (standard, full, hardware with screen): the earlier Windows pass ran on `8a746f6b` and does not include the Ctrl+1, Esc and drag fixes. Needs the ThinkPad booted into Windows and logged in.
2. **[A] Linux final files:** build from the release commit on the ThinkPad (`packaging/linux/release-build.sh --src ~/wt/main --commit <sha> --jobs 3`; the `96b85253` build started Nov 21), `packaging/linux/check-package.sh`, `cm-regress` standard and full on the installed `.deb`, copy the four files; aarch64 in the VM (`~/VMs/cm-linux/ssh.cmd`) if shipped.
3. **[M] Linux signing:** minisign the tarballs and `.deb` files; APT repository update afterwards (0.3.0 to 0.3.6).
4. **[M] Rename and hash the DMGs:** `CaptureMann-v0.3.6-macos-arm64.dmg` and `CaptureMann-v0.3.6-macos-x86_64.dmg`, write `.sha256` again.
5. **[A] Fill the text:** every `<FILL: ...>` in `release-0.3.6-github-text.md`, the licences sentence from a diff of `Cargo.lock` and `THIRD_PARTY_LICENSES.md` against `v0.3.5`.

## Tag and publish

1. **[M]** `CHANGELOG.md`: `## [0.3.6] - Unreleased` becomes `## [0.3.6] - <date>` and a new empty `## [Unreleased]` goes above; `CLAUDE.md` says "0.3.6 is released" with the final facts; `docs/project/status.md`; header of `specs/plans/release-0.3.6-plan.md` ("released <date>"). Commit and push; wait for CI and the Security scan. These docs commits sit above the tag if the tag stays on the commit the files were built from: say so in the text.
2. **[M]** `git tag -a v0.3.6 -m "CaptureMann 0.3.6"` on the commit the files were built from (`96b85253`) and `git push public v0.3.6`.
3. **[A]** `packaging/windows/release-files.sh` (checksums, winget manifest), then `scripts/check-release-assets.sh 0.3.6 <folder>`.
4. **[M]** `gh release create v0.3.6 -R craft-mann/capturemann --draft --title "CaptureMann 0.3.6: Quick Capture" --notes-file release-body.md` and upload the files; check the list; download one file per system; publish. The release `models-sherpa-2026-11-21` is a pre-release mirror and must stay a pre-release (never Latest).
5. **After:** Homebrew cask (`scripts/update-homebrew-cask.sh 0.3.6 /Users/trungtran/source/repos/homebrew-tap`), winget (`release-day-0.3.2.md`, section 7), APT, the plugin (`tests/manual/PLUGIN-1.md`), website download links and the feature page (`docs/website-features.md`), post-release hash checks, then the big disk cleanup (`target/` is about 300 GB; keep the two DMGs until the release is public).

## Known open items for 0.3.7

D4 (invented outro over room-only noise), X11 drag and Wayland placement of the bar, Tab in the preset sheet on X11, the Windows `ai.transcribe` case limit (600 s, `--timeout-scale` does not change it), `desktop.hotkey_overlay` guard (match the hotkey holder, not the process name), Long Shot's first-step loss on Windows, P-3 not reproduced, F-4 (the bar opens 5 px high on Windows), headless Wayland checks of Long Shot and Click Guide.
