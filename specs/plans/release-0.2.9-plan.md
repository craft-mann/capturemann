# Release 0.2.9 plan: Linux catch-up and bugs

Status: **draft** (Nov 3, 2026), nothing built. Scope set by the maintainer on Nov 3, 2026: 0.2.7 macOS only
(`release-0.2.7-plan.md`), 0.2.8 Windows (`release-0.2.8-plan.md`), **0.2.9 Linux**. A separate session and agent own
this plan and run alongside the 0.2.7 work (see "Working next to 0.2.7 and 0.2.8").

Rules that apply: general techniques only, no one-recording fixes; **nothing is called "verified" without a stored
report in `verification/`** (name the distribution, desktop, session type, GPU and app build); no GPL/AGPL in shipped
binaries; `target_os` only in platform code (`python3 scripts/check_target_os.py`); every user-facing action has an MCP
tool and a CLI command; UI text through `cm_i18n`; one change per commit with the two trailer lines; no failing tests
merge to `main`; the maintainer pushes `main`, signs (minisign) and publishes.

## Where 0.2.6 left Linux (inputs)

From `docs/development/linux-early-build.md`, `verification/LINUX-1-*`, `specs/plans/linux-recording-plan.md`
(decisions D-R1 to D-R5) and `specs/plans/auto-redact-linux-plan.md`:

- `.deb` and tarball early build; recording exists behind `CM_LINUX_RECORDING=1` but has never run on a real desktop;
  the VM run (headless GNOME 46) recorded 10 s at 30 fps with PipeWire and VP9.
- Wayland window capture and global shortcuts have no path on GNOME 46; aarch64 is tested only in a VM.
- Bugs found in the 0.2.6 reports (listed in task 1).

## Tasks

Lab: the ThinkPad T490 (`scripts/testlab/lab.sh`, Ubuntu 24.04, GNOME 46, Xorg and Wayland). The VM at
`~/VMs/cm-linux` covers aarch64 and headless work. The Windows agent (0.2.8) uses the same laptop; this plan starts
its laptop work when that agent reports it has handed the laptop back, and leaves it in the state 0.2.8 expects when
done (Windows bootable, network Private). VM and CI work can start at any time.

1. **Bugs from 0.2.6:** the Wayland Shortcuts page shows Recording rows and Change buttons although recording is off;
   the X11 tray "delay" check mark (stale or missed click) and "Recent captures" listing only the app's own capture; the
   `user_declined` code returned for a portal failure (portal response code 2 is not a user decline); the line about
   secrets "in the system credential store" when there is no Secret Service; the unlabelled steps in the X11 report
   (editor, tarball default prefix, tray activation, language), each run one by one with a label.
2. **Wayland window capture on GNOME 46:** `InteractiveScreenshot` returns no file. Experiment on the laptop: the
   ScreenCast portal with the window source type for a single frame, or reading the file the shell saved. Pick the
   approach by experiment and store it as a report before building.
3. **Recording on a real desktop** (see O5): turn on the existing recorder on the T490. VA-API H.264 hardware encode
   was seen working; software VP9 is the labelled fallback (D-R1 to D-R5). Fix what a real desktop shows (banner
   controls, Start sheet keycaps, settings that mean nothing for VP9). Gate: a 10-minute recording with screen and
   microphone plays back in sync on X11 and on Wayland (measured, report stored), plus `recording.lock` and `kill -9`
   recovery (REC-8). The 30-minute soak from `linux-recording-plan.md` stays a later gate.
4. **Global shortcuts:** the GlobalShortcuts portal on desktops that have it (KDE, newer GNOME); test one KDE machine or
   VM. Where no portal exists, the Shortcuts page says what works instead.
5. **Hardware gaps:** a real ARM device (or keep "VM only" in the notes), NVIDIA and AMD, two monitors and mixed scale;
   each a stored step or an explicit "not run".
6. **Packaging:** keep minisign for signing; decide on an APT repository or Flatpak (O3). A package checker script
   (architecture, glibc ceiling, no bundled `libwayland` or GPU libraries) runs in CI.
7. **Auto-redact and speaker engine on Linux:** re-run `CM_TESSERACT`/`CM_TESSDATA` Auto-redact and the sherpa-onnx
   `linux-x64` and `linux-aarch64` builds on the laptop and the VM, and record the result; fix what fails.

## Done when

Bugs closed with a test or a stored real-device step; the Wayland window path chosen, reported and built or explicitly
left for the next Linux release; and either recording seen on the laptop with a report or recording left for the next
Linux release with the release text saying so.

## How the agent works

One agent, one worktree: `git worktree add ../capturemann-wt/linux -b track/linux` from `main`. Owns:
`crates/cm-capture/src/sys/` (Linux files), Linux files in `crates/cm-audio`, Linux code in `crates/cm-platform`,
`packaging/linux`, `docs/development/linux-early-build.md`, Linux steps in `scripts/testlab/`, Linux manual scripts and
reports. Must not touch macOS or Windows `sys/` files, run `make bundle`, push, publish, or say "verified" without a
report. Shared files with one owner (the maintainer resolves at merge): `Cargo.lock`, workspace `Cargo.toml`,
`locales/*.pot`/`vi.po`, `CHANGELOG.md`, `docs/project/status.md`, `THIRD_PARTY_LICENSES.md`. Version stays 0.2.6 on the
branch until the release commit. Commands before every handoff: `cargo fmt --all -- --check`, `cargo clippy
--workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `python3
scripts/check_target_os.py`. Final report: commits, tests run, reports written, what was not done.

## Working next to 0.2.7 and 0.2.8

Same rules as the Windows plan: merge 0.2.7's M1 shared-code commits (`apps/desktop` window lifetime) first and
rebase; keep `apps/desktop` edits to Linux-specific files until then; `cm-platform` is shared with the Windows agent, so
Linux and Windows code live in separate files and the maintainer merges one track at a time. Release order is 0.2.7,
0.2.8, 0.2.9; do not tag 0.2.9 before 0.2.8 is published. Linux early builds are rebuilt only with 0.2.9 unless a
shared-code change forces it.

## Decisions needed from the maintainer

- **O3** Linux distribution beyond `.deb` and tarball: APT repository or Flatpak in 0.2.9, or later.
- **O5** Is recording on Linux a **must** for 0.2.9? Default proposal: no. It ships only if the gate passes.
- **O9** Which machines are acceptable for the hardware gaps (a KDE VM for shortcuts, an ARM device, NVIDIA/AMD).

## Risks

- The laptop is shared with the Windows plan; a delay in 0.2.8 delays the laptop work here. VM and CI work does not wait.
- GNOME gives no stable window-capture or shortcut path on every version; claim only the versions tested.
- Hardware encode paths differ by GPU and driver; one laptop is not a matrix, and the release text lists the GPU.
- Wayland gives apps no recording indicator or permission control of their own; the product text must match what the
  portal shows.
