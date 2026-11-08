# Release 0.3.0 plan: Linux catches up (no longer an early build)

Status: **draft** (rewritten Nov 8, 2026 after 0.2.9 shipped on Windows and macOS). Nothing built.

**Scope rule (maintainer, Nov 8, 2026):** 0.3.0 is a Linux-only release with one goal: Linux gets the features Windows and
macOS have, so it stops being an "early build"; then a complete regression and the release. Anything that is not
Linux-only goes to [`release-0.3.1-plan.md`](release-0.3.1-plan.md). A shared-code fix comes in here only when a Linux
task cannot be finished without it, and then it is named in the task.

Rules that apply: general techniques only, no one-recording fixes; **nothing is called "verified" without a stored
report in `verification/`** (name the distribution, desktop, session type, GPU and app build); no GPL/AGPL in shipped
binaries; `target_os` only in platform code (`python3 scripts/check_target_os.py`); every user-facing action has an MCP
tool and a CLI command; UI text through `cm_i18n`; one change per commit with the trailer lines; no failing tests
merge to `main`; the maintainer pushes `main`, signs (minisign) and publishes.

## Where Linux stands (inputs)

- 0.2.6 `.deb` and tarball early build; recording exists behind `CM_LINUX_RECORDING=1`; the VM run (headless GNOME 46)
  recorded 10 s at 30 fps with PipeWire and VP9 (`docs/development/linux-early-build.md`,
  `specs/plans/linux-recording-plan.md`, decisions D-R1 to D-R5, `specs/plans/auto-redact-linux-plan.md`).
- Wayland window capture and global shortcuts have no path on GNOME 46; aarch64 is tested only in a VM.
- The `cm-regress` Linux driver exists (`crates/cm-regress/src/cases/linux.rs`, `scripts/testlab/regress-linux.sh`).
  Reports of Nov 6, 2026 on the ThinkPad T490 (X11, `verification/L1-linux-0.2.9-regression-*`): the hardware tier was
  PARTIAL with **35 passed, 17 failed, 25 skipped of 77**. Task 1 starts from those failures.

## Tasks

Lab: the ThinkPad T490 at 192.168.0.102 (`scripts/testlab/lab.sh`, Ubuntu 24.04, GNOME 46, Xorg and Wayland; Windows work is finished, so
the laptop is free). The VM at `~/VMs/cm-linux` covers aarch64 and headless work. No sound-playing tests after 22:00 (maintainer, Nov 8, 2026; it was 23:00)
Vietnam time.

1. **Triage the 17 regression failures** (first, one by one, each a product bug, a wrong test expectation or a lab limit,
   written into a short triage note in `verification/`):
   - `pkg.files`: the package has no `LICENSE` or `THIRD_PARTY_LICENSES` (compliance; fix in `packaging/linux`).
   - `import.formats`, `import.rotated`, `import.vfr`: import fails with "no hardware video encoder" when VA-API is
     missing. A Linux computer without a hardware encoder must still import (software VP9 editing copy, as D-R1 chose
     for recording), or say clearly why not.
   - `export.quick`, `export.presets`: exports come out as VP9, not H.264. Decide from the export spec whether H.264 is
     required on Linux (no libx264 in the LGPL build; VA-API H.264 where present) or the test expectation changes.
   - `rec.screen_cursor`, `rec.kill9_recovery` (portal refused screen capture, process killed by a signal),
     `rec.region`, `rec.region_min` (`not_implemented` for window and region recording).
   - `mcp.policy_allow`: reproduce on macOS first; fix here only if it is Linux-only, otherwise it goes to 0.3.1.
   - The remaining failures in the same report.
2. **Bugs from 0.2.6:** the Wayland Shortcuts page shows Recording rows and Change buttons although recording is off; the
   X11 tray "delay" check mark and "Recent captures" listing only the app's own capture; `user_declined` returned for a
   portal failure (portal response code 2 is not a user decline; `crates/cm-capture/src/sys/portal.rs`); the line about
   secrets "in the system credential store" when there is no Secret Service; the unlabelled steps in the X11 report
   (editor, tarball default prefix, tray activation, language), each run one by one with a label. Each fix cites its
   AC ID and has a test or a stored real-device step.
3. **Recording, on by default:** remove the `CM_LINUX_RECORDING=1` gate once the gate below passes. Screen, microphone,
   system audio and cursor tracks; pause and resume; `recording.lock` and `kill -9` recovery (REC-8; add
   `tests/manual/REC-8.md`, it does not exist). VA-API H.264 where present, software VP9 as the labelled fallback. Fix what
   a real desktop shows (banner controls, Start sheet keycaps, settings that mean nothing for VP9). Gate: a 10-minute
   recording with screen and microphone plays back in sync on X11 and on Wayland, measured (`scripts/testlab/av-sync-analyze.py`),
   report stored. The 30-minute soak stays a later gate.
4. **Window and region recording / capture:** region recording through the portal crop or a compositor-side crop; window
   capture on Wayland GNOME 46 (ScreenCast portal window source for one frame, or the file the shell saved). Choose by
   experiment, store the report before building. Where a path does not exist, the UI and the error say which versions
   work and which do not (never a silent empty result).
