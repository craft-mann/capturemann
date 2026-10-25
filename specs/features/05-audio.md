# Audio (AUD)

**Build status (Oct 21, 2026, macOS only).** `cm-audio` is built: device listing, permission state, the mic recorder with RNNoise, the system-audio recorder (ScreenCaptureKit), level meters and crash-safe WAV files; both recorders are wired into the recording session. Windows (cpal WASAPI, loopback) is written but never run. Verified on macOS hardware (single short runs of `record_e2e`, about 30 s or less, output not stored): microphone capture, a drift of about -1.7 ppm and system audio capture; the drift figure is a single short run and says little about 30 minutes; tested with fakes: timestamp placement, gaps, pause, device loss, resampling, denoise on and off, WAV recovery, level math. The 50 ms sync against video (NFR-3) is not measured. Manual script: `tests/manual/AUD-1.md`.

### AUD-1 Microphone with device picker and level meter
**Priority** P0 · **Status** In progress (built on macOS; AC2 sync unmeasured; Windows never run) · **Crates** cm-audio (`cpal`) · **Depends on** REC-1 · **Spike** S3 (passed: 48 kHz mono, drift +0.2 ms over 60 s)

**User story** — As a creator, I want to choose my microphone and see its level, so that I know my voice is being recorded before I start talking.

**Description**
- Device list from the OS (`id`, name, channels, sample rate, default flag, from `list_audio_devices`, which is the audio half of `list_devices`); default = system default input; last choice remembered with fallback to default.
- Live level meter in the setup panel and the control bar (5 bars; green to -12 dBFS, amber to -3 dBFS, red at clipping), updated at **30 Hz** (the first build used 20 Hz; the meter is cheap and 30 Hz matches a 33 ms window). It reports peak and RMS in dBFS; `get_recording_status` carries `mic_level_dbfs` (peak) and `mic_rms_dbfs`.
- Input gain is the OS's; CaptureMann shows a "Too quiet" hint if the peak stays below -40 dBFS for 5 s while recording, and "Clipping" if peaks hit 0 dBFS more than 3 times in 10 s.
- Stored as `mic.wav`: 48 kHz (resampled if the device runs at another rate), **32-bit float**, mono or the device's channel count **capped at 2**; timestamps from the capture callback info, converted to `MediaTime`. `mic.wav` is always the **raw** file.
- **File naming.** With noise suppression on (the default) the track's `TrackInfo.file` in the manifest is `mic.denoised.wav` and the raw file is named in `details.raw_file` (`mic.wav`); with it off, `TrackInfo.file` is `mic.wav` and there is no denoised file. Both files have identical length.
- **Alignment by timestamp.** Every chunk is compared with where its capture stamp says it should be; if it is more than 20 ms late, silence is inserted, and if it is more than 20 ms early the excess leading frames are trimmed. This bounds the error against the session clock to 20 ms whatever the device clock does; the slope of the raw offsets is reported as `details.drift_ppm`, and `details.resyncs` counts the corrections. Real dropouts (device lost, queue overrun) are silence-filled and listed in `details.gaps`; `gaps` holds dropouts only, **never pauses**: pauses are under `details.pauses` (REC-4). The first sample is at `TrackInfo.start` (about 80 ms after the session origin on macOS), which is start latency, not drift.
- **Crash safety.** The WAV header is written up front and patched about once per second, so a crash leaves a header at most about 1 s stale; recovery repairs it from the file length (`repair_wav`). The WAV header is 80 bytes with a `JUNK` chunk reserved, so the writer continues as **RF64** past 4 GiB (about 3 hours of 48 kHz stereo float) instead of failing. A track whose file stops being writable (disk full, I/O error) is kept and truncated (`details.truncated_at`) with a `track_failed` warning, never deleted. After a crash, the lock's hints (start offset, the denoised file's name) let recovery rebuild the track entry and repair both files with `repair_wav` (REC-8). The writer calls `sync_data` when it patches the header.

