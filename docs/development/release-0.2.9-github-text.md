# GitHub release text for 0.2.9 (draft, not published)

Prepared Nov 6, 2026 from `specs/plans/release-0.2.9-plan.md`, `git log v0.2.8..HEAD` and the 0.2.8 text
(`release-0.2.8-github-text.md`). **Nothing here is final until the regression reports exist.** Every `<FILL: ...>` below is
a placeholder to fill from a stored report in `verification/`; if there is no report for a line, delete the line or write
"not run". Never write "passed" or "verified" without a report (CLAUDE.md wording rule). Today `verification/` holds only
dry-run reports (macOS `--dev` standard run on 0.2.7, Linux smoke against the installed 0.2.6 package), which are not release
evidence for 0.2.9. Steps: `release-day-0.2.9.md`.

- **Tag:** `v0.2.9` (never reuse a tag). **Title:** `CaptureMann 0.2.9: fixes on Windows, macOS and Linux`.
- **Type:** draft, then **pre-release** (the version is 0.x). One tag, released when all three platforms have a stored
  regression run (decision of Nov 6, 2026).
- **Files to attach:**
  - Windows: `capturemann-0.2.9-windows-x64-setup.exe`, `capturemann-0.2.9-windows-x64.zip`, their two `.sha256` files and
    `SHA256SUMS.txt` (from `packaging/windows/release-files.sh`). Not code-signed.
  - macOS: `CaptureMann-v0.2.9-macos-arm64.dmg` and `CaptureMann-v0.2.9-macos-x86_64.dmg` with their `.sha256` files (made by
    the `Release` workflow; signed, notarized and stapled if the notarization secrets are set, otherwise say "not notarized").
  - Linux: `capturemann-0.2.9-linux-x86_64.tar.gz`, `capturemann-0.2.9-linux-aarch64.tar.gz`,
    `capturemann_0.2.9_amd64.deb`, `capturemann_0.2.9_arm64.deb`, their `.sha256` files and the `.minisig` files
    (`packaging/linux/sign-linux.sh`).

---

## Release body

# CaptureMann 0.2.9: fixes on Windows, macOS and Linux

0.2.9 is a bug-fix release. It fixes four problems reported on 0.2.8 and earlier, and it is the first release since 0.2.6
with **files for all three platforms**: Windows 11 (x64), macOS 13+ (Apple Silicon and Intel) and Linux (Ubuntu 24.04+,
x86_64 and aarch64, early build). No new features. **macOS and Linux users also get everything that came in 0.2.8** (see
"Also new for macOS and Linux" below), because 0.2.8 was Windows only.

## Fixed

1. **Reveal in the export toast** opens the folder that holds the exported file, with the file selected (File Explorer and
   Finder). On Linux it opens the folder; a Linux file manager cannot be told to select a file in general. On Windows the
   button opened the wrong folder.
2. **Screenshot annotation editor, Vietnamese text.** Typing with Telex or VNI in the Text tool now reaches the annotation
   (the tool leaves composition to the system input method), and the text is drawn and exported with its accents.
   **New text** starts at size 24, in white, with no outline; change the colour and size in the panel as before. Existing
   annotations are not changed. Tested on Windows and macOS input methods: <FILL: from the stored manual report for
   `tests/manual/I18N-IME.md` and the ANN-1 steps; if none, write "tested with fakes only">. **Linux input methods (IBus,
   Fcitx) were not tested.**
3. **The Models page no longer says "Mac" on Windows or Linux.** The one-line model descriptions were written for the Mac
   ("every Mac", "8 GB Macs") and were English-only. They are now neutral ("every computer", "8 GB of memory") and are
   translated, including Vietnamese. A test fails if a model description in the catalogues names a platform.
4. **The Speakers tab** gives the speaker list the free height of the panel and scrolls when there are many speakers (before,
   about one and a half rows were visible on a laptop screen). The note "Voices are compared in memory only. No voiceprint is
   saved, and people are never recognised across recordings." moved to **Settings, Privacy**, as a fifth fact; nothing about
   how voices are handled changed. At the smallest window (720 x 480) the panel scrolls as a whole and nothing is cut.

The Windows test job on CI is also faster; that is internal and changes nothing you download.

## Security

