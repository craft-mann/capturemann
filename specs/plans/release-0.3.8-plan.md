# Release 0.3.8 (plan)

Status: **draft, written Nov 25, 2026 after the release of 0.3.7, for the maintainer's review.** Scope proposed: the open items left by 0.3.7 (section 2, Fixes) and the regression-suite and release-process findings of the 0.3.7 public check; section 1 (Changes) is empty until the maintainer sets it. An item that adds a capability the specs do not already promise belongs in section 1 and needs the maintainer's word. Every item states the end state and names where it was found. Reports are in `verification/` (`*-0.3.7-*`). Order of work is at the end.

## 1. Changes

None decided. The maintainer sets this section at review. Backlog that could go here (not committed): in-app update (Velopack spikes done, `specs/plans/release-0.3.3-plan.md`), windowed audio, WebP and HEIC image import, a headless daemon on macOS, Click Guide on Linux.

## 2. Fixes

The "Known open items after 0.3.7" of `docs/development/release-day-0.3.7.md`, one item each, then the findings of the public check.

### 2a. Transcripts

- **Invented words over room-only noise (D4).** A 9-second Vietnamese clip that holds only room noise (about -60 dBFS) no longer gets an invented sign-off. The guard has a measured rule for audio that has no speech reference; the rule is written into `specs/features/07-ai.md` with its numbers and is adopted only if it loses no quiet real speech on the silence and quiet-speech sets (`silence_probe`, `quiet_silence`, `outro_guard`). The measurements, which were not run in 0.3.7, run first. Until then the known limit stays in the release text. (0.3.6 D4.)
- **The last word of a piece.** The check that every word has an end after its start runs on a long real recording on all three systems, and any piece that fails it is fixed in the engine. It needs the maintainer's consent for a real recording. (0.3.5 G3.)

### 2b. Quick Capture

- **Wayland placement.** On a Wayland session the bar opens centred on the display and the after-capture bar under the captured area wherever the portal or the compositor allows it; where nothing can place them the spec says where they open (today the top-left on GNOME, Linux limits and AC12). Checked on GNOME Wayland on the ThinkPad. (0.3.7 not done; Linux QA N-3.)

### 2c. Windows

- **Long Shot first step.** The first scroll step of Long Shot on Windows is not lost. The scroll-back that recovers it stays. (Windows F-9.)
- **Edge second tab (P-3).** A maximised Edge window with a second tab no longer ends Long Shot after 2 frames. It is tried again with the traces kept (`refused-NN.txt`, `trace.txt`); if it does not reproduce, the item is closed with the traces named in the report. (P-3, not reproduced in 0.3.7.)

### 2d. Library and the daemon

- **`list_library` kind `guide` with status.** `list_library` with kind `guide` and a status filter returns the same captures in the daemon, the MCP tool and the CLI as the library page shows for Click Guides with that status chip; a test covers the three. (Open since 0.3.7.)
- **Docs.** `specs/features/09-library.md` describes the 0.3.7 sidebar and filter bar (SB-1 to SB-7 of `release-0.3.7-plan.md`): two buttons, one row of six modes, the fixed library order, Needs attention first and amber, Agents & CLI state, the icon rail under 960 pt.

### 2e. Translations

- **Vietnamese.** The 16 strings still untranslated (3660 of 3676 translated, `cargo run -p cm-i18n -- coverage`) are translated, and the draft strings (375) are listed for the maintainer's review. `cm-i18n check` passes.

### 2f. The regression suite (`cm-regress`)

- **`installer.silent` baseline.** The Start-menu check compares the entries after the uninstall with the entries before the install and flags only entries that appeared with the install. A shortcut that existed before the run (a lab dev install's `CaptureMann (dev).lnk`) is never reported as left behind. (Windows public check, Nov 25, 2026: the case failed on that shortcut and passed once it was moved away.)
- **Screen Recording not granted (macOS).** When the package under test has no Screen Recording permission, the cases that need it skip with a message naming the permission, instead of failing with `permission_missing`, and the report header says so. (macOS public check, Nov 25, 2026: 9 failures on an app copy in `/tmp`.)

### 2g. Release process and packaging

- **APT listing.** `apt list -a capturemann` on an amd64 computer does not list arm64 rows, or `linux-apt.md` says why it does (the sources file names both architectures). (0.3.6 QA, cosmetic.)
- **winget.** The pull requests 472327, 477195 and 483031 are open and no 0.3.7 pull request exists. This is an account matter; the state is recorded in the release text and the README once the moderators answer.
- **Public check on the DMGs.** The macOS regression of the release is run on the notarized DMG's app copy with a Screen Recording grant, as in 0.3.6; a `brew install` test runs in a throwaway location. (0.3.7 not run.)

## 3. Not in 0.3.8

New features (section 1 backlog), and the 0.3.7 "Not run" list that needs hardware or a system the lab does not have: Intel Mac, Windows 10, SmartScreen, a second monitor, mixed DPI, NVIDIA and AMD encoders, KDE.

## 4. Order of work and exit

The D4 measurements (2a) start first because they decide whether a rule is built. The library and daemon item (2d), the translations (2e), the suite fixes (2f) and the docs run in parallel on disjoint files; Wayland (2b) needs the ThinkPad on Ubuntu with a Wayland session, and the Windows items (2c) need it on Windows. During development only fmt, clippy and the tests of the touched crates run. After development the maintainer tries the build by hand; every item is fixed or answered with a line in the release text. Then the full gate runs once on the final tree (fmt, clippy, tests, `cargo deny`, Trivy), the Windows package workflow, the Linux and macOS builds, and `cm-regress` on the final files of each system, stored in `verification/`.
