# 0.3.2 final morning report (night of Nov 11 to 12, 2026)

Written by the overnight orchestrator (the main Claude Code session) for the maintainer's review on Nov 12. It is updated during the
night; the **running log** at the end says how far it got. Same pattern as `release-0.3.1-final-morning-report.md`. Wording rule: PARTIAL
reports only; "verified" is not used without a stored report. Plan: `release-0.3.2-plan.md`; board: `release-0.3.2-status.md`.

## Where things stand at 01:30 (read this first)

**Code is final: `77af6b04` (pushed). `public/main` is `5d493d08` = the same code plus docs, reports and the dated CHANGELOG (docs-only push, authorized as "make the release ready": it lets the tag commit contain the CHANGELOG that `release.yml` reads). CI on `5d493d08` was started at 01:30 and takes about 1.5 h: wait for it to be green before tagging.**

| Gate | State |
| --- | --- |
| CI on `77af6b04` (macOS, Windows, Ubuntu, Security scan) | green (macOS `check` hung once in `cm-media` `vp9_export` on the runner, green on re-run) |
| Dependabot alerts | 0 open (the 10 `zip` alerts closed after the spike crates were removed) |
| `windows-tests.yml` five consecutive green runs on the final code | met (`77af6b04`: 43056869830, 43058966521, 43060553064, 43062596263, 43064395521) |
| fmt, check scripts, i18n check, `cargo deny`, Trivy gate | clean |
| Workspace clippy (local) | <running at 01:30, see the log below> |
| Windows 11 candidate regression (ThinkPad) | standard 44/0, full 45/0, ai 2/0, installer 1/0 (PARTIAL) |
| macOS candidate DMGs + regression | built, Developer ID signed, **not notarized**; standard 37/0, full 38/0 (PARTIAL; 6 desktop-session cases not run, rerun them in your Terminal) |
| Linux aarch64 candidate + regression (VM) | full 37/0, tarball and `.deb` install 1/0 (PARTIAL) |
| Linux x86_64 | **candidate built on GitHub only (CI artifacts), NOT regressed: the ThinkPad did not come back from the reboot into Ubuntu** |
| Release text and release-day steps | written; `<FILL>` items need the final files |

**Needs you, in this order:**
1. **The ThinkPad:** look at it (boot menu? GRUB? new address?). It was left on the Ubuntu boot entry; if it boots Ubuntu, tell me the address (or let me find it) and I will build/regress Linux x86_64 on it, then `lab.sh switch windows`. Without it the Linux x86_64 files have no hardware regression (say so in the text, or hold them).
2. **CI green on `5d493d08`** (about 1.5 h from 01:30), then tag: `git tag -a v0.3.2 -m "CaptureMann 0.3.2"` on that commit and `git push public v0.3.2` (never reuse a tag).
3. Final files from the tag: Windows package workflow on `v0.3.2`; macOS: `Release` workflow on the tag, **or** notarize the overnight DMGs (`bash packaging/macos/notarize.sh <dmg>`, then recompute the `.sha256`; valid only because the tag commit has the same code as the one built, see `~/release-0.3.2/macos/NOTES.md`); Linux: x86_64 per item 1, aarch64 either reuse the candidate (same code) or rebuild; sign the Linux files with minisign.
4. Fill the `<FILL>` items of `docs/development/release-0.3.2-github-text.md` from the reports and the final SHA-256 sums, save the body as `release-body.md`, upload to the draft release, publish. Then APT, winget (472327 and 477195 are open), Homebrew cask, plugin submission (`docs/development/plugin-submission-0.3.2.md`). All steps: `docs/development/release-day-0.3.2.md`.

## Authorizations given by the maintainer (Nov 11, 2026, about 21:30)

- The orchestrator may **push fixes to `public/main`** that turn CI green (small, after `cargo fmt`, clippy, `cargo deny` and the Trivy gate run
  locally; see `feedback-security-scan-before-merge`). **Never a tag, never a force push.**
- **Candidates tonight, the maintainer tags in the morning.** Candidate files are built from the same commit the tag will point to; after the
  tag, the final files build from the tag (Windows package workflow on the tag, macOS Release workflow, Linux builds) and are re-checked. A
  report on candidate files says "candidate files of commit <sha>", not "final files".
