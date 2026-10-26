# Voice and meetings (VOC)

Audio-only recording for meetings (online and in the room), interviews, one-on-ones, lectures, product introductions and voice notes. It reuses the recording engine (one session clock, one track per source, crash-safe WAV masters from AUD-1 and AUD-2), the `.cmrec` bundle and the library; the transcript, translation, speaker labels and reports come from AI-1, AI-5, AI-8 and AI-9. Plan, model choices and spikes: `../plans/voice-and-meetings-plan.md`.

v0.1.0 has shipped, so "P1" in this file means **in the next stage of work** (order in the plan), not v0.1. There is no release date; a release is cut when the maintainer decides.

### VOC-1 Audio-only recording
**Priority** P1 · **Status** Ready · **Crates** cm-audio, cm-daemon, cm-library, apps/overlay, apps/desktop · **Depends on** AUD-1, AUD-2, AUD-3, AUD-6, REC-4, REC-5, REC-8, AGT-2, DESIGN §5.2, §5.7

**User story** — As someone running an interview, a meeting or a one-on-one (online or with my laptop on the table), I want to record just the audio, so that I get a full recording, a transcript and an AI report afterwards without recording my screen.

**Description**
- **A third capture mode.** "Record audio" sits next to the screenshot and screen-recording entries: tray menu, launcher (LNC-1), the start sheet's mode switch (Screen / Audio), and a new default shortcut **⌥⇧V** (Windows Ctrl+Alt+Shift+V), configurable with the others (CAP-2). An agent uses `start_recording { mode: "audio" }`.
- **Sources.** Microphone (AUD-1 device picker and level meter, noise suppression per AUD-3), system audio (AUD-2), or both. Default: microphone only. Each source is its own track, exactly as in a screen recording (`mic.wav` / `mic.denoised.wav`, `system.wav`). No screen, camera or cursor track is opened, and **Screen Recording permission is not requested for microphone-only recordings** (macOS needs it only when system audio is on, because system audio comes from ScreenCaptureKit; the sheet says so next to the switch).
- **Use-case presets in the sheet** (they only set defaults and the AI-9 template suggested after stop; the user can change every source):

| Preset | Sources | Report suggested after stop |
| --- | --- | --- |
| In the room (interview, 1:1, meeting around a laptop) | Mic | Meeting minutes, or Interview notes when chosen |
| Online meeting (Zoom, Meet, Teams, a browser call) | Mic + system audio | Meeting minutes |
| Lecture or talk | Mic | Lecture notes |
| Product introduction / demo call | Mic + system audio | Product presentation recap |
| Voice note | Mic | Summary |

