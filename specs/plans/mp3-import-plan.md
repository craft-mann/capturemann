# Plan: import MP3 audio (target 0.3.1)

Status: **draft** (Nov 8, 2026), nothing built; **scheduled for 0.3.1** (maintainer, Nov 9, 2026, see release-0.3.1-plan.md section 2). Asked for by the maintainer on Nov 8, 2026 ("we can't import mp3 audio"). Not for
0.2.9 (a stabilization release; a new FFmpeg build means every export and recording path is tested again). Place it in 0.3.0
(Linux features) or in a 0.2.10 if the maintainer wants it sooner; decision below.

Rules that apply: general techniques only; **nothing is called "verified" without a stored report in `verification/`**; no
GPL/AGPL in shipped binaries (the MP3 decoder in FFmpeg, `mp3float`, is LGPL; no encoder is added); `target_os` only in platform
code; every user-facing action has an MCP tool and a CLI command (import already has both); UI text through `cm_i18n`; one
change per commit with the two trailer lines; the maintainer pushes, tags and publishes.

## What is wrong today

`specs/features/18-import.md` (IMP-1): the bundled FFmpeg is built with only the components the product uses, and it has no
demuxer or decoder for MP3. `ffprobe` cannot open an MP3, so `probe.rs` identifies it from its first bytes (`ID3` tag or an
MPEG audio frame sync, `sniff_container` returns `"mp3"`) and the file is classified **Can't import: `unsupported_format`**
(IMP-1.AC2). Nothing else in the import path is MP3-specific: an imported audio file is decoded to `mic.wav` (48 kHz, 32-bit
float, `prepare.rs` `decode_sound`) and becomes an `audio` bundle (VOC-1), so the editor, transcription and Export audio work
on it as on any other audio capture.

Not read yet, to check in task 1: `packaging/ffmpeg/build-windows.sh` and `build-linux.sh` are assumed to need the same three
components as `build-macos.sh` (their lists are the same shape); the macOS `Info.plist` and the Windows `installer.nsi`
associations list `m4a` and `wav` only.

## Scope

In: import of **MP3** (MPEG-1/2 Layer III, constant or variable bit rate, mono or stereo, 8 to 48 kHz) from the Import sheet,
drag and drop, Open With, the CLI (`capturemann import`) and MCP (`import_media`). **Bare AAC (`.aac`, ADTS) stays out** (maintainer,
Nov 9, 2026): it is still refused with the existing message; the `.aac` extension is not added to the accepted lists.

Out: bare AAC (above), exporting MP3 (needs an encoder, libmp3lame, a separate decision; `15-voice.md` lists it as out of scope), FLAC, OGG,
Opus, AIFF and WebM audio (each needs its own demuxer and decoder; see "Later"), more than one audio stream, ID3 chapters.

## Tasks

1. **FFmpeg builds (the long part).** In `packaging/ffmpeg/build-macos.sh`, `build-windows.sh` and `build-linux.sh` add to the
   component lists: demuxers `mp3` and `aac`; decoder `mp3float`; parser `mpegaudio` (and `aac_latm`/`aac` stays); and to the
   `REQUIRED_*` self-check lists the same. Rebuild FFmpeg for macOS arm64 and x86_64, Windows x64 and Linux x86_64 and aarch64
   (`make ffmpeg`, the Windows and Linux scripts); each self-check must still say LGPL v2.1 with no GPL, nonfree or libx264;
   update `BUILD-INFO.txt` and the licence notes if they list components; record the size change (expected a few hundred KB).
   Run the whole export, preview and regression set with the new binaries (the existing test run under
   `packaging/ffmpeg/with-bundled-ffmpeg.sh`), because FFmpeg is shared by every export.
2. **Probe and classify (`crates/cm-media/src/import/`).** `probe.rs`: `ffprobe` now opens MP3, so the container name `mp3`
   becomes a `Family::Mp3`; keep `sniff_container` for damaged files. `classify.rs`: add the family
   to the audio containers and `mp3` to the accepted audio codecs (`AUDIO_CODECS` comment says "AAC and PCM"); the status is
   **Prepared for editing** (the original is kept as the master, `mic.wav` is made from it) like other non-PCM audio, with a
   reason `container_mp3`, or **Ready** if the spec keeps M4A as the model: decide in task 2 by reading how M4A is classified
   (IMP-1.AC1 says M4A is Ready or Prepared). Unit tests in `classify.rs` for MP3 in each family and the error cases.
3. **Decode path (`prepare.rs`).** MP3 goes through `decode_sound` unchanged; check and test: encoder delay and padding
   (gapless: the first sample must be the first audible sample, `aresample=async=1:first_pts=0` plus the LAME header skip), VBR
   files without a length header (the duration used for progress and the manifest must come from the decoded frames, not the
   header estimate), cover art in an ID3 `APIC` frame (an attached picture, not video, already rule in IMP-1), sample rates 22.05
   and 24 kHz (MPEG-2) and 44.1 kHz (resampled to 48 kHz), mono and joint stereo, a file that is cut off in the middle (the
   decodable part imports with a warning, or `unreadable_file`: decide with the spec text for other formats).
