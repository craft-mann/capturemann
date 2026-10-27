# VOC-1 report: audio-only recording

**As reported.** The maintainer ran `tests/manual/VOC-1.md` on the 0.2.0 dev bundle (`CaptureMann-dev.app`)
and reported on Oct 27, 2026 that the checks **pass**. The result was reported for the script as a whole:
no per-step notes, no numbers (memory readings, drift, recovered length) and no screenshots were
stored. Nothing here is measured by the agent that wrote this file; every PASS below is the maintainer's
statement.

- **Date:** 2026-10-27
- **Who:** the maintainer
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26 (as in the other reports; not re-read for this run)
- **Build:** git `6769383` (branch `voice`), release profile, `make bundle-dev` bundle `CaptureMann-dev.app`
  (bundle id `dev.capturemann.app.dev`), version 0.2.0
- **FFmpeg:** the bundled LGPL build (`make ffmpeg`, Oct 27, 2026)
- **Fixture:** none; real recordings made in the app

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Entry points (tray, launcher, sidebar mic button, shortcuts) | PASS (as reported) | |
| 2 The sheet, Audio mode | PASS (as reported) | |
| 3 Consent line (AC7) | PASS (as reported) | |
| 4 Start, HUD and meter (AC1, AC3) | PASS (as reported) | |
| 5 Option-Shift-V and Option-Shift-M (AC3, AC4) | PASS (as reported) | |
| 6 Online meeting, system audio (AC2) | PASS (as reported) | the 10-minute sync figure was not stored |
| 7 Permissions (microphone) | PASS (as reported) | |
| 8 Lid and long recordings | PASS (as reported) | |
| 9 Agent started recording (AC8) | PASS (as reported) | |
| 10 Killed process, recovery (AC5) | PASS (as reported) | the recovered length was not stored |
| 11 Three-hour soak (AC6) | PASS (as reported) | memory and drift readings were not stored |

## Acceptance criteria

VOC-1.AC1 to AC9: PASS as reported (no stored measurements for AC2 call sync, AC5 recovered length, AC6
memory and drift).

## Problems found

None reported.

## Verdict

PASS (as reported). Because the numbers were not stored, a status that needs them (for example "under
200 MB for 3 hours") should say "as reported".
