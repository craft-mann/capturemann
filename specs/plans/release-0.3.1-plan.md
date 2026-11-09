# Release 0.3.1 plan

Status: **reviewed Nov 9, 2026, decisions applied**. Nothing built. Platforms: Windows, macOS and Linux
(0.3.1 is the first release since 0.2.9 that covers all three, so it becomes the next "Latest" on GitHub).

The earlier backlog of this file (27 items, Nov 8 to 9, 2026) was cleared at the maintainer's request; it is in git history
(`git show 2ef3e02a:specs/plans/release-0.3.1-plan.md`). Items from it are brought back only when listed below.

Rules: same as the 0.3.0 plan (general techniques only, no one-recording fixes; nothing is called "verified" without a stored
report in `verification/`; no GPL/AGPL in shipped binaries; `target_os` only in platform code; every user-facing action has an
MCP tool and a CLI command; UI text through `cm_i18n` with a `vi.po` entry; one change per commit with the trailer lines;
`cm-regress` per platform before release; no sound-playing tests after 22:00 Vietnam time; the maintainer pushes, signs, tags and
publishes).

**Build order** (decided in the review of Nov 9, 2026): §2 MP3 first, because it changes the FFmpeg build on all three systems;
run `cm-regress` standard on each right after it lands. Then §1, §4 and §3 (no new FFmpeg), then the freeze, then §5. Do the
"to check first" steps of §1 and §4 before scheduling those items.

**Freeze and release gate** (decided Nov 9, 2026): the freeze starts when §1 to §4 are merged and CI is green on all three
systems; after it only fixes go in. 0.3.1 becomes the latest release on Windows, macOS and Linux, so it is **not published**
until CI is green on all three and the regression (§5) has passed on all three, each report stored in `verification/`.

## 1. The transcript uses the original microphone by default

Today a transcript is made from the noise-reduced microphone copy unless the setting `ai.transcribe_raw_mic` is on
(`crates/cm-daemon/src/ai/actions.rs`, `Transcribe` request field and the setting). Change the default to the **original**
(raw) microphone track.

- Acceptance: a new recording or import with both tracks is transcribed from the raw mic unless the user (or the request)
  asks for the noise-reduced copy; Settings shows the choice in plain words; `set_settings`, the CLI and MCP name it the
  same way. **No migration (decided Nov 9, 2026):** the new default applies to every transcription that starts after the update
  (a new recording or import, or a re-transcribe), because the setting is read when the job starts; a library with no stored
  `ai.transcribe_raw_mic` uses the original microphone, a stored value (on or off) is kept, and transcripts already made are
  not changed. The settings store writes only the keys it is given (`set_settings`), so an explicit "off" stays on disk; one
  test covers both cases (no key: raw mic; key `false`: noise-reduced copy). Also update the texts that say "off by default":
  the MCP `transcribe` description (`cm-mcp/src/tools.rs`), the CLI help (`cm-cli/src/args.rs`), the comments in
  `cm-daemon/src/ai/{actions,transcripts}.rs` and `apps/desktop/src/ai/client.rs`, and the test in `cm-daemon/tests/ai_ops.rs`.
- **No benchmark gate (decided Nov 9, 2026):** the maintainer already tested the raw mic as the default; the change is only
  the default. The reason the noise-reduced copy was the default is in the 0.2.7 transcription-quality plan; the MCP text
  still explains the trade-off of each source.
- The system-audio track is unchanged.

## 2. Import MP3 (audio)

Plan: [`mp3-import-plan.md`](mp3-import-plan.md) (the bundled FFmpeg has no MP3 demuxer or decoder; `mp3float` is LGPL, no
encoder is added). **Decided (maintainer, Nov 9, 2026):** MP3 only, bare AAC stays refused; the capture title comes from the ID3
title when there is one, else the file name; the MP3 is converted once to the standard WAV master and the Import sheet shows the
size first.

