# Release 0.3.1 plan

Status: **draft** (rewritten Nov 9, 2026 by the maintainer's list). Nothing built. Platforms: Windows, macOS and Linux
(0.3.1 is the first release since 0.2.9 that covers all three, so it becomes the next "Latest" on GitHub).

The earlier backlog of this file (27 items, Nov 8 to 9, 2026) was cleared at the maintainer's request; it is in git history
(`git show 2ef3e02a:specs/plans/release-0.3.1-plan.md`). Items from it are brought back only when listed below.

Rules: same as the 0.3.0 plan (general techniques only, no one-recording fixes; nothing is called "verified" without a stored
report in `verification/`; no GPL/AGPL in shipped binaries; `target_os` only in platform code; every user-facing action has an
MCP tool and a CLI command; UI text through `cm_i18n` with a `vi.po` entry; one change per commit with the trailer lines;
`cm-regress` per platform before release; the maintainer pushes, signs, tags and publishes).

## 1. The transcript uses the original microphone by default

Today a transcript is made from the noise-reduced microphone copy unless the setting `ai.transcribe_raw_mic` is on
(`crates/cm-daemon/src/ai/actions.rs`, `Transcribe` request field and the setting). Change the default to the **original**
(raw) microphone track.

- Acceptance: a new recording or import with both tracks is transcribed from the raw mic unless the user (or the request)
  asks for the noise-reduced copy; Settings shows the choice in plain words; `set_settings`, the CLI and MCP name it the
  same way; a library whose user already chose a value keeps that value (migration: only an unset setting changes).
- To check first: why the noise-reduced copy was the default (transcription-quality plan 0.2.7, `specs/plans/transcription-quality-plan-0.2.7.md`);
  run the transcript benchmark (`verification/benchmarks/`) on both sources before the change and store the numbers. The
  default moves only if the raw mic is not worse on the benchmark; if it is worse on noisy clips, say so in the Settings text.
- Both platforms of the system-audio track are unchanged.

## 2. Import MP3 (audio)

Plan: [`mp3-import-plan.md`](mp3-import-plan.md) (the bundled FFmpeg has no MP3 demuxer or decoder; `mp3float` is LGPL, no
encoder is added). Decisions still open there: bare AAC (recommended yes), the title from the ID3 tag, disk use.

- Acceptance: an `.mp3` file imports as an audio capture on Windows, macOS and Linux through the app, the CLI and MCP
  (`import`), with the transcript, the audio editor and Export audio working on it; a clear error for a broken file; the
  FFmpeg build lists only the added components; `make licenses` and `cargo deny` stay clean; the FFmpeg change is tested on
  every export and recording path (`cm-regress` standard on all three systems), because a new FFmpeg build touches all of them.
- Linux: the FFmpeg build is `packaging/ffmpeg/build-linux.sh`; Windows and macOS have their own build scripts.

## 3. Export a recording: choose resolution and format, see the file size first

Today the export dialog offers a preset or the recording's own size (`apps/desktop/src/ui/export_ui.rs`, spec
`specs/features/08-export.md`, EXP-1 to EXP-3).

- The user chooses the **resolution** (the source size and smaller steps, for example 2160p, 1440p, 1080p, 720p, 480p,
  never above the source) and the **format** (MP4, WebM, GIF, and audio-only where it applies), and sees an **estimated file
  size** before pressing Export, updating as the choice changes.
- The estimate comes from the duration, the chosen resolution and the encoder's bit-rate rule (hardware H.264 rate, the
  Linux VP9 fallback, GIF palette size, audio bit rate), shown as a range ("about 180 MB") and labelled an estimate; after
  the export the real size is shown. If the estimate is far off in the tests, the rule is wrong, not the label.
- Acceptance: the same choices exist in the CLI (`capturemann export <id> --resolution 1080p --format mp4`, plus a
  `--estimate` that prints the size without exporting) and in MCP (`export_video` parameters and an estimate tool or field);
  a stored test compares estimate and real size on three recordings per format; presets stay and fill the two fields.
- Open decisions: which resolutions and formats to offer (Windows and macOS hardware encoders have limits; Linux without a
  hardware encoder is VP9); whether the estimate may take a short probe of the recording (cheap, more accurate) or only a formula.

## 4. Transcript progress for a recording, like an imported file

An imported recording or audio file shows the transcription's progress; a recording or audio capture made in the app does
not show it the same way (`apps/desktop/src/ui/recording/transcript_card.rs` versus the progress of
`ui/import_cards.rs` and `ui/ai.rs`).

- Acceptance: while a transcript of a recording or an audio capture is being made, the same progress (percent, the stage,
  cancel) appears as for an import, in the recording's page, the audio page and the library card, on all three systems;
  it survives closing and reopening the page, shows an error with a retry, and is the same job (`transcribe`, progress
  events on the bus) so the CLI and MCP report it too.
- To check first: whether the two screens already read the same job events and one of them does not subscribe, or the
  recording path starts the job another way.

## 5. Regression, bug fixes and stabilization

The release ends with a complete regression and fixes of what it finds. No new feature goes in after the freeze.

- `cm-regress` full and hardware tiers on **Windows 11** (the lab laptop), **macOS** (Apple Silicon; Intel if a machine is
  available) and **Linux** (X11 and Wayland, `.deb` and tarball, the aarch64 VM), run on the **final release files**, each
  report stored in `verification/` and marked PARTIAL when it covers one computer.
- Fix every failure or list it in the release text with its reason. Known: the Windows CI flake
  `exp4_ac4_cancel_deletes_partial_output` (cancel noticed after about 0.4 s, export wound down after about 31 s against a 20 s
  limit; failed on two of three Windows runs of Nov 8 to 9); the two Mac `cm-desktop` render tests that fail only inside the
  repository's `target/` (`recedit::render`); the rebuilt 0.3.0 files had no separate `cm-regress` run.
- CI green on all three systems before the tag; GitHub's Linux package workflow (never run) run once and fixed.
- The APT repository: an amd64 install, upgrade and removal from it tested; the host (GitHub release assets for now)
  answers a first download slowly and sometimes with a 504, so decide whether 0.3.1 moves it (S3 + CloudFront at
  `apt.<domain>`; the users' `.sources` file would change). The repository key expires Nov 8, 2031.
- Version bump, CHANGELOG, release text, checksums, signatures, tag: as in `docs/development/release-day-0.3.0.md`.

## Not in 0.3.1 unless the maintainer adds it

The cleared backlog (Windows Reveal button, output-device chooser, audio disk use, Windows 10 and other hardware runs, winget,
and the rest) stays out. Bugs found by the regression in section 5 are in.