4. **Title from the tag (optional, small).** The ID3 `title` as the default capture title, else the file name; tags are read by
   `ffprobe` (`format.tags.title`), never written. Never put tags that name people into logs.
5. **App and OS entry points.** `apps/desktop/src/import_flow.rs`: add `mp3` (and `aac` is already there) to `AUDIO_EXT` and
   `DOCUMENT_EXT` (the last one decides what Open With and a double click hand to the app). `packaging/macos/Info.plist`
   (`CFBundleDocumentTypes`, currently `m4a` and `wav`) and `packaging/windows/installer.nsi` (the `SecOpenWith` section, registry
   keys for `.m4a` and `.wav`) add `.mp3`. Linux: the `.desktop` file MIME types if a Linux package ships one (0.3.0).
6. **Words.** The Can't-import messages and the sheet's "supported formats" line (search `M4A, WAV` and `MP4, MOV, M4V` in
   `apps/desktop`, `crates/cm-media/src/import/mod.rs`, `crates/cm-cli/src/args.rs`, `crates/cm-mcp/src/tools.rs`,
   `specs/02-agent-interface.md`, `docs/development/design-surfaces.md`): add MP3 (and AAC), regenerate the `.pot`
   (`cargo run -p cm-i18n -- extract`), Vietnamese in `locales/vi.po`, run `-- check`.
7. **Specs.** `18-import.md`: IMP-1.AC2 loses MP3 (and bare AAC) from the unsupported list; new ACs: an MP3 (CBR and VBR, with and
   without ID3 and cover art) imports as an audio bundle whose `mic.wav` has the length of the file within 50 ms; a damaged MP3
   is `unreadable_file`; an MP3 renamed to `.wav` is imported by its content. Update "Formats supported" and "Out of scope"
   (the "0.2.8 candidate" line). Add the item to `docs/project/status.md`.
8. **Tests.** A tiny MP3 fixture (a few KB, 2 s of a tone, 44.1 kHz, with an ID3v2 tag and a 64x64 cover) **checked in** under
   `crates/cm-media/tests/fixtures/` so the tests do not need an encoder (the CI `ffmpeg` and the lab machines may not have
   libmp3lame); a VBR one and a mono 22.05 kHz one. Tests: probe (`dry_run`), classify, full import to a bundle, `mic.wav` length
   and first-sample check against the known tone, transcription path unchanged (no model needed: the existing fake). The
   regress case `import.audio` (`crates/cm-regress`) gets an MP3 input; its fixtures come from the same checked-in files.
9. **Manual and release checks.** `tests/manual/` import script: MP3 from the file picker, drag and drop, Open With (Windows
   Explorer, macOS Finder), a 1 hour MP3 (import time and memory; the audio is decoded to a 32-bit float WAV, about 700 MB per
   mono hour, so the disk use of an imported MP3 is large: say so in the release notes), then transcribe it. Stored reports in
   `verification/`; `cm-regress` smoke, standard and hardware on each platform's final package.

## Acceptance

- An MP3 file that plays in a normal player imports on macOS and Windows from the sheet, drag and drop and the CLI, as an audio
  capture with the right title, length (within 50 ms), channel count and no click at the start.
- Transcription, the audio editor and Export audio work on it.
- A damaged, empty or protected file is refused with the same codes as before; nothing is written on a refusal.
- Every shipped FFmpeg is still LGPL-only and its self-check passes; the full export regression passes on each platform.
- IMP-1.AC2 and the "supported formats" text agree with the code; the manual script has a stored report.

## Risks

- **FFmpeg rebuild.** Every export and the whole regression depend on it: a rebuild is a full re-test, which is why this is not in
  0.2.9. Pinned sources and checksums stay as they are; only the configure lists change.
- **Disk use.** The imported audio is kept as a float WAV; a long MP3 becomes a large file (an hour of mono is about 700 MB at 48 kHz). A smaller internal format is a later change.
- **Odd files.** Free-format MP3, MPEG-2.5 (8 kHz), files with garbage before the first frame, and ID3v1 only: test with a few
  real files from the maintainer (never commit private recordings).
- **Patents.** MP3 patents expired in 2017; the decoder is LGPL. No change to the licence text beyond the component list.

## Later (not in this plan)

FLAC, OGG/Vorbis, Opus, AIFF, WebM audio: the same three kinds of change per format (demuxer, decoder and parser in the three
build scripts; one `Family`; tests). MP3 export: the libmp3lame encoder (LGPL) plus a new format in Export audio.

## Decisions (maintainer, Nov 9, 2026)

- **Release:** 0.3.1, on all three systems (the FFmpeg rebuild is re-tested on every platform that ships it).
- **Bare AAC (`.aac`):** not included; MP3 only.
- **Title from the ID3 tag:** yes. The ID3 `title` is the default capture title, else the file name; the tag text is cut to a
  sensible length and stripped of control characters, and is never used as a path.
- **Disk use:** the imported MP3 is converted once to the standard float WAV master; the Import sheet shows the size it will
  take before the user confirms, and the release notes say so. A smaller stored format is not part of this plan.