5. **Camera:** webcam bubble and camera track on Linux (V4L2 or PipeWire), to match CAM-1 to CAM-3; if it cannot be done, the
   release text says so.
6. **Global shortcuts:** the GlobalShortcuts portal where it exists (KDE, newer GNOME), tested on one KDE machine or VM;
   where none exists the Shortcuts page says what works instead.
7. **Parity sweep:** go through `docs/project/status.md` and `specs/platform-acceptance.md` row by row; every feature that
   works on Windows or macOS is on Linux, is a new task here, or is listed as an explicit gap in the release text (with the
   reason). The result is the "no longer early build" statement the release makes.
8. **Auto-redact and speaker engine:** re-run Auto-redact (`CM_TESSERACT`/`CM_TESSDATA`) and the sherpa-onnx `linux-x64` and
   `linux-aarch64` builds on the laptop and the VM; fix what fails; bundle `llama-server` in the tarball and `.deb` if the
   built-in text engine is to work (it is not bundled yet).
9. **Packaging and APT repository:** keep minisign for the tarball; a signed APT repository (O3) for amd64 and arm64 with an
   install, upgrade and remove check in CI; a package checker script (architecture, glibc ceiling, no bundled `libwayland`
   or GPU libraries, licence files present) runs in CI.
10. **Hardware matrix:** NVIDIA and AMD encoders, a real ARM device, two monitors and mixed scale; each a stored step or an
    explicit "not run" in the release text.
11. **Complete regression and release:** `cm-regress` full and hardware tiers on X11 and on Wayland, on the `.deb` and the
    tarball, plus the aarch64 VM; every failure is fixed, or listed in the release text with its reason. Then version
    bump to 0.3.0, CHANGELOG, release text (`docs/development/release-0.3.0-github-text.md`), checksums, minisign, tag.
    Release text lists the distribution, desktop, session type and GPU of every run.

## Done when

Tasks 1 to 4 and 7 closed with tests or stored reports; recording is on by default and passed its gate on X11 and Wayland;
the full regression has no unexplained failure; the release text states exactly what runs on which Linux setup and what
does not; and the Linux status in the README and `docs/project/status.md` no longer says "early build".

## How the agent works

One agent, one worktree: `git worktree add ../capturemann-wt/linux -b track/linux` from `main`. Owns:
`crates/cm-capture/src/sys/` (Linux files), Linux files in `crates/cm-audio`, `crates/cm-camera`, Linux code in
`crates/cm-platform`, `packaging/linux`, `docs/development/linux-early-build.md`, Linux steps in `scripts/testlab/`, Linux
cases in `cm-regress`, Linux manual scripts and reports. Must not touch macOS or Windows `sys/` files, run `make bundle`,
push, publish, or say "verified" without a report. Shared files with one owner (the maintainer resolves at merge):
`Cargo.lock`, workspace `Cargo.toml`, `locales/*.pot`/`vi.po`, `CHANGELOG.md`, `docs/project/status.md`,
`THIRD_PARTY_LICENSES.md`. Version stays 0.2.9 on the branch until the release commit. Commands before every handoff:
`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, `python3 scripts/check_target_os.py`. Final report: commits, tests run, reports written,
what was not done. Clean the worktree's `target/` when merged.

## Decisions

Decided by the maintainer on Nov 8, 2026 (all four decisions are closed):

- **O5** 0.3.0 catches up **every feature that is available on Windows and macOS**, recording included (screen, window and
  region, microphone, system audio, camera, cursor, pause, recovery, shortcuts). A feature may be left out only with an
  explicit, reasoned entry in the release text (for example a compositor that offers no path), never silently.
- **O3** Distribution beyond `.deb` and tarball: an **APT repository** in 0.3.0 (signed repository, published with the
  release; add a task for the repository layout, signing key handling and an install-and-upgrade check in CI).
- **O9** Hardware gaps: the same ThinkPad T490 at `192.168.0.102` (lab laptop, `scripts/testlab/lab.sh`) is the machine
  for every real-device step. Gaps that this machine cannot cover (ARM hardware, NVIDIA/AMD, KDE shortcuts, a second
  monitor) stay "not run" in the release text unless the maintainer names another machine.

- **O10** (decided Nov 8, 2026) H.264 on Linux: hardware H.264 (VA-API, NVENC) where it works, software VP9 as the
  labelled fallback for import, editing copy and export where it does not (no x264, no OpenH264: GPL and patents). The
  export dialog and the CLI say which codec is used and why; the `youtube-1080p` preset says it makes WebM when there is
  no hardware H.264 encoder.

## Risks

- One laptop is not a hardware matrix; the release text lists the GPU of each run.
- GNOME gives no stable window-capture or shortcut path on every version; claim only the versions tested.
- Wayland gives apps no recording indicator or permission control of their own; the product text must match what the
  portal shows.
- Region recording and the camera are the largest unknowns; if they slip, the release still ships with named gaps rather
  than waiting.
