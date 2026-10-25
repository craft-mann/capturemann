# AUD-1 manual script: microphone, system audio, noise suppression (`cm-audio`)

Covers what CI cannot: a real microphone, a real default-device switch, real sync. Everything
else (timestamp placement, drift, gaps, pause, device loss, resampling, denoise on/off, WAV crash
recovery, level math) is covered by `cargo test -p cm-audio` with `FakeAudioSource`.

**Privacy:** steps 2 to 7 record your microphone or whatever the computer plays. Keep runs short
(the helpers cap at 10 s) and delete the output folders afterwards. Nothing here prompts: grant
Microphone (and, for system audio, Screen Recording) to the terminal app first in System Settings >
Privacy & Security, restart the terminal, and check with step 1. The ignored tests print the
permission state and stop when it is not "authorized".

## 1. Devices and permission state (no recording)

```
cargo test -p cm-audio --test hardware lists_devices -- --ignored --nocapture
```

Pass when: every microphone appears with `id`, `name`, `channels`, `sample_rate`; exactly one has
`default: true` and it matches System Settings > Sound > Input; `system_audio.supported` is true on
macOS 13+ with `backend: screencapturekit`; `microphone_permission` is `authorized` after you grant
it (and `denied` after you revoke it: the message then reads "Microphone access is off...", AUD-1.AC5).

## 2. Microphone smoke test (3 s, microphone on)

```
cargo test -p cm-audio --test hardware mic_records -- --ignored --nocapture --test-threads=1
```

Speak for the 3 s. Pass when: the test passes (raw and denoised files of equal length, about 3 s),
the printed `details` show `rate` 48000, `device` the one you expect, `drift_ppm` small (null on a
3 s run is fine), `resyncs.count` 0, and `stats.dropped` 0. The files are deleted by the test.

## 3. Meter and device switching (AUD-1.AC1, microphone on, no files)

```
cargo test -p cm-audio --test hardware monitor_shows_a_level -- --ignored --nocapture
```

Pass when a non-silent level prints within 600 ms. Then, with two microphones (for example the
built-in one and a USB or Bluetooth one), change the default input in System Settings > Sound
and rerun: the printed `format.device_name` follows the new default. (The app's picker uses
`MicMonitor::start(DeviceSelector::Id(..))`, which restarts the monitor on the chosen device.)

## 4. Timestamp calibration and mic vs system sync (NFR-3, AUD-1.AC2, AUD-2.AC1)

Goal: confirm that mic and system tracks, stamped by OS capture time, line up on the session
clock, and learn whether `MicConfig::stamp_offset_ms` needs a non-zero default.

1. `cargo run -p cm-audio --example sync_probe -- record --yes --secs 8 --out /tmp/audio-sync`
2. Within the 8 s, in another terminal, play a beep through the speakers at normal volume:
   `afplay /System/Library/Sounds/Ping.aiff` (headphones would starve the mic of the beep).
3. `cargo run -p cm-audio --example sync_probe -- analyze --out /tmp/audio-sync`

Pass when `mic - system` is within a few ms (acoustic delay is about 3 ms per meter of distance
between speaker and microphone, so expect a small positive number) and in any case inside 50 ms.
Record the result in docs/SPIKES.md S3. A consistent offset larger than that means cpal's `capture`
stamp (callback time minus buffer latency) is off on this device: set `stamp_offset_ms` to cancel
it. For a video-level check use the S3 flash/beep method on a finished recording once the session
wiring lands (method of Oct 19: generated flash/beep video, FFmpeg analysis of `final.mp4`);
AUD-1.AC2 needs it at the start and end of a 30-minute recording.

## 5. 44.1 kHz device (AUD-1.AC3)

In Audio MIDI Setup, set the input device format to 44100 Hz (or use a Bluetooth headset in call
mode, which runs at 16 kHz or 24 kHz). Run step 2. Pass when `mic.wav` is 48000 Hz, 3 s +- 10 ms
(the test asserts 2.7 to 3.3 s; the printed `frames` should be 144000 +- 480) and
`details.source_rate` shows the device rate.

## 6. Unplug (AUD-1 edge case, microphone on)

Use step 2's test but with a 3 s window long enough to unplug a USB microphone that is the default
input (extend the sleep in the test locally or use the app once wired). Pass when capture continues
on the new default within 1 s: `events` contain `device_lost` then `device_changed`, the
`details.gaps` entry has `reason: "device_changed"`, and the file is as long as the recording
(silence over the gap).

## 7. System audio (AUD-2, Screen Recording on)

```
cargo test -p cm-audio --test hardware system_audio -- --ignored --nocapture --test-threads=1
```

During the 4 s, play a tone or music (`afplay /System/Library/Sounds/Ping.aiff` repeatedly). Pass
when `system.wav` is 48 kHz stereo about 4 s and the printed level shows the tone. AC2 (countdown
sound not in the track): the app's own process audio is excluded by `SCStreamConfiguration`; if the
countdown is played by a different process than the one recording, add that process's bundle id to
`SystemAudioConfig::exclude_bundle_ids` and verify the countdown is absent from `system.wav`.
AC3 (Windows device change) needs a Windows machine: change the default output while recording and
check `device_changed` appears within 2 s.

## 8. Noise suppression fixture (AUD-3.AC1)

Record 20 s of speech with a fan or typing in the room (a real recording, via step 2 with the sleep
extended), then compare the noise floor of the pauses in `mic.wav` and `mic.denoised.wav`. Pass
when the denoised floor is at least 15 dB lower. The synthetic version of this check runs in CI
(`denoise::tests::suppresses_fan_noise_between_speech_by_more_than_15_db`); CPU (AC2) is measured
by `cargo test -p cm-audio --release --test perf -- --ignored --nocapture` (about 0.4% of one core
for mono at 48 kHz on an M-series Mac).