- Acceptance: an `.mp3` file imports as an audio capture on Windows, macOS and Linux through the app, the CLI and MCP
  (`import`), with the transcript, the audio editor and Export audio working on it; a clear error for a broken file; the
  FFmpeg build lists only the added components; `make licenses` and `cargo deny` stay clean; the FFmpeg change is tested on
  every export and recording path (`cm-regress` standard on all three systems), because a new FFmpeg build touches all of them.
- Linux: the FFmpeg build is `packaging/ffmpeg/build-linux.sh`; Windows and macOS have their own build scripts.

## 3. Export a recording: choose resolution and format, see the file size first

Design: `docs/development/design-surfaces.md` section 5.13 (boards E0 to E4, `design/redesign/export-size-format-design.md`).
Today the app has no export dialog: Export starts one export at the recording's own size (`apps/desktop/src/ui/export_ui.rs`, spec
`specs/features/08-export.md`, EXP-1 to EXP-3).

**What the user gets**
- A popover on the recording page's Export button (a centred dialog under 900 px). The button always opens it on the defaults
  (MP4, recording size); **no split button and no remembered choices** (decided Nov 9, 2026, to keep it simple; boards E1 and
  E2 draw a split button, the build does not). `⌘E` / Ctrl+E opens the popover; while a file exports the button shows the
  progress and still opens the popover, so a second export can be queued.
- **Resolution:** the recording's size plus fixed steps (1440p, 1080p, 720p, 480p), never above the source, no custom sizes.
  **Format:** MP4, WebM and GIF (audio export stays in the audio editor).
- An **estimated file size** before pressing Export, updating as the choice changes, shown as a range ("About 200–290 MB") and
  labelled an estimate; after the export the real size is shown. On Linux without a hardware H.264 encoder the MP4 choice is
  VP9 and the popover says so.
- **No presets in the popover and none added in 0.3.1** (decided Nov 9, 2026); the engine's existing presets stay available
  to the CLI and MCP unchanged.

**Estimate**
- A formula only (duration after cuts, resolution, a per-codec per-size bit-rate table, audio included), no probe. Range =
  estimate −15 % to +20 %, 2 significant figures.
- **Pass line (decided Nov 9, 2026): the shown range contains the real size on every sample.** One miss fails the test. A stored
  test compares estimate and real size on three recordings for each format that has an estimate (MP4, WebM); the three
  recordings must span a mostly static screen, a typical screen recording and a high-motion one, so the table is shown to hold
  across content. If it fails, fix the rule or bring the probe back as a new decision; the label is not the fix.

**GIF (decided Nov 9, 2026)**
- GIF stays in the chooser (the engine, CLI and MCP options already exist; the dialog gets it now) and has **no estimate**: the
  popover says "Size known after export", then shows the real size. `--estimate` for GIF answers that there is none. A GIF
  estimate later is a new decision.
- Its own limits: width 480, 640, 800 or 1024 (listed instead of the resolution steps, never above the source); at most 30 s; the
  existing "fit under N MB" option (`--max-mb`).
- A clip over 30 s is refused with the design's text, "GIF is for clips of 30 s or less. This one is 3:12.", and the hint to set In
  and Out or choose MP4 or WebM; the EXP-3 edge case in `08-export.md` takes this text in place of "Use MP4 or WebM for clips
  longer than 30 s".

**CLI and MCP (identical behaviour)**
- CLI: `capturemann export <id> --format webm --size 720p`, plus `--estimate`, which prints the size without exporting.
- MCP: `export_video` takes `format`, `size`, `width` (GIF only) and `max_mb`, and its result carries the estimate; a new Free
  read-only tool `estimate_export` takes the same fields (added to the tool table in `specs/02-agent-interface.md`).
- Rule for `--size` and the existing `--width`: `--size` sets the frame for MP4 and WebM, `--width` stays GIF-only, and
  `--width` with MP4 or WebM is `invalid_input`.
- The refusals have one text in the app, CLI and MCP (`gif_too_long`, `size_larger_than_source`, `gif_cannot_fit`); on Linux
  without H.264 the estimate reports `codec: "vp9"`.

