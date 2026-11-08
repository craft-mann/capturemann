# Release 0.3.0 morning report (written the night of Nov 8 to 9, 2026)

Written by the night orchestrator. The running record with timestamps is `release-0.3.0-night-log.md`. Wording rule:
nothing is "verified" without a stored report; every report named here is in `verification/` and is marked PARTIAL (one
computer).

## In one paragraph

0.3.0 (Linux only) is **built, tested and prepared, not published**. All worker branches are merged into `main`; the x86_64
packages were built on the lab laptop and the aarch64 packages in the VM from commit `5301d4b8`; `cm-regress` ran on those
files on X11, GNOME Wayland and the aarch64 VM with **0 failures** in every tier that ran; all 17 failures of the 0.2.9 Linux
run have an outcome. Nothing was pushed, tagged, signed or uploaded. What is missing is listed under "Not done".

## What is done and merged

Final `main` head: see `git log -1`. Code frozen at `5301d4b8` (the packages are built from it); later commits are documents, `cm-regress` cases, tests and reports, plus one behaviour-neutral move: the cursor-track rule (`records_cursor_track`) went from `cm-daemon` to `cm-platform::capability` because `check_target_os.py` (CI job `docs`) failed on it; the packages therefore differ from `main` by that move only.
175 commits and 40 merges since `2a2911fc` (0.2.9).