**Acceptance criteria**
- **AC1** Given two microphones, then both are listed and switching changes the meter within 500 ms.
- **AC2** Given a 30-minute recording, then mic audio stays within NFR-3 (≤ 50 ms) of video at start and end (flash/beep test).
- **AC3** Given a 44.1 kHz device, then `mic.wav` is 48 kHz with correct duration (± 10 ms).
- **AC4** Given silence for 5 s while recording, then the control bar shows "Too quiet". (The daemon raises the `mic_silent` warning in `recording.warning`; the HUD hint is not built.)
- **AC5** Given microphone permission is denied, then recording can start without a mic, the panel shows the permission message, and the manifest has no mic track.

**Edge cases and errors** — Device removed mid-recording → switch to the system default within 1 s, mark the gap in `session.json`, toast "Mic changed to `<name>`". Bluetooth headset switching to a low-quality profile is the OS's choice; show the active sample rate.

**Agent interface** — `list_devices` → mics `{ id, name, channels, max_channels, sample_rate, default }`, `microphone_permission`; `start_recording { mic: false | true | "default" | device_id | { device_id?, denoise? } }`. `get_recording_status` includes `mic_level_dbfs` (peak), `mic_rms_dbfs` and `mic_clip_events`.

**Settings (Settings → Webcam & audio, board R13)** — `audio.mic_device` (a device id string; a missing key, `null` or `""` means the system default input; agent-writable in the `audio` family) is the microphone used when none is named, in this order: (1) the microphone named by the request (`start_recording` `mic: "<id>"` or `{ device_id }`, `capturemann record start --mic ID`) or picked in the Record sheet for one recording (not saved); (2) `audio.mic_device` when that microphone is connected; (3) the system default input. `mic: true` / `mic: "default"` / `--mic default` mean (2) then (3). A saved microphone that is not connected is replaced by the system default for that recording, and the Settings row shows its name with "Not connected" and "Using the system default until it is plugged in."; the saved id is kept. The Microphone dropdown never offers Off (the Record sheet's microphone switch and `mic: false` do that).

**Out of scope** — Multi-mic recording, audio monitoring through headphones.

**Test notes** — S3 sync measurement automated: generated flash/beep video, FFmpeg analysis of `final.mp4` (the method used on Oct 19).

---

### AUD-2 System audio
**Priority** P1 · **Status** In progress (macOS built and recorded on hardware; Windows loopback never run) · **Crates** cm-audio, cm-capture · **Depends on** REC-1

**User story** — As a teacher, I want the sound of videos I play during the lesson recorded too, so that students hear what I hear.

**Description**
- macOS: ScreenCaptureKit audio capture (macOS 13+), excluding CaptureMann's own sounds; Windows: WASAPI loopback of the default output device.
- **macOS implementation.** System audio runs on **its own audio-only `SCStream`** (a 16×16 px, 1 fps video stream whose frames are discarded, because ScreenCaptureKit needs a content filter), separate from the screen track's stream, so system audio can be toggled independently and the screen recorder needs no audio code. The current process's audio is always excluded, and `SystemAudioConfig.exclude_bundle_ids` adds other apps (for example a helper process that plays the countdown sound). Audio arrives as 48 kHz stereo float on the host clock.
- **Stop behavior.** `system.wav` ends about 50 to 65 ms before the stop time, because the last buffer is still in flight inside ScreenCaptureKit when the stream is stopped. The 50 to 65 ms was measured on hardware in two runs. Checks of the file's end against the recording duration therefore use a tolerance of 100 ms (the `record_e2e` harness uses 100 ms); this is tail loss, not drift, and it does not move earlier samples. The same alignment rules as the mic apply (20 ms resync, silence for gaps).
- Stored as `system.wav` (48 kHz, 32-bit float, stereo) on the session clock.
- Off by default; toggled in the setup panel; level meter alongside the mic meter.
- Feedback guard: if both mic and system audio are on and no headphones are detected, show a hint "Use headphones to avoid echo".

**Acceptance criteria**
- **AC1** Given system audio on and a test tone playing, then `system.wav` contains the tone aligned within NFR-3 to the video.
- **AC2** Given CaptureMann plays its countdown sound, then it isn't in `system.wav`.
- **AC3** Given the output device changes mid-recording (Windows), then capture follows the new default device within 2 s and the gap is marked.

**Edge cases and errors** — macOS 12 or older → option hidden. DRM-protected audio may be silent by OS design.

**Agent interface** — `start_recording { system_audio: true }`.

**Out of scope** — Per-app audio capture (P2).

**Test notes** — Tone test both OSes; device-switch manual test on Windows. `tests/manual/AUD-1.md` step 7.

---

### AUD-3 Microphone noise suppression
**Priority** P1 · **Status** In progress (built on macOS; AC1 tested synthetically, AC2 about 0.4% of one core on an M-series Mac, AC3 needs the editor) · **Crates** cm-audio (`nnnoiseless`) · **Depends on** AUD-1

**User story** — As a teacher at home, I want fan and keyboard noise reduced, so that my lessons sound professional without extra equipment.

**Description**
- RNNoise-based suppression (`nnnoiseless`, BSD-3) runs in real time on the mic stream, writing `mic.denoised.wav` beside the raw `mic.wav`.
- On by default; strength not adjustable in v0.1 (on/off). The default comes from the setting `audio.noise_suppression` and a request can override it with `mic: { denoise }`. When on, the manifest's mic track points at `mic.denoised.wav` and names the raw file in `details.raw_file` (see AUD-1).
- Exports use the denoised track when on; the user can switch to the raw track per capture after recording (no re-recording).

**Acceptance criteria**
- **AC1** Given a speech + fan noise fixture, then the denoised output's noise floor in non-speech sections is at least 15 dB lower than the raw track.
- **AC2** Given suppression on, then CPU cost is < 5% of one core on a mid-range laptop.
- **AC3** Given a capture, when the user turns suppression off in the editor, then the export uses `mic.wav`.

**Edge cases and errors** — Music recorded through the mic gets damaged by suppression: show a hint in the editor "Turn off noise suppression for music".

**Agent interface** — Default via `set_settings` `audio.noise_suppression`; per capture via MCP `set_audio_mix { capture_id, denoise: true|false }` (Free, one undo step).

**Out of scope** — Echo cancellation, EQ, compression (P2).

**Test notes** — Fixture-based dB measurement in CI.

---

### AUD-4 Separate audio tracks, mixed on export
**Priority** P0 · **Status** In progress (the interim `quick` export mixes mic and system audio on the session clock with a peak limiter, offsets from the manifest and a mono mic layout, tested with FFmpeg-gated tests. Built and tested with synthetic signals, Oct 22, 2026: `cm_audio::mix` (per-track gain and mute, BS.1770-4 integrated loudness with both gates, a -1 dBTP limiter checked on a 4x oversampled peak, the mix on the session clock through `TrackInfo.start` and `manifest.gaps`) and `set_audio_mix` (`capturemann edit audio`), stored in `manifest.edits[0].audio`; the edited export calls `cm_audio::mix` and `export_video { loudness_lufs }` (-40 to -5) normalises the level; AC1 (+6 dB within 0.5 dB), AC2 (-14 LUFS within 1 LU, true peak under -0.5 dBTP after the AAC encode) and AC4 (a muted source is silent) are tested on a real export with FFmpeg and a GPU on this Mac; not built: the editor controls, the platform presets that set -14 by themselves (EXP-2), the denoise switch in the export) · **Crates** cm-media, cm-render · **Depends on** AUD-1, AUD-2, EXP-1

**User story** — As a creator, I want mic and system audio stored separately, so that I can balance or mute them after recording.

**Description**
- Tracks stay separate in the bundle. The editor has per-track volume (−∞ to +12 dB) and mute.
- Export mixes the tracks on the session clock (offset-aligned), applies a limiter at −1 dBTP, and normalizes integrated loudness to the preset's target (−14 LUFS for platform presets, off for "Source").
- Track times: each audio file starts at its own first-sample time (`TrackInfo.start`, session time) and, after a pause, is compacted (REC-4): the mixer places samples with `TrackInfo.start` plus `manifest.gaps`, never by assuming file time 0 is session time 0.
- Mono mic tracks are mapped to a proper mono layout before AAC encoding (bug found in S3: FFmpeg reads hound's mono WAV as layout "FL", which the AAC encoder rejects).

**Acceptance criteria**
- **AC1** Given mic and system tracks, when exporting with mic at 0 dB and system at −6 dB, then the mix reflects those gains (measured on test tones ± 0.5 dB).
- **AC2** Given a YouTube preset, then the export's integrated loudness is −14 LUFS ± 1 LU and true peak ≤ −1 dBTP.
- **AC3** Given a mono mic WAV, then the AAC export succeeds (regression test for the S3 bug).
- **AC4** Given system audio muted, then it's absent from the export.

**Edge cases and errors** — Different track lengths → mix to the video length; silence-pad shorter tracks.

**Agent interface** — *As built:* gains are -96 to +12 dB (silence is `mic_mute` / `system_mute`; JSON has no -inf), at least one field is required, and the mix is capture-level, one undo step. MCP `set_audio_mix { capture_id, mic_gain_db?, system_gain_db?, mic_mute?, system_mute?, denoise? }` → `{ revision }` (Free); `export_video { loudness_lufs?: -14 | null }`. CLI `capturemann edit audio <id> --system-gain -6`.

**Out of scope** — Per-segment volume automation (P2).

**Test notes** — ffmpeg `ebur128` measurement in CI; S3 mono regression fixture.

### AUD-6 Mute the microphone while recording
**Priority** P1 · **Status** In progress (built on macOS Oct 25, 2026 and tested with fakes only: the scripted mic source, fake tracks and the daemon; the HUD button is not seen by eye and no hardware run is recorded, script `tests/manual/AUD-MUTE.md`) · **Crates** cm-audio, cm-daemon, cm-mcp, cm-cli, apps/desktop · **Depends on** AUD-1, REC-5

**Behavior** — The HUD Mic button (board R18), `capturemann record mic --mute|--unmute` and the MCP tool `set_mic_muted { muted }` mute and unmute the microphone of the running recording, also while it is paused. Muting never stops or restarts the track, so sync and the session clock are untouched. While muted, `mic.wav` and `mic.denoised.wav` hold digital silence (exact zeros) and the files keep their length. The muted spans are listed in the manifest as `mic_muted: [{ from, to }]` in session ticks (also in `session.mic_muted` and `tracks.mic.muted`); `to` is `null` only for a span still open when a crash was recovered, and a span open at stop ends at the stop time. Muting twice changes nothing (`changed: false`). It needs a recording with a mic track (`invalid_input` otherwise, `not_found` with no recording). Events `recording.mic_muted` and `recording.mic_unmuted`; `get_recording_status` and `recording.stats` carry `mic_muted`, which is how the HUD follows a mute made elsewhere.

**Boundary rule** — A frame is silenced if and only if its capture time (the OS stamp of its chunk plus its offset in the chunk, plus the calibration offset) is at or after the mute time and before the unmute time. The rule is on the stamp axis like the pause gates, so it does not depend on how the OS cut the audio into chunks. The mute time is read under the lock the recorder's worker reads the spans with, so no real sample of a muted span can reach a file; the output frames of a muted span are zeroed after the resampler and the denoiser as well, so their filter tails cannot leak into it. The digital-silence warning (`mic_silent`) looks at the live signal and is not raised by a deliberate mute.

**Agent interface** — `set_mic_muted { muted, recording_id?, request_id? }` → `{ recording_id, muted, changed, at_ms }` (Free: the recording itself was confirmed). CLI `capturemann record mic --mute|--unmute [--id]`. No global shortcut in v0.1.

**Test notes** — `cargo test -p cm-audio --test mic_mute` (boundary exactness, denoised file, 44.1 kHz, pause while muted, mute before the first sample), `cargo test -p cm-daemon --test recording_session aud6`, `--test recording_engine aud6`, the MCP and CLI tests, and `apps/desktop` model and layout tests.
