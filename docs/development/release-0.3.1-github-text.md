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

- **Long transcripts show up while they are made.** A recording or audio file longer than 10 minutes is transcribed in
  5-minute pieces (as before); now the text of the finished pieces is readable at once on the recording page and in the audio
  editor ("Transcribing: the first 25 minutes are ready. The rest follows."), read-only, while the rest is made. Cancelling removes
  it; a held job (a recording started) keeps it. `capturemann ai transcript` and the MCP `get_transcript` return it marked
  `partial: true` with `covered_secs` and `total_secs`. Captions, translation, speakers and reports wait for the final transcript.

- **A long recording opens at once.** The editor window shows first and the sound is prepared behind it ("Preparing the sound... 40 %");
  Play pressed early starts by itself when it is ready. A 4 hour audio file used to hold the window at "Loading..." for 21 s on the
  test Mac (macOS, as reported by the maintainer; the other systems share the code but were not timed).
- **A temporary guard against sentences nobody said, for Vietnamese transcripts.** Whisper sometimes writes a sign-off ("subscribe to
  the channel", "thanks for watching") over a stretch with no speech. New Vietnamese transcripts leave out a line when the audio under
  it is no louder than the room and the line is long or every word is certain (switch "Leave out lines the audio does not hold" on the
  AI page, on by default) and you can add your own "Lines to leave out". Every dropped line is kept with its time range in the
  transcript. **It runs on Vietnamese only for now:** on one English call it dropped a sentence a person had said softly, so other
  languages keep every line. It does not catch every invented line: one heard over real sound (other voices, music) is not dropped, and your own list "Lines to leave out" is for those. Neither changes a finished transcript; re-generate to apply. Three drops of a 4 hour Vietnamese file were listened to by the
  maintainer and were all noise; the other nine were not.

## Changed

- **Transcripts use the original microphone by default.** The noise-reduced copy can drop quiet or distant speakers. Settings
  (AI page) still lets you choose the noise-reduced copy; a value you saved before is kept; finished transcripts are not changed.

## Fixed

- "Find speakers" on a very long recording (4 h 13 min, automatic speaker count) could quit the app while the speech engine's runtime
  shut down between two windows; the runtime is now kept for the life of the app. It did not recur in one run on the test Mac.
- The app no longer quits during a transcription when the speech engine meets a decode window whose text ends within its first
  160 ms (found on a 73-minute imported MP3).
- Cancelling an export no longer waits on a pipe held by another process (a Windows CI flake); wind-down waits are bounded.
- A good MP3 was refused as unreadable on Ubuntu 24.04's FFmpeg 6.1 when its stated length ran a few frames past the decoded
  length (variable bit rate files); the check now allows three MP3 frames.
- A cancelled edited export could leave its `.part` and `.mix.part` files behind on Windows (a just-killed process still held
  them); deleting them is retried for up to 2 seconds.
- Two macOS render tests hung in Metal device discovery (a test setup problem, not a render bug).
- The MP4 "Up to" size was not a ceiling on Linux GPUs with constant-quality H.264 only (see Known limits).

## Checked, and what was not

Reports in `verification/` (all PARTIAL; one computer each), run on the release files built from `5e1a0d9d` (the code of the release):
`L1-windows-0.3.1-regression-standard-tag-package-*`, `-full-tag-package-*`, `-full-tag-package-ai-*` (the files built from the tag, standard 51, full 58, transcription, speakers and silent installer 3 of 3; earlier runs on the CI package of `5e1a0d9d`: `-standard-final-*`, `-full-final-*`, `-full-final-ai-*`); `L1-macos-0.3.1-regression-standard-final-dmg-arm64-*`,
`-full-final-dmg-arm64-*` (and `-run2`, `-run3`), `-hardware-final-hw-nosound-*`, `-hardware-final-hw-sound-*`;
`L1-linux-0.3.1-regression-standard-x11-final-tarball-*` (and `-run2`), `-full-x11-final-tarball-full-*`, `-full-x11-final-deb-install-*`,
`L1-linux-0.3.1-aarch64-vm-final-*`. 0 failed in the runs listed here, except that two runs each had a failure that was found, explained and
fixed in the test or a later rerun, and the report is kept: macOS `full` (cm-overlay still exiting at the end of two cases; reruns 45 of 45
twice) and Linux `standard` (45 of 46: a test that expected a typical export size on a GPU that gives none by design; 46 of 46 after the test
was fixed). Counts: macOS `standard` 43, `full` 45, `hardware` 54 and the sound case 1; Windows 11 (ThinkPad T490) `standard` 44, `full` 58,
plus transcription, speakers and the silent installer 3 of 3; Linux x86_64 (same laptop, X11) `standard` 46, `full` 59, `.deb` install 1;
Linux aarch64 VM `full` 37. The export-size test: macOS 18 of 18 samples within the ceiling, Linux UHD 620 18 of 18 after the fix.
Not run: **Intel Mac hardware**, Windows 10, SmartScreen, mixed DPI, NVIDIA/AMD
encoders, KDE and other Linux distributions, a second monitor.

## Known limits

- **The typical export size is a calibrated guess.** Calibrated on one Mac and one user's screens (10 recordings, 36 exports;
  86 % of held-out samples within a factor 1.5, never above the ceiling). The factor for imported files (3.8) rests on two
  imported H.264 files. **Windows and Linux encoders, 60 fps, 4K and Intel are not calibrated**; the ceiling is stated for the
  macOS hardware encoder and the Linux UHD 620 laptop; `estimate_vs_real` was **not run on Windows**.