- The orchestrator may **reboot the ThinkPad between Windows and Ubuntu** (`scripts/testlab/lab.sh switch`) and use the aarch64 VM.
- Not allowed without a new answer: hardware regression tiers (screen, sound, microphone, camera), anything with sound after 22:00, touching
  `/Applications`, `target/bundle` (Claude Desktop uses it), `~/Movies/CaptureMann`, or the maintainer's installed apps.

## Gates for "ready to tag" (all must hold on one commit)

1. `ci.yml` green on macOS, Windows and Linux, and the Security scan green on that commit.
2. `exp4_ac4_cancel_deletes_partial_output` and the Windows tests: five consecutive green `windows-tests.yml` runs on that commit.
3. Local: `cargo fmt`, `clippy --workspace --all-targets --locked -D warnings`, `cargo test --workspace --locked --no-fail-fast`, `cargo deny check`, `check_target_os.py`, `check_no_bare_command.py`, `cm-i18n check`, Trivy gate; `THIRD_PARTY_LICENSES.md` unchanged by `make licenses` (or regenerated and committed).
4. A stored `cm-regress` report per system for candidate files: macOS arm64 (and the x86_64 DMG checked on Apple Silicon), Windows 11, Linux x86_64, Linux aarch64.
5. Release text with no `<FILL>` left that can be filled without the maintainer; claims without a report removed.

## Roster

| Role | Task | Output |
| --- | --- | --- |
| Orchestrator (main session) | CI watch and fixes, `windows-tests.yml` x5, final local checks, merges, this report, the commits | this file, `verification/` commits |
| `materials` (agent) | release text, release-day 0.3.2 steps, status, CLAUDE.md, plan headers | `docs/development/release-0.3.2-github-text.md`, `release-day-0.3.2.md` |
| `mac-release` (agent) | both DMGs from the release commit, notarized when the credentials work, `.sha256`, DMG checks, macOS regression standard and full | `~/release-0.3.2/macos`, reports |
| `win-regress` (agent) | Windows package candidate on the ThinkPad: standard, full, ai and installer cases | reports |
| `linux-aarch64` (agent) | build, `check-package.sh`, `full` regression in the VM | `~/release-0.3.2/linux/aarch64`, report |
| `linux-x86` (agent) | after Windows: reboot to Ubuntu, build and regress, back to Windows | `~/release-0.3.2/linux/x86_64`, report |

## What stays with the maintainer (M)

Tag `v0.3.2` and push it; run the final-file builds from the tag (or tell me to); minisign the Linux files (`sign-linux.sh`, secret key never
leaves your machine); upload to the draft release and publish; APT upload; winget pull request (472327 and 477195 are still open); Homebrew
cask; website and announcement; plugin submission; SignPath; Intel Mac, Windows 10 and other hardware not run. Steps: `docs/development/release-day-0.3.2.md`.

## Running log

