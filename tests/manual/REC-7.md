# REC-7 manual script: cursor telemetry (`cm-cursor`)

Covers REC-7 AC1 to AC4 where automation needs real input hardware. The format and the
coordinate convention are documented in `crates/cm-cursor/src/schema.rs`.

## Status of the author's runs

| Item | macOS | Windows |
| --- | --- | --- |
| Library, writer, reader, pause, permission fallback (fake source, CI) | automated, passing | compile-checked only (`cargo check --target x86_64-pc-windows-msvc`) |
| Real event tap, images, polling fallback | run (see below) | NOT RUN: the hook and `GetCursorInfo` code has never executed on Windows |
| Injected timestamp accuracy and click completeness (AC1/AC2) | NOT RUN (needs `CM_ALLOW_INPUT_INJECTION=1`, Accessibility, Input Monitoring) | NOT RUN |

## Setup (macOS)
- Grant **Input Monitoring** to the terminal that runs `cargo` (System Settings > Privacy &
  Security > Input Monitoring) and restart the terminal. Without it the denied path is exercised
  (that is also a test, see check 3).
- For the injection test only: also grant **Accessibility** (synthetic events are dropped
  silently without it).

## Automated-but-ignored tests

```
cargo test -p cm-cursor -- --ignored --nocapture --test-threads=1
```

| Test | AC | Pass when |
| --- | --- | --- |
| `rec7_real_source_header_images_and_cpu` | AC1 (image part), NFR CPU | prints `status: ... EventTap`, at least one `Cursor` event, PNG sizes and hotspots plausible (Retina: scale 2), CPU a few percent while you move the mouse. Writes to a temp dir that is deleted at the end |
| `rec7_ac4_real_polling_fallback` | AC4 | header `clicks: unavailable`, `move_source: poll`, `permission_missing: input_monitoring`, moves recorded when you move the mouse during the 1.5 s |
| `rec7_ac1_ac2_injected_timestamp_accuracy_and_click_completeness` | AC1, AC2 | needs `CM_ALLOW_INPUT_INJECTION=1`. MOVES YOUR REAL CURSOR for about 4 s and posts 30 middle-button clicks near the top-right of the main display (middle button, so nothing is activated). Pass: down 30/30, up 30/30, median click timestamp error under 5 ms. Do not run it while working. Without the variable it prints SKIPPED and passes |

## Manual checks
1. AC1 on a 10-minute recording (needs the full recorder, M3 slice with screen capture): record
   with "Record cursor separately" off (cursor baked), draw the telemetry cursor over the video
   at the sampled frames, confirm each is within one frame and 2 physical px of the baked cursor.
   Repeat on a Retina and a 1x display, and with the cursor on a second display (`outside: true`).
2. AC2: click 20 times with the left button in a drawing app, then
   `grep -c '"type":"down"' cursor.jsonl` and `grep -c '"type":"up"' cursor.jsonl` both print 20.
3. AC4: revoke Input Monitoring for the app, record 10 s moving the mouse and clicking. The
   recording must succeed; `cursor.jsonl` has moves only; the HUD shows the
   `permission_missing(input_monitoring)` warning; `session.json` has `cursor.clicks: "unavailable"`.
4. AC3 privacy: type a distinctive string (for example `zq7xk2 lorem`) in any app while
   recording, then `grep -r zq7xk2 <bundle>` finds nothing. Hold Shift while clicking and confirm
   the `down` line has `"mods":["shift"]` and no other key information.
5. Crash safety: `kill -9` the recording process after 10 s; `CursorLog::read` still opens the
   file (a torn last line is reported as `truncated`) and at most about 1 s of events is lost.
6. Pause: pause for 5 s while moving the mouse, resume; no event has `t` inside the paused
   interval except the re-anchor `move`/`cursor` lines written exactly at the resume time.
7. Windows (not run by the author): repeat checks 2, 4 and 6 at 100% and 150% scaling with a
   per-monitor-v2 DPI aware process; confirm positions equal screen-track pixels and that the
   hook is not removed by Windows during a 10-minute recording.
