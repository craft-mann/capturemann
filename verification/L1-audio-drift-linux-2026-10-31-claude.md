# L1 report: the 21 ms resync and -2229 ppm in the Linux system-audio track

- **Date:** 2026-10-31
- **Who:** Claude (run for the maintainer; nobody listened; no real sound device)
- **Machine:** the QEMU VM of the other L1 reports (aarch64, Ubuntu 24.04.5, 4 vCPUs shared with other jobs, PipeWire 1.0.5 with WirePlumber, headless GNOME session). The session's PipeWire was used; the test made its own null sink `cm-probe-sink` (`pw-cli create-node adapter` with `support.null-audio-sink`, `object.linger=true`, destroyed at the end) and recorded **its** monitor, so the session's default sink was never changed.
- **Tool:** `cargo run -p cm-audio --example system_drift_probe` (new; `crates/cm-audio/examples/system_drift_probe.rs`): `SystemAudioRecorder` over the PipeWire monitor of one sink, a session clock, N seconds, then the track details and the beep onsets found in `system.wav`. Playing: `pw-play --target cm-probe-sink beeps.wav`, a 66 s stereo file with a 100 ms 1 kHz beep every 2.000 s (made with Python's `wave`), started 2 s after the recording.
- **Build:** debug build of the work tree of the commit before this report, `--no-default-features`, `CARGO_INCREMENTAL=0`.

This is **not** a real device: the "Dummy Output" and the null sink are driven by the PipeWire graph's timer, which is derived from `CLOCK_MONOTONIC`, so a real clock difference between a sound card and the host clock cannot show here. What it shows is how the stamps behave when the graph changes its quantum.

## Cause

Every PipeWire buffer was stamped "callback time minus buffer length" (`chunk_start_ns`). The trace (callback time next to `pw_stream_get_time`'s `now`, `ticks` and `delay`) showed two things:

1. The callback runs 0.05 to 5 ms after the graph cycle began and its spacing jitters from 14 to 29 ms around the 21.33 ms quantum, while the graph's own `now` is spaced exactly 21.33 ms.
2. When another program starts playing (`pw-play` here, 2 s into the recording) the graph raises its quantum from 1024 to 2048 frames. The first 2048-frame buffer arrives 21.4 ms after the last 1024-frame one, and "callback time minus 42.7 ms" then lies 21.3 ms before the end of the previous buffer. The pipeline took that for an overlap, trimmed 21.33 ms of good audio and recorded one resync (`max_offset_ms` 21.3, `trimmed_ms` 21.33). Its drift estimator, fed one point of that step, printed -2229 ppm (-94 ppm and -887 ppm in the 60 s and 20 s runs below: the figure is the step divided by the span, not a rate). The ticks are contiguous across the switch (each buffer starts where the previous one ended), so no audio was lost: the trim itself was the error. Under CPU load the callback jitter alone crossed the 20 ms tolerance (one run: 3 resyncs, 33 ms inserted, 46 ms trimmed, `max_offset_ms` 33).

So it was an estimation bug, not a rate error: the beeps were 2.000 s apart all along.

## Fix

`graph_start_ns` (`crates/cm-audio/src/sys/linux_pipewire.rs`): the first frame of a buffer is stamped with the graph cycle's `now` minus the stream's `delay` (PipeWire's own `pw_time`, same monotonic clock as `SystemHostClock`), and the old "callback time minus length" stays only as the fallback when PipeWire gives no usable report. A sink monitor has `delay` 0 (its data is the mix of the cycle that is starting), so its buffer begins at the cycle start whatever the quantum; a capture device reports the latency it adds. Real drift is not hidden: the stamps still come from the host clock and the pipeline's 20 ms resync and `drift_ppm` logic are unchanged.

Side effect, stated plainly: a system-audio stamp is now one quantum (21 ms, or 43 ms when the quantum is larger) later than before for the same sound, and it no longer depends on the quantum. The first beep landed at 2.484 s in the file before and 2.590 s after, in runs with different start times, so that is not a comparison of the two. Whether the new absolute offset to the picture is right needs the A/V flash-and-beep check on a real session (`stamp_offset_ms` is the place for a measured constant).

## Numbers (60 s, the beeps played from 2 s, `pw-play` raising the quantum to 2048)

| | before (stamp = callback time - length) | after (stamp = graph time - delay) |
| --- | --- | --- |
| resyncs | 1 (quiet VM, 60 s and 20 s runs); 3 and 4 (loaded VM, 60 s runs) | **0**, **0** (two 60 s runs, loaded VM, load average 1.4 to 3.5) |
| trimmed / inserted | 21.33 ms / 0 (quiet); 45.6 and 47.1 ms trimmed, 33.2 and 48.0 ms inserted (loaded) | 0 / 0 |
| `max_offset_ms` | 21.3 (quiet); 33.2 and 26.5 (loaded) | **0.001** |
| `drift_ppm` in the track details | -94, -887, -134, -72 | **-0.016** |
| beeps found / spacing in `system.wav` | 29 / 2.000 s every time | 29 / 2.000 s every time |
| `duration_ms` | 60020, 60016, 60007 | 60032, 60032 (one quantum of 2048 frames past the 60 s stop) |

## Tests

- `cargo test -p cm-audio --lib linux_pipewire` on macOS and in the VM: `graph_time_does_not_depend_on_the_buffer_length`, `an_unusable_graph_report_is_ignored` (zero or negative `now`, a 0 rate, a delay beyond one second, a stamp that would be negative) and the two older tests pass. The VM run also passed the whole `cm-audio`, `cm-platform`, `cm-auth` and `cm-overlay` suites and `cargo clippy --all-targets --no-default-features -- -D warnings` for those crates and `cm-desktop`.
- No automated test replays the quantum switch (it needs PipeWire); the probe above is the evidence.

## Not shown

- A real sound card or USB device with its own clock (drift and a non-zero `delay`), a Bluetooth sink, a PipeWire graph with a rate change (44.1 kHz device), the microphone path with a device that reports a delay (the same function stamps it, never run with real input), a second quantum change back to a smaller size, a PipeWire daemon restart.
- A/V alignment with the new stamps (flash and beep) on a real session.

## Verdict

PARTIAL. The 21 ms resync and the -2229 ppm figure were a stamping error at a graph quantum change (and callback jitter), reproduced in the VM and gone after the change in two 60 s runs. Real clock drift against a physical device, the microphone with a reported delay and the picture/sound offset are not measured.