| Track | Result (branch merged) |
| --- | --- |
| media | Import/export without a hardware encoder: VA-API H.264 with a probed rate mode (the T490's iHD driver offers CQP only, which was why 0.2.9 fell back to VP9), software VP9 as the labelled fallback; codec labels in result, CLI and toast; O10 notes in `specs/features/08-export.md` and `18-import.md`; test expectations for the Linux encoder policy (`CM_EXPORT_ALLOW_GPL=1` in CI keeps libx264 coverage) |
| packaging | licence files where Linux packages keep them; `check-package.sh`; `release-build.sh` (tarball + `.deb` + checks + sums from a commit); APT repository tool `make-apt-repo.sh`, `test-apt-repo.sh`, CI job, `docs/development/linux-apt.md`; text-model download and generation checked on the T490 |
| recording | recording on by default (`CM_LINUX_RECORDING=0` opts out); GNOME Mutter screen-cast with no dialog (portal fallback); region and X11 window recording; VA-API H.264 / VP9 fallback; 10-minute gates; kill -9 recovery; late-frame fix (event-driven PipeWire source) |
| camera | Video4Linux2 backend (MJPEG/YUYV), camera track, picker and check |
| platform | idle-sleep guard, PipeWire microphone list, chime player (pw-play, paplay, aplay), cursor track on X11 |
| desktop | tray fixes, Wayland window screenshot via the shell's saved file, Shortcuts page, diagnostics line, delay-pill display-id bug (`desktop.settings_applied`), lab environment for Wayland |
| regress (NEW-F) | `shot.window`, `redact.window_text`, `desktop.recording_banner`, `desktop.settings_applied`, installer cases, Wayland skips with reasons, headless skips |
| UI pass | recording UI looked at on X11 and Wayland, nine fixes; four more small fixes (Shortcuts subtitle, Linux Permissions note, cursor row, tests) |
| Vietnamese | 28 + 4 new Linux strings as drafts (117 drafts in total) |

## Test and regression results (numbers from the stored reports)

Mac (the final code state, commit `5301d4b8`): `cargo fmt --check`, `cargo clippy --workspace --all-targets --locked -D warnings`,
`check_target_os.py`, `check_md_links.py`, `cm-i18n check`, `cargo deny check` pass. `cargo test --workspace --locked
--no-fail-fast`: 166 suites ok, **2 failures in `cm-desktop` lib tests** (`recedit::render::tests::source_errors_keep_the_last_picture`
and `worker_is_latest_wins_and_stops`: the preview renderer produces no picture within 20 s). The code of that path is unchanged
since `9c6133cb`, where the whole suite passed earlier the same night; I could not tell why the Mac stopped producing the picture
(the GPU path of the compositor; the machine was busy with the aarch64 VM). **Treat as environment-dependent, rerun on a quiet Mac
before pushing** (`cargo test -p cm-desktop --lib recedit::render`). **Follow-up on the two Mac render tests (Nov 9, 04:50, lead):** I reran them. The same source passes (8/8, under 1 s, eight runs) when
built into a fresh target directory outside the repository, on the commits `a18c7641`, `5301d4b8` and the current `main`, and
`v0.2.9` passes too; a `git bisect` between `9c6133cb` and `5301d4b8` ended on a documents-only commit, so no code change causes it.
Inside the repository's own `target/` (also after `cargo clean -p cm-desktop`) both tests still time out at `render.rs:760`
(the worker thread never returns a picture). So the failure depends on where the test binary is built and run in this session's
environment (the GPU path of the preview renderer), not on the 0.3.0 code. Still run `cargo test -p cm-desktop --lib recedit::render`
once in your own terminal before pushing.

`cm-regress --dev --tier standard` on the Mac: 34 passed, 0 failed,
22 skipped (`L1-macos-0.3.0-dev-cm-regress-standard-shared-code-check-*`): the shared changes do not break import, export or MCP on the Mac.
Laptop (Linux, x86_64, run over ssh without a desktop session): fmt and clippy pass; `cargo test --workspace --no-fail-fast`: 166 suites ok and 1 failure (`agt2_agent_start_asks_with_the_exact_plan`: the cursor row of the start confirmation now says "Shown in the video only" when the run has no X11 session; the test expectation was widened for Linux, then `recording_engine` 65/65 passed on the laptop). Not re-run as a whole after that one-line test change.

`cm-regress` on the final files (passed / failed / skipped; reports `L1-linux-0.3.0-regression-*`):

| Run | Result |
| --- | --- |
| X11 smoke / standard | 13/0/1 and 49/0/7 |
| X11 hardware tier (screen, mic, camera; no sound) | 58/0/20 of 78 |
| X11 full with tarball installer / `.deb` installer | 57/0/14 and 56/0/15; `linux.deb_install` on a clean machine 1/1 |
| X11 `rec.soak` 10 minutes (screen + microphone, silent) | 1/0/0 |
| X11 standard on the installed `.deb` (apt upgrade from 0.2.9 first) | 49/0/7 |
| Wayland smoke / standard / hardware (no sound) | 13/0/1, 47/0/9, 55/0/23 of 78 |
| aarch64 VM headless, standard / full | 34/0/22 and 35/0/36 |

Triage of the 17 failures of 0.2.9: `L1-linux-0.3.0-triage-2026-11-09-claude.md` (all 17 pass or skip with a reason; `mcp.policy_allow` was the portal dialog, Linux-only, passes on the Mac).
Bugs the regression itself found and that are fixed: `desktop.settings_applied` (delay pill sent the 0-based display id; X11 engine id is 65), the `linux.vaapi_encoder` case matched the camera line, `rec.screen_cursor` and `shot.invalid` expected things that Wayland cannot do (now stated gaps), headless runs of `mcp.confirm_tier`/`policy_allow`, the Wayland lab environment (no DISPLAY in gnome-shell's environment under gdm).
Recording gates (reports `L1-linux-0.3.0-recording-ten-minutes-*`): X11 silent 10 minutes: 18013 frames, 0 dropped, 300/300 flashes; Wayland screen + mic + system audio: flash-to-beep median -5.9 ms, no drift; kill -9 recovery < 1 s lost on Wayland.

## Decisions I took alone

(Also in the night log.) Merge order and conflict handling (version bump made early so builds could be warmed up; docs afterwards). Quiet hours: the maintainer's 22:00 limit was applied from the moment it arrived: no sound case ran after 22:00 and the final regression has none. `youtube-1080p` keeps VP9 + AAC in MP4 labelled "VP9, not H.264" (O10 said WebM; real WebM is in the 0.3.1 plan). One shared Wayland block at night (desktop agent, then recording, then my regression), X11 restored afterwards. The sidebar "Record" dimming written by a UI agent changes macOS and Windows too and was reverted (0.3.1 plan item 19). The camera has no live bubble on Linux (stated gap). Text-model download check was allowed (once, deleted after). `rec.system_audio`, the chime player and the 30-minute soak were not run. The first Wayland reports (run with no display variables, a lab-environment fault) and the warm-build X11 report were deleted and the runs repeated. The lab's installed 0.2.9 `.deb` was upgraded, removed and purged for the `.deb` install case and then reinstalled as found.

## Not done / not run (also in the release text)

- AMD and NVIDIA encoders, ARM hardware, a second monitor or scale other than 1, KDE and wlroots (the GlobalShortcuts portal path is written and tested with a fake portal only), Fedora/Arch/Debian, Flatpak/Snap.
- Sound cases (rec.system_audio, chime, speakers): quiet hours. The X11 flash-to-beep offset was not measured. No 30-minute soak (a 10-minute soak case passed on X11).
- Wayland window recording (needs the buffer's crop metadata), cursor track and global shortcuts on Wayland (GNOME 46 has neither path).
- The GitHub workflows `linux-package.yml` (including the `apt-repo` job) have never run on GitHub: expect a first-run fix. The APT repository was proved in a chroot with a throwaway key, not with the real key or host.
- Manual IME scripts (Telex/VNI under IBus) on the final build, the editor's "Screen" lane has no thumbnails on any OS (0.3.1), the first-run Linux intro note still says "your desktop asks" (0.3.1), the Auto-redact editor button was not clicked on a real desktop.
- The two Mac `cm-desktop` render tests (above).

## What you must do (see `docs/development/release-day-0.3.0.md`)

1. Read `docs/development/release-0.3.0-github-text.md` (it fills in the numbers and the checksums of the files below) and decide the APT host and the APT key (recommendation in `linux-apt.md`: your own domain with a static host; otherwise GitHub release assets, flat layout; sign locally, no key in CI).
2. Re-run `cargo test -p cm-desktop --lib recedit::render` on a quiet Mac; check `git log` and push `main` to `public`; wait for CI and expect to fix the Linux workflow on its first run.
3. Copy the four package files from the laptop (`~/wt/release-out/5301d4b8-x86_64/`) and the VM (`~/wt/release-out/5301d4b8-aarch64/`), check the sums, sign with `packaging/linux/sign-linux.sh`, tag `v0.3.0`, create the draft pre-release, then publish. If you rebuild from the tag, the checksums change: replace them in the text.
4. Optional: run the APT repository steps and the user test on a clean Ubuntu 24.04.

Files (SHA-256): `capturemann-0.3.0-linux-x86_64.tar.gz` `cae0cfea6617891ffaea0cba048686007b039b99cb297d1ee3c685f8f0805072`;
`capturemann_0.3.0_amd64.deb` `30b2cd10316233755cc730271315c03cbcdb6c0d16aa4f54267b37fc128a7158`;
`capturemann-0.3.0-linux-aarch64.tar.gz` `10513d7059f595d834d525b43e03f5da61341f8ec363efc09ebecd8f6171c968`;
`capturemann_0.3.0_arm64.deb` `46f7909df48c893fac6261062607c0d888a15eeb8878131fe11b086c3a50f06a`.
**The four files with their `.sha256`, `SHA256SUMS.txt` and `BUILD-INFO.txt` are on this Mac in `~/release-0.3.0/x86_64` and `~/release-0.3.0/aarch64`** (checked against the sums), and the x86_64 set is also in `~/lab/release-0.3.0-linux/` on the laptop; the build folders are `~/wt/release-out/5301d4b8-*` on the laptop and the VM (with the unstripped programs in `debug/`).

## Risks

- One laptop, one GPU (Intel UHD 620) and one desktop (GNOME 46 on Ubuntu 24.04): other Linux setups are untested. The release text says so.
- Skipping GNOME's share dialog means the compositor's consent prompt is gone: our own Confirm tier, banner and tray state remain, and GNOME's orange dot. Say it in the release text (it does).
- Network downloads in the package build are flaky from the lab (stalls at ~190 KB/s): source tarballs were cached for the final build; a rebuild from a clean machine downloads again and may stall.
- The preview-render test failure on the Mac depends on the build location and environment, not on the code (see the follow-up above); confirm it in your own terminal.
