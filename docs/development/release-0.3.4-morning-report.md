# 0.3.4 morning report (Nov 15, 2026 night, for Nov 16)

One page. Facts come from the reports in `verification/` and `git log` at the time of writing; where a fact is unknown it says so. Nothing is called
"verified"; every report is PARTIAL. Steps for the release: `release-day-0.3.4.md`. Text: `release-0.3.4-github-text.md`.

## What changed tonight

- Everything in the plan is merged on `main` (version 0.3.4, 89 MCP tools). Decisions of Nov 15 built: Published page removed, Click Guide redaction proposals removed,
  local-time titles, window picker for Click Guide, Auto text colour, sidebar tiles, one Vietnamese name for Click Guide (`9d2a5827`).
- Fixes from pass 5 and 6 merged: Long Shot fixed bars on both sides with a scrollbar join, early stops (LS-9), `busy` before any confirmation, console windows
  (`8ac34fe9`, `562f32da`, `ac1ef9fb`); the Click Guide 500-step cap with toasts and a bounded frame queue (CG-9, CG-10); Retype and Blueprint line boxes (`9e2a3dbf`).
- Windows build of `7fb80d70` installed on the lab laptop (setup run silently, `capturemann --version` 0.3.4, one start). A Windows pass 7 was running with no report yet.
- Docs: CLAUDE.md "Current state", `docs/project/status.md` (0.3.4 table with reports and gaps), plan status note, release text (checked list and Known limits filled), `release-day-0.3.4.md`, this file.

## Where the reports are (`verification/`)

- Regression: `L1-windows-0.3.4-84bc00cd-final-regression-{full,hardware}-2026-11-15-claude.*` (66/0 with 14 skipped; 68/0 with 20 skipped; installed build of `84bc00cd`);
  `L1-macos-0.3.4-dev-bundle-26a05100-regression-{standard,full,hardware}-2026-11-15-claude.*` (49/0, 57/0, 57/0; Nov 14 dev bundle). **Neither covers the final code.** Older: `L1-windows-0.3.4-ac730b17-*`,
  `-main-a1bf57dc-*` (including `soak5min`), `-rc1-regression-hardware-*`.
- Summaries: `QA-0.3.4-summary-macos[-pass2..6]-2026-11-15-claude.md`, `QA-0.3.4-summary-windows[-pass3..6]-...` (pass 6 hold the newest defect tables).
- Feature reports: `QA-0.3.4-<topic>-{macos,windows}[-passN]-*` (long-shot, click-guide, boards, bulk, ann-8 to ann-11, imp-6, b1, b2-b3, library, published-recordings, vietnamese, install);
  builds: `QA-0.3.4-final-build-windows-*` (`84bc00cd`) and `QA-0.3.4-windows-final-build-*` (`7fb80d70`); older single checks `L1-*-0.3.4-long-shot-*`, `GDE-*`, `HUD-1-*`, `S2-fill-quality-0.3.4-*`.
- Audits and proposals: `docs/development/ux-audit-long-shot-click-guide-0.3.4.md` (P1 7, P2 16, P3 11), `ux-audit-boards-bulk-0.3.4.md`, `ux-proposals-0.3.4-for-design.md`, `design-requests-0.3.4.md`, `spikes-0.3.4-long-shot.md`.

## Defects still open (severity as the reports rate them)

| Severity | Defect | Source |
|---|---|---|
| medium to high | Click Guide: a click burst grew memory to GB and `guide stop` took 11 minutes (Windows). **A bounded queue and the 500-step cap were merged after that run; not rerun.** | win pass 6 #1, mac pass 6 CG-10 |
| medium | A guide started by the CLI or an agent has no bar and no Stop (a toast only) | win pass 6 #2, CG-2, audit P1-1, D4 |
| medium | Long Shot Auto ended early on all five macOS fixtures; **fixes merged (`562f32da`), not rerun** | mac pass 6 LS-9 |
| medium | Long Shot ended after one frame with fixed left and right bars, and on a classic console; **fixed in `ac1ef9fb`, not rerun on Windows** | win pass 6 #3, #4 |
| unknown | Hand scroll through the real HUD never run; the example gave 2 frames per pattern in a debug build | mac pass 6 LS-11 |
| low to medium | A second Long Shot or guide request showed a confirmation over the page; **fixed in `ac1ef9fb` (`busy` first), not rerun** | win pass 6 #5 |
| low | Markdown export overwrites a folder or file without asking; agents can overwrite files with `export_guide` (audit P1-4, P1-7); title backticks not escaped | win pass 6 #10 |
| low | A click on CaptureMann's own dialog, or on a window over the guided one, becomes a step | win pass 6 #6, #7 |
| low | Quitting during a Long Shot or guide discards it (D5); a failed one-frame run is stored (D26) | audit P1-2, LS-13, LS-08 |
| low | Boards plainer than S10 (labels, margins, shadow); Retype partial boxes on busy backgrounds; polish nits in `ux-proposals-0.3.4-for-design.md`, section 5 | proposals |
| low | Windows licence list in the package is the macOS one | `QA-0.3.4-windows-final-build-*` |