GitHub code scanning (Trivy and Dependabot) reported 15 open alerts, all in one file: `packaging/phowhisper/requirements.txt`,
the Python requirements of a script the maintainer runs by hand to convert PhoWhisper checkpoints. **Nothing in the app, the
installers, the zip, the DMGs or the Linux packages uses that file, so nothing in the files you download is affected and
there is nothing for you to update.** The pins were raised (torch 2.15.0, transformers 5.22.1 and their companions) and the
`small` model was converted again; the result is a byte-identical model file (same SHA-256), so the Vietnamese benchmark
still applies (`packaging/phowhisper/README.md`). One transformers alert (alert 8) has no patched version yet: the maintainer
dismisses it as "not used in shipped code" or it closes when a fix is published <FILL: state of the alert list on the
release day>. The converter README says to convert only the three pinned VinAI revisions in a throwaway virtual environment.

## Also new for macOS and Linux (from 0.2.8, which was Windows only)

macOS was last released at 0.2.7 and Linux at 0.2.6. Shared changes from 0.2.8:

- Two new agent tools with CLI commands: `get_cursor_track` (`capturemann cursor`) and `get_platform_rules`
  (`capturemann platform-rules`). The MCP server now lists 79 tools.
- `capturemann settings set` is followed by the running app at once (a `settings.changed` event), and the capture delay is
  read for each capture.
- A recording whose encoder cannot keep up is marked with an `encoder_overloaded` warning in its manifest and the drop
  counters are honest; a recording area that is too small is `invalid_input` (`recording_area_too_small`, smallest side
  18 px), not a missing encoder; a microphone that delivers only silence is reported after 4 s (`mic_silent`); the daemon
  publishes `camera.reconnected` when a camera comes back.
- Error and help texts name your system (Windows, Linux or Mac) instead of always saying "Mac".
- `ai transcribe` answers `no_model` when no speech model is installed, instead of taking a folder of speaker models for one.
- Vietnamese: the interface text was reviewed by the maintainer on Nov 6, 2026, except 86 entries that are still drafts.
- `cm-regress`, the release regression suite, now drives macOS and Linux packages as well as Windows
  (`docs/development/regression.md`).

The Windows-only items of 0.2.8 (recording on Windows, Auto-redact with Windows.Media.Ocr, installer, zip) are in the 0.2.8
release notes and are unchanged here.

## What was and was not run per platform

**To be filled from the stored reports; delete a cell's claim if its report does not exist.** All runs are by an agent on
one lab machine per platform unless a line says a person looked. A PARTIAL report is not "verified".