**Spec, tests and checks**
- Add new acceptance criteria (new IDs; `EXP-4` is taken; nothing renumbered) to `specs/features/08-export.md` for the choice,
  the estimate and the CLI/MCP parameters, and cite them in the tests.
- A manual script in `tests/manual/` for the popover (`⌘E` / Ctrl+E, the narrow-window dialog, queueing a second
  export, the four size looks, Vietnamese text).
- A `cm-regress` case each for `export --estimate` and `estimate_export`.
- Do the §5 export-cancel investigation before the queueing work: both touch the export job's cancel path.

## 4. Transcript progress for a recording, like an imported file

An imported recording or audio file shows the transcription's progress; a recording or audio capture made in the app does
not show it the same way (`apps/desktop/src/ui/recording/transcript_card.rs` versus the progress of
`ui/import_cards.rs` and `ui/ai.rs`).

- Acceptance: while a transcript of a recording or an audio capture is being made, the same progress (percent, the stage,
  cancel) appears as for an import, in the recording's page, the audio page and the library card, on all three systems;
  it survives closing and reopening the page, shows an error with a retry, and is the same job (`transcribe`, progress
  events on the bus) so the CLI and MCP report it too.
- A manual script in `tests/manual/` for the progress, the reopened page, the error and the retry.
- To check first: whether the two screens already read the same job events and one of them does not subscribe, or the
  recording path starts the job another way.

## 5. Regression, bug fixes and stabilization

The release ends with a complete regression and fixes of what it finds. No new feature goes in after the freeze.

- `cm-regress` full and hardware tiers on **Windows 11** (the lab laptop), **macOS** (Apple Silicon; Intel if a machine is
  available) and **Linux** (X11 and Wayland, `.deb` and tarball, the aarch64 VM), run on the **final release files**, each
  report stored in `verification/` and marked PARTIAL when it covers one computer.
- Intel Mac: if no machine is available, the macOS report is PARTIAL and the release text lists "Intel Mac hardware not run"
  as a known limit (as 0.2.9 did); it does not block the release (decided Nov 9, 2026).
- Fix every failure or list it in the release text with its reason. Known:
  - The Windows CI flake `exp4_ac4_cancel_deletes_partial_output` (cancel noticed after about 0.4 s, export wound down after
    about 31 s against a 20 s limit; failed on two of three Windows runs of Nov 8 to 9). **Required (decided Nov 9, 2026): it must
    not fail on GitHub Actions.** Find the cause of the slow wind-down first (a control run on the runner, not a raised limit),
    fix the export's cancel path, and show the test passing on at least five consecutive Windows runs before the tag; a limit
    change is allowed only if the cause is shown to be the runner and not the product.
  - The two Mac `cm-desktop` render tests that fail only inside the repository's `target/` (`recedit::render`).
  - The rebuilt 0.3.0 files had no separate `cm-regress` run.
- The hardware tier records the screen, camera and microphone and plays sound: run it only when the maintainer asks, and not
  after 22:00 Vietnam time for anything that plays sound.
- CI green on all three systems and the regression passed on all three before the tag or the publish (the release gate above); GitHub's Linux package workflow (never run) run once and fixed.
- The APT repository: an amd64 install, upgrade and removal from it tested. **Decided (maintainer, Nov 9, 2026): the host stays
  on GitHub release assets** (few Linux users yet; no move in 0.3.1). GitHub answers a first download slowly and sometimes with a
  504, which the install lines work around with retries; a later move to S3 + CloudFront at `apt.<domain>` would change every
  user's `.sources` file. The repository key expires Nov 8, 2031.
- Version bump, CHANGELOG, release text, checksums, signatures, tag: as in `docs/development/release-day-0.3.0.md`.

## Not in 0.3.1 unless the maintainer adds it

The cleared backlog (Windows Reveal button, output-device chooser, audio disk use, Windows 10 and other hardware runs, winget,
and the rest) stays out. Bugs found by the regression in section 5 are in.