Whether audit P1-2 (a capture error must not discard a Long Shot) and P1-6 (follow the window) are built is **not determined here**; read the audit and `git log`.

## Decisions waiting

- **D1 to D31:** see `docs/development/ux-proposals-0.3.4-for-design.md`, section 3.
- Move area and Blueprint: keep the amber "Not a redaction" note or drop it as Retype did (D1).
- Which audit P1 items to build before the tag (P1-1 bar for agent guides, P1-4 and P1-7 overwrite rules, P1-2, P1-6) and which go to Known limits (the text already lists them as open).
- Linux in 0.3.4 or not: no Linux 0.3.4 build or regression was run tonight. To ship it you need the aarch64 VM (`~/VMs/cm-linux`) and the x86_64 ThinkPad booted into Ubuntu (`scripts/testlab/lab.sh switch ubuntu`), about a day. Otherwise Linux stays at 0.3.3 and the APT repository is not touched.
- Windows licence list: fix (`make licenses` and commit, or bash and Python on the lab laptop) or accept and say so.
- winget pull requests 472327 and 477195: state not known tonight.
- Whether to run the 5-minute Windows soak (needs the microphone) and the macOS hardware tier on the final build.

## Bundles

- macOS dev bundle built from `main` at 21:51: `/Users/trungtran/source/repos/capturemann/target/bundle/CaptureMann-dev.app` (bundle id `dev.capturemann.app.dev`); the copy in `~/Applications/CaptureMann-dev.app` dates from 18:09 (older). Refresh it per the rebuild process before testing. No signed or notarized 0.3.4 DMG exists.
- Windows (unsigned): ThinkPad lab laptop, `C:\wt\out\capturemann-0.3.4-windows-x64-setup.exe` (SHA-256 `1b3f07eb4417274e2dbcaeb000252a8f457256b934802bdc3f49f81c85660415`) and `capturemann-0.3.4-windows-x64.zip`
  (`da7f0622c4dd54892d2952b8b35fa4379d3a0fe3b066758880e4e02274b0d7a5`), built from `7fb80d70`, installed there; source in `C:\wt\main`. (The `84bc00cd` files that the regression ran on were overwritten by this build.)
- Linux: none for 0.3.4.

## Commands to finish the release (as listed in `release-day-0.3.4.md`)

```sh
git log --oneline v0.3.3..HEAD | head -100                       # pick the release commit <sha>
cargo fmt --all -- --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked
cargo deny check; python3 scripts/check_plugin_version.py; make licenses
git push public main                                             # wait for ci.yml and the Security scan
scripts/testlab/regress.sh windows --tier standard --allow screen   # then --tier full, on the candidate; macOS likewise
export CM_SIGN_IDENTITY=...; export APPLE_API_KEY_ID=...; export APPLE_API_ISSUER=...; export APPLE_API_KEY_PATH=...
make release; make release CM_ARCH=x86_64                         # rename to CaptureMann-v0.3.4-macos-<arch>.dmg, rewrite the .sha256
git tag -a v0.3.4 -m "CaptureMann 0.3.4" && git push public v0.3.4
packaging/windows/release-files.sh <downloaded Windows package>  # then: scripts/check-release-assets.sh 0.3.4 <folder>
gh release create v0.3.4 -R craft-mann/capturemann --draft --prerelease --title "CaptureMann 0.3.4: Long Shot, Click Guide, Retype and boards" --notes-file release-body.md
scripts/update-homebrew-cask.sh 0.3.4 /Users/trungtran/source/repos/homebrew-tap    # after the release is public
```

Update the README badge (still says 0.3.3) and the CHANGELOG date before the tag.
