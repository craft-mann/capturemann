# PLAY-1 report

- **Date:** 2026-10-22
- **Who:** Claude (automated terminal run; no person watched the window or listened)
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26.6.2 (no display or audio device was used)
- **Build:** git `94d9a96` (main), debug build of `capturemann` and `cm-daemon` run through `cargo`, headless daemon on `/tmp/cm-m4.sock`
- **FFmpeg:** ffmpeg version 9.0.2 (Homebrew)
- **Fixture:** none opened (the script is about the editor window, the audio device and what a person sees and hears)

Words: PASS / FAIL = measured in this run. "needs-person" = nobody has done it (watching the window, listening, a real player). "not built" = the feature does not exist. Frame inspection below is Claude looking at extracted PNG frames, not a person watching playback.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Open | needs-person | the window was not launched |
| 2 Play (smooth, sound, flash and beep together) | needs-person | |
| 3 Pause skipped | needs-person | the pause is removed from the export duration: PASS in `rec4_pause_is_removed_from_picture_and_sound_and_they_stay_in_sync` (automated) |
| 4 End and loop | needs-person | |
| 5 Scrub paused | needs-person | |
| 6 Scrub while playing | needs-person | |
| 7 Frame step | needs-person | |
| 8 Edits while playing | needs-person | |
| 9 Mute and gain | needs-person (CLI side PASS) | `edit audio --mic-mute` and `--system-gain` are stored and applied by the export (see EDT-2); the preview picking them up is needs-person |
| 10 No audio device | needs-person | |
| 11 Long clip | needs-person | |

## What was run instead (automated, `cargo test --workspace --locked`, 1687 passed, 0 failed, 33 ignored)

`cm_media::decode` (continuous decoder and cache, FFmpeg fixtures), `recedit::pacer` (audio-clock pacing, loop, end), `cm_audio::playback` (channel and rate conversion), and the parity test (the preview's render path against the export, below). These do not replace watching or listening.

Parity gate, this run (1280x720, `cargo test -p cm-desktop --test parity -- --nocapture`, 3 tests passed in 15.9 s): mean error of 255 / PSNR / share of clearly different pixels per case: control 0.00-0.58 / 36-99 dB / 0-0.53 %; trim 0.54-0.58 / 36 / 0.4-0.6 %; cut+speed across the pause 0.53-0.96 / 35-37 / 0.5-0.7 % (shift -1 once at 2x); background (gradient, padding, radius, shadow) 0.25-0.93 / 32-45 / 0.1-1.1 %; bubble, nothing masked 0.50-0.96 / 34-36 / 0.55-0.74 %; everything 0.77-1.03 / 32-34 / 0.8-1.05 %.

## Acceptance criteria

PLAY-1 has no numbered ACs; every step is needs-person.

## Problems found

None observed (nothing was run on a person's side).
