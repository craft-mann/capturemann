# VOC-2 report: audio editor and Export audio

**As reported.** The maintainer ran `tests/manual/VOC-2.md` on the 0.2.0 dev bundle and reported on
Oct 27, 2026 that the checks **pass**, including the final build with the report view, the Speakers tab and
the export. The result was reported for the script as a whole: no per-step notes, loudness numbers or
screenshots were stored. Every PASS below is the maintainer's statement.

- **Date:** 2026-10-27
- **Who:** the maintainer
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26 (not re-read for this run)
- **Build:** git `6769383` (branch `voice`), release profile, bundle `CaptureMann-dev.app`, version 0.2.0
- **FFmpeg:** the bundled LGPL build (`make ffmpeg`, Oct 27, 2026) with the `wav` muxer and `pcm_s24le`
- **Fixture:** none; real audio recordings and the test meeting copy

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 to 8 The page, now speaking, lanes, seeking, keys, cuts, suggested cuts, overview | PASS (as reported) | |
| 9 to 11 Transcript tab, Translate, footer exports | PASS (as reported) | |
| 12 to 13 Export audio sheet and the M4A / WAV export, loudness | PASS (as reported) | the measured loudness was not stored |
| 14 Export audio only on a screen recording | PASS (as reported) | |
| 15 Screen recordings' tabs | PASS (as reported) | |
| 16 Narrow and light | PASS (as reported) | |
| 17 Vietnamese | PASS (as reported) | the maintainer also reviewed the Vietnamese strings |

## Acceptance criteria

VOC-2 acceptance criteria: PASS as reported, except AC5 (60 minutes of audio in 60 s), which was not
measured.

## Problems found

Reported and fixed on the same day, before the final build: playing from a position reset to 0:00 (a fault of a
hand-made test copy, not of the app), and the report view's text was squeezed by its chips (fixed).

## Verdict

PASS (as reported), AC5 not measured.
