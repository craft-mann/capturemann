# 0.3.1 night report (Nov 9 to 10, 2026)

Written by the night orchestrator. Nothing was pushed, tagged or published. Every report below is PARTIAL (one computer each) and was run on
the **dev bundle / CI package / VM build of an earlier commit**, so none of them replaces the run on the final files (step 5 of the list at the end).

## What ran

| Run | Commit | Result | Report |
| --- | --- | --- | --- |
| macOS `cm-regress` standard (before the night's merges) | 97ea4b8a | 43 passed, 0 failed | `verification/L1-macos-0.3.1-regression-standard-2026-11-09-claude.md` |
| macOS `cm-regress` full (screen, mic, camera, sound), dev bundle, 22 more cases than standard incl. the MP3 import | ea0fd595 | 52 passed, 0 failed, 21 skipped (not implemented on macOS, needs a second monitor, a model or an installer) | `L1-macos-0.3.1-regression-full-full-2026-11-09-claude.md` |
| macOS `cm-regress` hardware tier (mic, camera, system audio) | same | 55 passed, 0 failed, 25 skipped | `L1-macos-0.3.1-regression-hardware-hw-2026-11-09-claude.md` |
| `cargo fmt`, `clippy -D warnings`, `cargo test --workspace --locked --no-fail-fast`, `cargo deny check`, `check_target_os.py`, `cm-i18n check` | d62549ec | all clean; 4626 tests passed, 0 failed, 50 ignored | `L1-macos-0.3.1-cargo-test-workspace-2026-11-10-claude.txt` |
| `estimate_vs_real.rs` (heavy, macOS, bundled FFmpeg) | d62549ec | passed: 18 of 18 samples at or below the bound (highest real/bound 0.94) | `verification/benchmarks/export-estimate-macos-2026-11-10.md` |
| Linux aarch64 VM: `release-build.sh` (tarball and .deb, `check-package.sh` both) then `cm-regress` full | 7bd42994 | build and package checks passed; 37 passed, 0 failed, 36 skipped (no desktop session, no GPU in the VM) | `L1-linux-0.3.1-aarch64-vm-regression-full-2026-11-10-claude.md` |
| Windows package run 42648505921 and tests run 42648553860 (on main 97ea4b8a) | 97ea4b8a | both **success** | GitHub Actions |
| Windows `cm-regress` standard and full on the ThinkPad | 97ea4b8a | **not run, blocked** (see below) | `L1-windows-0.3.1-regression-BLOCKED-2026-11-10-claude.md` |

The VM files (not signed, not for release): `~/wt/release-out/7bd42994-aarch64/` in the VM (`capturemann-0.3.1-linux-aarch64.tar.gz` sha256 `c71dc2dc...3b93`,
`capturemann_0.3.1_arm64.deb` sha256 `a4ec3b4c...8996`).

## Blocked: Windows regression (needs you)

Windows 11 on the ThinkPad now has **Smart App Control on** (`VerifiedAndReputablePolicyState` = 1). It refuses the unsigned 0.3.1 installer and
`cm-regress.exe` ("An Application Control policy has blocked this file", "blocked by your organization's Device Guard policy"). The 0.2.9 runs of Nov 8
were not blocked, so it was switched on or left evaluation mode since. I changed nothing on the laptop (no bypass). Options: turn Smart App Control
off (Windows Security, App and browser control; it cannot be turned on again without a reset), or sign the files (SignPath), then run
`scripts/testlab/regress.sh windows --tier standard --run 42648553860 --package-run 42648505921 --commit 97ea4b8a` and `--tier full --allow screen,sound`.
The laptop was up in Windows, logged in, and reachable all night.

## Fixes and merges

- **MP3 on Ubuntu CI** (two import tests failed on ffmpeg 6.1: the stream's stated length is 2.06 s for a 2 s file): commit e5d742a5 (agent branch,
  reviewed by me, merged as 53ebb450): `verify` allows three MP3 frames of slack for codec `mp3` only (a half-length decode is still refused, new test),
  and the need-bytes test allows 10 %. Not reproduced here (macOS ffmpeg); derived from the CI log. fmt, clippy, `cm-media` and `cm-daemon` import suites pass.
  **Confirm on the next Linux CI run.** The VM (ffmpeg 9.0.2 bundled) imports the MP3 fixture as 2.09 s for 2.00 s.
- **Real MP3 fixture in `import.audio`** (ea0fd595): `tests/data/regress/tone.mp3` (8 KB, synthetic sine, command in the README there), embedded in `cm-regress`.
  Passed on macOS and on Linux aarch64.
- **The two Mac `recedit::render` failures** (d62549ec): not a render bug. The preview worker thread was the first thing in the process to ask Metal for its
  device list, and `MTLCopyAllDevices` blocked in `dispatch_once` (shown with `sample`), so the two worker tests timed out. The tests now create a
  compositor on the test thread first. All 8 render tests pass. Real app is not affected as far as known (the app creates a compositor on its own threads
  earlier), but if the preview ever fails to appear on first open of the editor on a Mac, this is the first place to look.
- **Flaky `cm-daemon ipc::tests::r2_7_stale_socket_is_replaced_live_one_is_not`**: not reproduced (40 isolated runs, 5 full-lib runs, and 8 parallel loops of
  25 full-lib runs under load, 0 failures; the whole workspace also passed). The code takes a lock around the process-wide umask. Left as is; no cause found.
- Worktrees and branches of merged agent work were removed (`track/*`, `worktree-agent-*` except the stale `agent-a669babfe2aec922c`, kept on purpose).

## Not done

- Windows `exp4_ac4_cancel_deletes_partial_output` five consecutive green runs: needs the push and repeated `Windows tests` runs (tests run 42648553860 on 97ea4b8a was green, once).
- `estimate_vs_real` on Windows and on Linux (Windows blocked; the VM has no hardware encoder; the ThinkPad Linux side was not used because it was in Windows).
- Linux ThinkPad (x86_64, X11/Wayland) regression, Intel Mac, `ai.transcribe` and `ai.speakers` (no model passed), `installer.dmg`, `rec.soak`, second monitor.
- Final-files regression on all three systems.
- Version, CHANGELOG, README and status were already at 0.3.1 / [Unreleased]; I did not change CHANGELOG or `docs/project/status.md` (no facts changed beyond this report).

## Merged but not pushed (main, after 97ea4b8a)

```
e5d742a5 import: an MP3's stated length gets three frames of slack in verify; the size estimate test allows 10 % (IMP-1)
ea0fd595 regress: import.audio also imports a 2 s MP3 fixture (MP3 import, 0.3.1)
53ebb450 Merge commit 'e5d742a5'
5d388852 docs: 0.3.1 release text and release-day steps (drafts)
801b337a verification: macOS 0.3.1 cm-regress standard, full and hardware on the dev bundle (PARTIAL, 0 failures)
d62549ec desktop tests: initialise Metal on the test thread before the preview worker (recedit::render hang in MTLCopyAllDevices)
7bd42994 verification: macOS cargo test summary; Windows regression blocked by Smart App Control (not run)
+ the commits after it: the estimate_vs_real report and this report (git log --oneline 97ea4b8a..HEAD)
```

## What you must do

1. Decide on Smart App Control on the ThinkPad (see above) so the Windows regression can run.
2. `git push public main`; check CI on macOS, Windows and Linux (the MP3 fix is for Ubuntu).
3. Run `Windows tests` until `exp4_ac4_cancel_deletes_partial_output` passes five runs in a row.
4. Read `docs/development/release-day-0.3.1.md` and `release-0.3.1-github-text.md` (drafts; `<FILL>` items are release-day facts).
5. Build the final files on all three systems, run the regression on them (Windows `--tier full --allow screen,sound`; macOS `--tier full` with the notarized app; Linux on the laptop and VM), store the reports.
6. Sign, tag `v0.3.1`, draft the pre-release, publish, APT repository.