| Platform | Package built from the tag (CI run, commit) | Smoke | Standard | Hardware | Report files |
| --- | --- | --- | --- | --- | --- |
| Windows 11 x64 (ThinkPad T490, Intel UHD 620) | <FILL: run id, commit> | <FILL: n of m> | <FILL: n of m> | <FILL: passed / failed / skipped, which cases skipped> | <FILL: `verification/L1-windows-0.2.9-regression-*`> |
| macOS arm64 (the maintainer's Mac, `-dev` bundle for the runs) | <FILL> | <FILL> | <FILL> | <FILL: screen, microphone, system audio and camera run only with the maintainer's consent> | <FILL: `verification/L1-macos-0.2.9-regression-*`> |
| macOS x86_64 (Intel) | <FILL: built and checked on Apple Silicon only> | not run on Intel hardware | not run | not run | none |
| Linux x86_64 (ThinkPad T490, Ubuntu 24.04, X11 and GNOME Wayland) | <FILL> | <FILL> | <FILL> | <FILL: recording cases skip unless `CM_LINUX_RECORDING=1`> | <FILL: `verification/L1-linux-0.2.9-regression-*`> |
| Linux aarch64 | <FILL: built on CI; run on hardware? likely "not run"> | <FILL> | <FILL> | <FILL> | <FILL> |

Bug fixes: Reveal (EXP-4.AC5) and the text defaults (ANN-1.AC7, AC8) have unit tests with fakes; the Speakers tab layout and
the hint wording have layout and catalogue tests; the regress cases or stored manual steps for each: <FILL: report files, or
"not run">. CI on the tag commit (`cargo test --workspace` on macOS and Windows): <FILL: link or "see Actions">.

### Not run (same as 0.2.8 unless a report says otherwise)

- **Windows:** Windows 10 22H2; SmartScreen on a real browser download; a second monitor with mixed scaling
  (`tests/manual/DPI-WIN.md`); NVIDIA and AMD hardware encoders and drivers (only Intel UHD 620 was used); the Samsung
  monitor cases.
- **macOS:** the Intel DMG on a real Intel Mac (no Intel Mac is available; the Intel crash of issue 16 is not confirmed
  fixed on hardware); <FILL: any macOS case that stayed SKIP, for example `hud_click_through`, `recording_placement`,
  `shot.window`, `redact.window_text`, `rec.each_monitor`>.
- **Linux:** recording is behind `CM_LINUX_RECORDING=1` and was not run as a release test <FILL: unless a report says so>;
  input methods (IBus, Fcitx) in the annotation editor; MP4 export needs a VAAPI GPU or your own FFmpeg.
- **All:** the invented outro-style sentence in transcripts of quiet stretches is still a known fault (no transcription
  mitigation in this release); playback follows the system's default output device.

## Install

- **Windows 11, x64:** installer `capturemann-0.2.9-windows-x64-setup.exe` (per user, no administrator rights) or the zip
  `capturemann-0.2.9-windows-x64.zip` (unpack anywhere, run `cm-desktop.exe`; `capturemann.exe` is the command line).
  winget: `winget upgrade CaptureMann.CaptureMann` <FILL: only once the winget package is accepted; pull request `<URL>`>.
  The files are **not code-signed**; a browser download may show a SmartScreen warning (More info, then Run anyway).
  SHA-256: installer `<FILL>`, zip `<FILL>`. Check with `certutil -hashfile <file> SHA256`.
- **macOS 13+:** `CaptureMann-v0.2.9-macos-arm64.dmg` (Apple Silicon) or `CaptureMann-v0.2.9-macos-x86_64.dmg` (Intel);
  <FILL: "signed and notarized" or "not notarized">. Or `brew upgrade --cask craft-mann/tap/capturemann`
  (`brew install --cask craft-mann/tap/capturemann` for a first install). SHA-256: arm64 `<FILL>`, x86_64 `<FILL>`.
  Check with `shasum -a 256 -c <file>.sha256`.
- **Linux (early build; Ubuntu 24.04+ or Debian 13, glibc 2.39 or newer):** `sudo apt install ./capturemann_0.2.9_amd64.deb`
  (or `_arm64.deb`), or the tarball for your CPU and its `install.sh`. Verify with
  `minisign -Vm <file> -P <public key from packaging/linux/minisign.pub>` and `sha256sum -c`. SHA-256: `<FILL>`.
  Screen recording is off unless `CM_LINUX_RECORDING=1`; MP4 export needs a VAAPI GPU or your own FFmpeg; the local text model
  needs a model file you download.

Report problems at the issue tracker with the output of `capturemann diagnostics`.

## Licences

Apache-2.0. The packages carry `THIRD_PARTY_LICENSES.md`. FFmpeg is an LGPL build (no GPL code, no libx264); its source
offer is in the package. The Rust dependency set did not change from 0.2.8 (`Cargo.lock` differs only in the workspace
versions).

---

## CHANGELOG section (add under `## [Unreleased]`; `release.yml` reads it for the macOS run)

```
## [0.2.9] - YYYY-MM-DD

0.2.9 is a bug-fix release for Windows, macOS and Linux. macOS and Linux also receive the changes of 0.2.8 (Windows only).

### Fixed
- Reveal in the export toast opens the folder of the exported file with the file selected (Linux: the folder).
- Annotation editor: Vietnamese text typed with Telex or VNI reaches the Text tool and is exported with its accents; new
  text starts at size 24, white, without an outline.
- The Models page no longer says "Mac" on Windows and Linux; the model descriptions are neutral and translated.
- The Speakers tab list scrolls and has a flexible height; the voiceprint note moved to Settings, Privacy.

### Security
- The PhoWhisper converter requirements (`packaging/phowhisper/requirements.txt`, used only by a maintainer script, not
  shipped) were raised to patched torch and transformers; nothing in the shipped files changed.
```