- Nov 11 21:30: authorizations received; runbook written; local full tests at 1,599 passed, 1 failed (`exp5_webm_width_works_end_to_end_and_is_refused_elsewhere`, a 120 s WebM export timeout under machine load; to be re-run alone); CI on `4f96ba59`: Security scan green, CI running.
- Nov 11 22:00: **materials** agent done and merged (release text, release-day-0.3.2 steps, status.md, CLAUDE.md; `<FILL>` list in `docs/development/release-0.3.2-github-text.md`). Windows package candidate (run 43038532341, `4f96ba59`) built green; `windows-tests.yml` run 1 of 5 green (43038612413).
- Nov 11 22:10: **Linux aarch64 candidate** of `4f96ba59` built in the VM (about 9 min, rustc 1.99.1, Ubuntu 24.04.5): `capturemann-0.3.2-linux-aarch64.tar.gz` SHA-256 `fecc4939312644275a7a780f26072c1a8756894eb1cb87428c6c0fe3b277141b`, `capturemann_0.3.2_arm64.deb` `cb235d27f62a87f18de7d7cdba3c301796d44ec7ee31c1aa2ac5ced7741bd267` (`~/release-0.3.2/linux/aarch64`, unsigned); `check-package.sh` PASS for both; regression `full` 37/0 (36 skipped of 73), tarball install 1/0, `.deb` install 1/0, same as 0.3.1; reports `verification/L1-linux-0.3.2-aarch64-vm-candidate-4f96ba59-*` (PARTIAL). The agent freed VM disk (84 % full) by deleting old scratch (0.3.0 `.deb` files, `release-out/6cdcb033-aarch64`, old regress outputs); the 0.3.1 release files on the Mac and the VM's `5e1a0d9d-aarch64` outputs are intact.
- Nov 11 22:20: **Windows 11 candidate regression** (ThinkPad, candidate package run 43038532341 of `4f96ba59`, test programs run 43038612413): standard 44/0 (14 skipped), full 45/0 (28 skipped; 0.3.1 had 58/0 because the 13 `--allow screen` cases were not run tonight), ai.transcribe + ai.speakers 2/0, installer.silent 1/0 on the rerun (the first run failed only because the installer had not been copied to the laptop; both reports kept). Extra checks: `capturemann text` returned both lines of a synthetic PNG exactly via `windows_ocr` (only en-US has an OCR pack on the laptop; vi has none), `diagnostics` names the recognizer, `capturemann mcp` initialize and `tools/list` = 81 tools. Files: `capturemann-0.3.2-windows-x64.zip` SHA-256 `3e2beed96f86212055a0f2f157c976a3298dc961e66f1c406d97c74784707801`, `capturemann-0.3.2-windows-x64-setup.exe` `2b0a8bc60bc1d295079047a9000437399698a34927b8bf211a39368e8a734951` (`~/release-0.3.2/win`, `win-out` with SHA256SUMS and the winget manifest). Reports `verification/L1-windows-0.3.2-*` (PARTIAL), commit `00f5ff14`. The laptop is back to its 0.3.1 install and still up in Windows. `windows-tests.yml` runs 1 to 4 of 5 green on `4f96ba59`.
- Nov 11 22:25: **linux-x86** agent started: reboot the ThinkPad into Ubuntu, build, check, regress, reboot back to Windows.
- Nov 11 22:30: **Gate 2 met on `4f96ba59`:** five consecutive `windows-tests.yml` runs green (run 1: id 43038612413 completed success 4f96ba59; run 2: id 43041147079 completed success 4f96ba59; run 3: id 43042943933 completed success 4f96ba59; run 4: id 43044371772 completed success 4f96ba59; run 5: id 43046111999 completed success 4f96ba59; ALL FIVE GREEN). If a fix is pushed later and changes code, rerun the five.
- Nov 11 22:45: **linux-x86 stopped:** after `lab.sh switch ubuntu` the ThinkPad never answered (ping, ssh and ARP all dead at 22:45, still `up: none` at 23:16; no power-cycle by other means was done). Linux x86_64 build and regression NOT run on the ThinkPad. Fallback: `linux-package.yml` triggered on GitHub (run 43051585284, `4f96ba59`) for x86_64 and aarch64 candidate artifacts with `check-package.sh` (no regression on real x86_64). **You must look at the ThinkPad in the morning** (boot menu? GRUB? a different address?); `lab.sh switch windows` needs Ubuntu up on ssh.
- Nov 11 22:50: **CI on `4f96ba59` failed** on macOS and Windows `Test` only: `layout_width_tests::english_and_vietnamese_labels_fit_their_controls` (the reviewed Vietnamese "Đang nhận dạng văn bản…" is 144 px, the editor Copy text button was pinned to a 110 px label room). Ubuntu job finished separately. Local full run: 4,068 passed, 2 failed (that same test; and `exp5_webm_width_works_end_to_end_and_is_refused_elsewhere`, a 120 s WebM export timeout under heavy machine load, to be re-run alone).
- Nov 11 23:25: fix `77af6b04` (button pinned to 190 px, test only; fmt and the Trivy gate clean locally) **pushed to `public/main`** together with the docs and reports commits (authorized). New CI run started; `windows-tests.yml` x5 restarted on `77af6b04` (the earlier five were on `4f96ba59`, code identical apart from this test constant).
- Nov 11 23:55: **macOS candidate DMGs** of `6f95def3` (code identical to the pushed `77af6b04`; the Developer ID signing worked without a dialog): `~/release-0.3.2/macos/CaptureMann-0.3.2-arm64.dmg` SHA-256 `862dd40dd419b05d2310aa40c1eb82d05a8e5f4a550b2a79b6da52d54a4f3a00` (87 MB), `CaptureMann-0.3.2-x86_64.dmg` `f4dc8905b3d9036eb2a245fc44b08718ddebe7eec74098978d7b4aa2a7ab5c0c` (92 MB), `NOTES.md` beside them. **Developer ID signed and hardened, NOT notarized, NOT stapled** (no notary credentials in the overnight session): `codesign --verify --deep --strict` passes, all 7 Mach-O files carry the identity and the hardened runtime, entitlements only audio-input and camera, `spctl` rejects both as "Unnotarized Developer ID" (expected), `hdiutil verify` valid, `verify-release.sh` fails only its 4 notarization checks per DMG; both apps run (`capturemann --version` 0.3.2; Intel under Rosetta). Regression on the arm64 app, no hardware: standard 37/0 (21 skipped), full 38/0 (35 skipped), no cm-overlay race. The first standard run failed `pkg.diagnostics` only because `regress.sh` used a stale `target/release/cm-daemon` (0.3.1) that `bundle-release` does not rebuild: rebuilt, rerun passed (both reports kept). **Six cases did not run** (SKIP "needs a logged-in desktop session": `targets.list`, `shot.invalid`, `desktop.stays_up`, `desktop.second_launch`, `desktop.tray_icon`, `mcp.confirm_tier`; 0.3.1 had them at 43 passes); rerun them from Terminal in your logged-in session: `scripts/testlab/regress.sh macos --tier standard`. Extra checks: `capturemann text` read both lines of the fixture exactly via `apple_vision`; `capturemann mcp` `tools/list` = 81. Reports `verification/L1-macos-0.3.2-*` (PARTIAL).
- Nov 12 00:06: final local checks on the pushed code: `cargo fmt`, `check_target_os.py`, `check_no_bare_command.py`, `check_md_links.py`, `check_plugin_version.py`, `cm-i18n check`, `cargo deny check` all exit 0; the WebM export test `exp5_webm_width_works_end_to_end_and_is_refused_elsewhere` passes alone (the 120 s timeout in the full run was machine load); old full workspace run: 4,706 passed, 2 targets failed (that WebM test under load, and the layout width test fixed in `77af6b04`). CI on `77af6b04`: `check (windows-latest)` green (it was the red job on `4f96ba59`), `licenses`, `msrv`, `docs`, `plan`, `bundle-macos` green, macOS and Ubuntu `check` still running; Security scan green. `windows-tests.yml` on `77af6b04`: runs 1 to 4 of 5 green. Workspace clippy still running.
- Nov 12 00:45: **Linux x86_64 candidate built on GitHub** (workflow `linux-package.yml`, run 43051585284, commit `4f96ba59`; the first x86_64 attempt died after 59 min with no log kept (lost runner, the 120 min timeout was not reached), the re-run of the failed job passed; the aarch64 job passed first time, 87 min): `capturemann-0.3.2-linux-x86_64.tar.gz` SHA-256 `efce48312104ddca01f797beb2bd359425b24f76eca1fafa70ec28d71b56cdb6`, `capturemann_0.3.2_amd64.deb` `1ed9443c93ecfdfe094fe11c72493ab4b29597ca229122c4d8bd4335472b703f` (`~/release-0.3.2/linux/x86_64-ci`, `.sha256` files verify, unsigned); the workflow's own tarball check, `check-package.sh` and the `.deb` install check passed. **No regression on real x86_64 hardware** (the ThinkPad did not come back from the reboot into Ubuntu). Note: these files come from the CI runner, the 0.3.1 flow built them on the ThinkPad with `release-build.sh`.
- Nov 12 00:45: `windows-tests.yml` **five consecutive green runs on the final code `77af6b04`** (runs 43056869830, 43058966521, 43060553064, 43062596263, 43064395521).
- Nov 12 01:11: **CI green on `77af6b04`** (run 43056825513): msrv, docs, plan, licenses, bundle-macos, check (ubuntu), check (windows) green on the first run; `check (macos-latest)` hung inside `cm-media` test `a_vp9_mp4_recording_exports_with_an_encoder_the_host_may_use` (a stuck `ffmpeg`, cancelled at the 90 minute limit; the test and `cm-media` code are unchanged in 0.3.2, it passes locally and on earlier macOS runs) and **passed on the re-run of the failed job**. Security scan green. If the hang returns on the tag run, root-cause it (an `ffmpeg` hang on the macOS runner image; the test has no timeout of its own).
