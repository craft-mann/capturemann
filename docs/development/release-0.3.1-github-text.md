# GitHub release text for 0.3.1 (draft, not published)

Prepared the night of Nov 9 to 10, 2026 from `specs/plans/release-0.3.1-plan.md`, `CHANGELOG.md` ([Unreleased]) and the 0.2.9 and
0.3.0 texts. **Nothing here is final until the regression on the final files exists.** Every `<FILL: ...>` is a placeholder for a
fact that only exists on the release day (checksums, run ids, notarization, winget). Never write "passed" or "verified"
without a stored report in `verification/` (CLAUDE.md wording rule). Steps: `release-day-0.3.1.md`.

- **Tag:** `v0.3.1` (never reuse a tag). **Title:** `CaptureMann 0.3.1: export sizes, MP3 import, transcript progress`.
- **Type:** draft, then **pre-release** (the version is 0.x). Covers Windows 11 (x64), macOS 13+ (Apple Silicon and Intel) and Linux
  (Ubuntu 24.04+, x86_64 and aarch64). It becomes the next "Latest".
- **Files to attach:** Windows `capturemann-0.3.1-windows-x64-setup.exe`, `...-windows-x64.zip`, `.sha256` files and `SHA256SUMS.txt`;
  macOS `CaptureMann-v0.3.1-macos-arm64.dmg` and `CaptureMann-v0.3.1-macos-x86_64.dmg` with `.sha256` (rename the
  notarized `make release` output as in the 0.2.9 text); Linux `capturemann-0.3.1-linux-{x86_64,aarch64}.tar.gz`,
  `capturemann_0.3.1_{amd64,arm64}.deb`, `.sha256`, `.minisig`. APT repository: release `apt` (assets), run
  `make-apt-repo.sh` again.

---

## Release body

# CaptureMann 0.3.1: export sizes, MP3 import, transcript progress

0.3.1 has files for **Windows 11, macOS and Linux**. Main changes:

## New

- **Export: choose size and format and see the file size first.** The Export button opens a popover: MP4, WebM or GIF; the
  recording's own size or 1440p, 1080p, 720p, 480p (never above the recording); and the size before you press Export: a best
  guess ("About 14 MB") with the ceiling as detail ("up to 69 MB"). GIF has no estimate (the real size shows after) and is for
  clips of 30 s or less. `capturemann export <id> --format webm --size 720p [--estimate]` and the MCP tools `export_video` and
  `estimate_export` do the same. On Linux without a hardware H.264 encoder MP4 is VP9 and the popover says so.
- **Import MP3.** An `.mp3` file imports as an audio capture (app, CLI, MCP): decoded once to the standard WAV master, the
  original kept beside it, the title from the ID3 tag or the file name. The Import sheet shows the full (decoded) size first.
  Bare AAC is still refused.
- **Transcript progress on every capture.** A recording's page, the audio page and the library card show the same progress
  (percent, stage, Cancel, errors with Retry) as an imported file.

## Changed

- **Transcripts use the original microphone by default.** The noise-reduced copy can drop quiet or distant speakers. Settings
  (AI page) still lets you choose the noise-reduced copy; a value you saved before is kept; finished transcripts are not changed.

## Fixed

- The app no longer quits during a transcription when the speech engine meets a decode window whose text ends within its first
  160 ms (found on a 73-minute imported MP3).
- Cancelling an export no longer waits on a pipe held by another process (a Windows CI flake); wind-down waits are bounded.
- <FILL: fixes from the regression of the night, from `specs/plans/release-0.3.1-morning-report.md`>

## Checked, and what was not

Reports in `verification/` (all PARTIAL; one computer each): <FILL: file names of the final-files runs: `L1-windows-0.3.1-*`,
`L1-macos-0.3.1-*`, `L1-linux-0.3.1-*`>. Not run: **Intel Mac hardware**, Windows 10, SmartScreen, mixed DPI, NVIDIA/AMD
encoders, KDE and other Linux distributions, a second monitor.

## Known limits

- **The typical export size is a calibrated guess.** Calibrated on one Mac and one user's screens (10 recordings, 36 exports;
  86 % of held-out samples within a factor 1.5, never above the ceiling). The factor for imported files (3.8) rests on two
  imported H.264 files. **Windows and Linux encoders, 60 fps, 4K and Intel are not calibrated**; the ceiling is stated for the
  macOS hardware encoder and <FILL: result of `estimate_vs_real` on Windows/Linux, or "not run there">.
- **On Linux GPUs that take no bit rate (VA-API constant quality, for example Intel UHD 620 with iHD) the MP4 "Up to" figure is a
  scaled worst case (video counted at 4 times the usual rate) and there is no "About" figure.** The first run had 3 of 18 samples
  over the old bound, all high-motion MP4 (worst 3.6 times); the second run (18 of 18 within the bound, worst MP4 at 0.91 of it) is
  `verification/L1-linux-0.3.1-estimate-vs-real-x11-uhd620-vaapi-run2-2026-11-10-claude.md`. A noisier picture than the synthetic test could still pass 4 times; the ceiling is not a guarantee there.
- An MP3's length is an estimate in the file; imports allow a few frames of difference between the stated and decoded length.
- <FILL: failures listed in the morning report that were not fixed>

## Install and checks

<FILL: as in the 0.3.0 and 0.2.9 texts: winget, Homebrew cask, APT lines and key fingerprint, checksums, minisign key>