- **Bundle.** A `.cmrec` with `kind: "audio"` in the manifest and no `screen.mp4`; the library index stores `kind = audio`. Everything that works on a recording's audio (transcribe, captions, AI-4 suggestions, trim and cut, upload to a destination) works unchanged.
- **HUD (DESIGN §5.2, compact variant).** Timer, mic level meter, Pause/Resume (REC-4), Mute mic (AUD-6), **Marker**, Stop, Discard. No area brackets (there is no area). The tray shows the recording state as for screen recordings (REC-5); an agent-started recording shows the same agent notice.
- **Markers.** The Marker button or **M** while the HUD is focused, the global shortcut ⌥⇧M (Ctrl+Alt+Shift+M), or `add_marker` adds `{ at, label? }` to `manifest.markers` (session time, so pauses don't shift them). Markers show on the editor's waveform and in AI-9 reports ("Marked moments").
- **Consent notice.** Recording other people without telling them is illegal in many places. The first audio recording, and every recording whose preset involves other people, shows a one-line notice in the start sheet: "Let everyone know they're being recorded." with "Don't show again" (setting `recording.consent_notice`, default on). The agent confirmation dialog always carries the same line. CaptureMann gives no legal advice beyond this line.
- **Long sessions.** Supported up to `max_duration_s` (default 4 h for audio, configurable to 12 h). While recording, the app holds an OS "prevent idle sleep" assertion (macOS `IOPMAssertion`); closing a laptop lid without an external display still sleeps the Mac, so the sheet's In-the-room preset shows "Keep the lid open". Disk use is shown before start: mic about 0.7 GB per hour (48 kHz 32-bit float mono master plus the denoised copy is about 1.4 GB), system audio about 1.4 GB per hour; the disk floor (NFR-7) applies.
- **After stop.** The bundle opens in the audio editor (VOC-2). When `ai.auto_transcribe` is on and a model is installed, transcription starts as for screen recordings (AI-1), followed by speaker labels (AI-8) when the setting `ai.auto_speakers` is on (default off until S7 passes).

**Acceptance criteria**
- **AC1** Given Audio mode with the microphone only, when the user records 60 s, then the bundle has `kind: "audio"`, a 60 s ± 20 ms mic track, no screen, camera or cursor files, and no Screen Recording permission prompt appeared.
- **AC2** Given mic and system audio, when a call plays through the speakers for 10 minutes, then both tracks exist and their start and end stay within 50 ms of each other on the session clock (NFR-3).
- **AC3** Given ⌥⇧V, then an audio recording starts with the last-used sources and the compact HUD shows timer, level meter, pause, mute, marker and stop.
- **AC4** Given the user presses Marker at 02:00, then `manifest.markers` holds an entry at 120 s ± 50 ms, and after a 30 s pause at 01:00 the marker is still at 120 s of recorded time (pauses removed).
- **AC5** Given the process is killed (`kill -9`) 5 minutes in, when the app starts again, then the recording is recovered with at least 299 s of audio (REC-8, AUD-1 header repair).
- **AC6** Given a 3-hour mic + system recording, then memory stays under 200 MB and the drift between the two tracks stays under 50 ms at the end.
- **AC7** Given the consent notice is on, then the start sheet shows it before Record, and an agent's `start_recording { mode: "audio" }` dialog contains the same sentence.
- **AC8** Given an agent calls `start_recording { mode: "audio", mic: true }`, then the Confirm dialog says "Record audio" with the microphone name and no screen target, and on approval a `kind: "audio"` recording starts.
- **AC9** Given `mode: "audio"` together with `target`, `camera`, `fps`, `size`, `codec`, `quality` or `cursor`, then the call fails with `invalid_input` naming the field and nothing starts.
- **AC10** Given the library, then audio recordings appear under a new **Audio** sidebar item (with count) and in All, each with a waveform thumbnail, title, date and duration; transcript search (LIB-1) finds them.

**Edge cases and errors**
- Microphone permission missing → `permission_missing` (`microphone`) before the dialog, as REC-1.
- System audio requested without Screen Recording permission → `permission_missing` (`screen`) with "System audio needs Screen Recording permission on macOS"; the sheet offers microphone only.
- Mic disconnected mid-recording → AUD-1 behaviour (`audio_device_*` warning, silence, switch to default if chosen).
- Mac goes to sleep (lid closed) → recording stops with `reason: "system_sleep"` and keeps what was captured.
- A screen recording is running → `busy`, as REC-1.AC6.

**Agent interface** — `start_recording { mode: "audio", mic?, system_audio?, preset?: "room"|"online"|"lecture"|"product"|"note", title?, max_duration_s?, countdown_s? }` (**Confirm**, OD-24 policy applies) → the same result object as REC-1 with `tracks` listing only audio tracks. `add_marker { recording_id?, label? }` → `{ marker }` (Free; also allowed after stop with `capture_id` and `at`). Pause, resume, stop, status and events as REC-1/REC-4. CLI `capturemann record start --audio [--mic <id>] [--system-audio] [--preset room]`, `capturemann record marker [--label ...]`. `list_library { kind: "audio" }`.

**Out of scope** — Capturing one app's audio only (P2, ScreenCaptureKit per-app filter), phone or Bluetooth hands-free call capture, meeting bots that join calls, calendar integration and automatic meeting detection (P2), compact masters (FLAC/AAC) instead of WAV (decided Oct 26, 2026: masters stay WAV; compacting is P2).

**Test notes** — Integration tests with scripted audio devices (no screen track, manifest kind, markers through pauses); kill -9 recovery test; 3-hour soak on hardware (`verification/` report); manual script `tests/manual/VOC-1.md` (permissions, consent notice, HUD, lid-open hint).

---

### VOC-2 Audio editor view and audio export
**Priority** P1 · **Status** Ready · **Crates** apps/desktop, cm-media, cm-daemon · **Depends on** VOC-1, EDT-1, AI-1, AI-4, DESIGN §5.5

**User story** — As a meeting host, I want to review the recording with its transcript, cut the small talk and export the audio, so that I can share a clean recording or keep it as a record.

**Description**
- The editor (DESIGN §5.5) shows, for `kind: "audio"`, a **waveform per track** (mic, system) instead of the video preview, with the timeline, markers and the caption/transcript panel. Playback, scrubbing, J/K/L and Space work as for video.
- The transcript panel shows lines with speaker names when AI-8 has run ("Linh 00:04:12 — …"); clicking a line seeks. Caption editing, find and replace and glossary (AI-1, AI-6a) work unchanged.
- Trim, cut, split and speed (EDT-1, EDT-2) and AI-4 silence/filler suggestions apply to all audio tracks together.
- **Audio export** (also available for screen recordings, as "Export audio only"): format **M4A** (AAC-LC, 48 kHz, 128 kbps mono or 192 kbps stereo; default) or **WAV** (48 kHz, 24-bit); tracks: Mix (default), Mic only, System only; loudness normalization to −16 LUFS (on by default for M4A); title and date written as file tags. Runs as an `export` job with progress (EXP-4). WAV needs the `wav` muxer and `pcm_s24le` encoder added to the LGPL FFmpeg build (`packaging/ffmpeg/build-macos.sh`). Output in the bundle's `exports/` and optionally a folder.
- Transcript exports (TXT, SRT, VTT from AI-1) gain a **Speaker names** switch (AI-8) and the report exports come from AI-9.

**Acceptance criteria**
- **AC1** Given a 10-minute audio recording, then the editor shows one waveform per recorded track with markers at their times, and Space plays from the playhead.
- **AC2** Given a cut from 01:00 to 01:30, when exporting M4A Mix, then the file is 9:30 ± 1 frame of audio (20 ms), AAC 48 kHz, and its integrated loudness is −16 ± 1 LUFS.
- **AC3** Given Mic only, then the export contains no system audio (cross-correlation with `system.wav` below 0.1).
- **AC4** Given a screen recording, when "Export audio only" runs, then an M4A with the same cuts is produced and no video is encoded.
- **AC5** Given a 60-minute recording on an M-series Mac, then M4A export finishes in ≤ 60 s.

**Edge cases and errors** — No transcript yet → the panel shows Transcribe (AI-1) or the no-model hint. FFmpeg missing → as EXP-1 (`job_failed`, `reason_code`).

**Agent interface** — `export_audio { capture_id, format?: "m4a"|"wav", tracks?: "mix"|"mic"|"system", loudness_lufs?: number|null, out_dir?, request_id? }` → `{ job_id }` → `{ path, size_bytes, duration_s }` (Free). Editing uses the existing tools (`trim`, `cut_segments`, `split`, `set_speed`). CLI `capturemann export <id> --audio [--format wav] [--tracks mic]`.

**Out of scope** — MP3 (needs an extra FFmpeg component; P2), multitrack WAV stems, audio effects (EQ, compression).

**Test notes** — Golden export tests (duration, codec, loudness with `ffmpeg -af ebur128`), the Mic-only isolation check with a fixture whose system track is a tone; manual script `tests/manual/VOC-2.md`.

---

### VOC-3 Live transcript while recording
**Priority** P2 · **Status** Draft (detail before work starts) · **Crates** cm-ai, apps/overlay

**User story** — As a meeting host, I want to see the transcript appear while we talk, so that I can catch a name or a number immediately and mark moments.

**Summary** — Stream the mic (and system) audio through a small Whisper model (base or small) in 5–10 s windows with VAD (whisper.cpp streaming) into an expandable HUD panel; the final transcript is still made after stop with the user's chosen model (AI-1). Clicking a live line adds a marker there.

**Key criteria (to refine)** — Live text appears within 3 s of speech; CPU under 30% of one machine on an M-series Mac with `base`; never blocks or degrades the recording (drops live windows instead); off by default.