- **On Linux GPUs that take no bit rate (VA-API constant quality, for example Intel UHD 620 with iHD) the MP4 "Up to" figure is a
  scaled worst case (video counted at 4 times the usual rate) and there is no "About" figure.** The first run had 3 of 18 samples
  over the old bound, all high-motion MP4 (worst 3.6 times); the second run (18 of 18 within the bound, worst MP4 at 0.91 of it) is
  `verification/L1-linux-0.3.1-estimate-vs-real-x11-uhd620-vaapi-run2-2026-11-10-claude.md`. A noisier picture than the synthetic test could still pass 4 times; the ceiling is not a guarantee there.
- An MP3's length is an estimate in the file; imports allow a few frames of difference between the stated and decoded length.
- A rare failure of one test of the daemon's local socket (`r2_7_stale_socket_is_replaced_live_one_is_not`) was seen once in CI
  and did not repeat in more than 200 runs; the cause is not known.
- Not run in this release: the Linux `.deb` install on a clean computer, the Linux Wayland session, transcription and speakers
  on Linux, long recordings on Windows and Linux by hand (a 2 h 07 min and a 4 h 13 min file were transcribed on one Mac only: 5.1 and 3.0 times real time with large-v3, 4.3 GB of memory for the transcript alone and 11.7 GB with the speaker job at the same time; `verification/benchmarks/`). The peak with the speaker job at the same time was 11.7 GB on a 48 GB Mac; **it was not tried on a 16 GB Mac** (it may swap), so start speakers after the transcript on a small Mac.

## Install and checks

| File | SHA-256 |
|---|---|
| `capturemann-0.3.1-windows-x64-setup.exe` | `d9ffd95665117c1c635e749df27c2f8dbc36d11844aa716abdfe738bf947cc78` |
| `capturemann-0.3.1-windows-x64.zip` | `44c3584bb23bb5c13f10f6911c36e35b0d3fa79a119123b31d2a7214e677d098` |
| `CaptureMann-0.3.1-arm64.dmg` | `fe9974c41dc206b9e05b14b87ae507ef5f935f8baf0265ae37fd205438a32342` |
| `CaptureMann-0.3.1-x86_64.dmg` | `e360ae60f939f37e51ffe19b9c812d693822a0d8619b1d11228703c5f36a3db5` |
| `capturemann-0.3.1-linux-x86_64.tar.gz` | `0c6b77c20b6eb047cd615bbf72ba863445713b75ce0df232e080d635c023a3e1` |
| `capturemann_0.3.1_amd64.deb` | `b9f8195a79e0fdf4b45df65c4cc4bc3d990db0459d272f7bad9ee1282a1b7f8d` |
| `capturemann-0.3.1-linux-aarch64.tar.gz` | `d5875e59b44b519ee1ecde7c3d749a5efb6e8962e9235610d749e34055eedf3d` |
| `capturemann_0.3.1_arm64.deb` | `5920160c2695794a6ae8be81eebcca0cd3452d5f108058df9b83653c8c813cba` |

Minisign public key: `packaging/linux/minisign.pub`; public key `RWTeQv8ogB2CCuZ7j0rr3MovdHod4QATmVn+I329YC8pomOC6R9Rxc2k` (verify: `minisign -Vm <file> -P <that key>`).

winget: pull request 472327 (0.2.8) was still open on Nov 10, 2026, so **0.3.1 is not in winget**; use the wording of `release-0.2.9-winget-note.md` with 0.3.1 as the newer version.

<FILL after upload: the Homebrew cask (tap commit) status, and a check of the table above against the uploaded files (the macOS DMGs were built locally by the maintainer, the Windows files by run 42942883045 on the tag).>

### Install with APT (Ubuntu 24.04 or newer, amd64 and arm64)

```sh
U=https://github.com/craft-mann/capturemann/releases/download/apt
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann-archive-keyring.gpg -o /usr/share/keyrings/capturemann-archive-keyring.gpg
sudo curl -fsSL --retry 5 --retry-all-errors $U/capturemann.sources -o /etc/apt/sources.list.d/capturemann.sources
sudo apt -o Acquire::Retries=5 update && sudo apt -o Acquire::Retries=5 install capturemann
```

GitHub's release host sometimes answers the first download of a file with a `504`; repeating the command works (the `Retries` option does it for you). The repository is hosted on GitHub release assets for now.

Repository key fingerprint: `516C CD4F C7B3 5167 8803  F71B DEB1 BB54 FAEF 557D` (RSA 4096, signing only, expires Nov 8, 2031). The repository is the set of files of the release tagged [`apt`](https://github.com/craft-mann/capturemann/releases/tag/apt). The key and source files must be the ones from that release; check the fingerprint with `gpg --show-keys /usr/share/keyrings/capturemann-archive-keyring.gpg`.

Install a file: Windows `capturemann-0.3.1-windows-x64-setup.exe` (or the zip), macOS the DMG of your chip, Linux `sudo apt install ./capturemann_0.3.1_amd64.deb` (or the tarball and `install.sh`).
