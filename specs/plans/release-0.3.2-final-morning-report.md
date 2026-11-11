# 0.3.2 final morning report (night of Nov 11 to 12, 2026)

Written by the overnight orchestrator (the main Claude Code session) for the maintainer's review on Nov 12. It is updated during the
night; the **running log** at the end says how far it got. Same pattern as `release-0.3.1-final-morning-report.md`. Wording rule: PARTIAL
reports only; "verified" is not used without a stored report. Plan: `release-0.3.2-plan.md`; board: `release-0.3.2-status.md`.

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
