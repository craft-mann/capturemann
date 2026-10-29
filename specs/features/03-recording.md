# Recording (REC)

Recording writes each source as its own track on one session clock (Technical design → Recording pipeline). Spikes: S3 passed (macOS, 1662×1080 30 fps, 0.06% late frames, A/V 21 ms). S6 is a conditional Go on macOS (screen 4K60 passes; with the camera the margin to 1% late frames is thin), so 60 fps stays behind the REC-3 self-test.

**Build status (Oct 21, 2026).** The recording engine is built on macOS: screen, camera, mic, system audio and cursor tracks, one session, pause and resume, crash recovery, and the daemon actions, MCP tools, CLI commands and events for them (M3 slices 1 to 4). The HUD UI (slice 5, REC-5), the countdown window, the area brackets, the live bubble and the start sheet are built in `apps/desktop` and unit tested; none of that UI has a stored hardware verification report. Windows is not built: the encoder and capture stubs return the shared `not_implemented` error (decided Oct 21, 2026; see `specs/README.md`). "Verified on hardware" below means a run on a MacBook Pro (Apple M4 Pro, 3024×1964 built-in display) by the `record_e2e` harness or the `--ignored` tests; "tested with fakes" means CI-style tests with scripted sources. Manual scripts: `tests/manual/REC-1.md`, `REC-5.md`, `REC-7.md`, `CAM-2.md`, `AUD-1.md`.

### REC-1 Record a display, window or region
**Priority** P0 · **Status** In progress (built on macOS: engine, start sheet, countdown, HUD, a clean stop when Screen Recording access is removed mid-recording (tested with fakes, not run on a Mac); screen files verified on hardware, the UI flows and AC2/AC4 by eye are unverified; Windows not built) · **Crates** cm-capture, cm-encode, cm-daemon, apps/overlay · **Depends on** CAP-1, REC-5, AGT-2

**User story** — As a creator, I want to record my whole screen, one window, or a region, so that viewers see exactly what I'm showing.

**Description**
- Start from the tray, the shortcut, the capture toolbar ("Record this area"), or an agent. A setup panel (launcher-style, DESIGN §5.2; the kit has no screen for it) shows: source (Display / Window / Region), size, fps, camera, mic, system audio, and a Record button; the last choices are remembered.
- **Window** recording follows the window when it moves and keeps its size fixed at start; if the window is resized, content is scaled to fit (letterboxed), never cropped.
- **Region** recording uses a rectangle picked with the CAP-1 overlay; the region is fixed on its display.
- CaptureMann's own recorder windows (HUD, bubble, countdown, area indicator, toasts, start sheet, confirmation dialog, launcher) are always excluded from the screen track: the daemon excludes the app's process by pid in every recording, and those windows also set `sharingType = none`. The library window (Studio, which also hosts the editor and the setup and permissions screens) is **captured** while it is visible: the recording shows everything on screen except the recorder UI (see REC-6, AC8).
- A 3-2-1 countdown (DESIGN §5.3, skippable with Space, off-able in settings) precedes recording. The countdown window never takes keyboard focus when it opens, so the app being recorded keeps the keyboard: click the disc once, then Space skips and Esc cancels; the global shortcuts cancel it without a click. Esc during a **start** countdown with no capture yet discards the attempt (nothing is recorded); Esc during a **resume** countdown cancels the resume and the recording stays paused. The setting is `recording.countdown_s` (0 to 10, default 3); the countdown runs in the daemon and publishes `recording.countdown` each second, so every client shows the same numbers. Nothing is recorded or created on disk during the countdown.
- **Two ids.** `recording_id` names the run and exists from the moment a start is accepted (the countdown included); `capture_id` is the library id of the bundle and exists once `recording.started` fires. Pause, resume, stop and status take the `recording_id`; every capture tool takes the `capture_id`. A cancelled or discarded run has a `recording_id` and no capture.
- `start_recording` returns after the countdown, once every track is delivering: `{ recording_id, capture_id, state, live_at_ms, tracks, warnings, indicator }`. `live_at_ms` is the session time at which all tracks are live (Technical design → One clock); the HUD clock starts there. `warnings` lists optional tracks that could not start (for example the camera) and permission fallbacks; the recording continues without them. `recording.starting` is published when the countdown is over and the tracks are being started (a slow camera can take up to 3 s, ScreenCaptureKit up to 20 s), and carries the `indicator`, so the HUD and tray show the recording state before `recording.started` (REC-5.AC1). If the start fails after that (a track would not open, the encoder refused), `recording.discarded { reason: "start_failed", error }` follows and the waiting `start_recording` fails.

**Acceptance criteria**
- **AC1** Given Display mode on a 1920×1080 display, when the user records 10 s, then `screen.mp4` is 1920×1080 (or the chosen output size) and ~10 s long (± 1 frame).
- **AC2** Given Window mode, when the user moves the window during recording, then the recording keeps showing the whole window.
- **AC3** Given Region mode with a 1280×720 region, then `screen.mp4` is 1280×720 with the region's pixels.
- **AC4** Given the control bar and webcam bubble are on screen, then neither appears in `screen.mp4`.
- **AC5** Given the countdown is on, when Space is pressed, then recording starts immediately.
- **AC6** Given a recording is running, when another start is requested (shortcut or agent), then it is refused with `busy` "A recording is already running".
- **AC7** (interim exporter, EXP-1) Given a recording stops normally and the setting `export.auto_on_stop` is on (the default), then the daemon enqueues one `export` job (preset `quick`) after the job holds are released, so a recording from the HUD, the CLI or an agent ends with one combined MP4 in the bundle's `exports/` folder; a discard, a cancelled countdown, the 60 fps self-test or the setting off enqueue nothing. At stop the engine stores the bubble layout in `edits[0].layout.camera` (`{ visible, shape, rect, mirror, size, ring, radius }`, rect as fractions of the recorded area); bubble moves *during* the recording are not recorded (known gap), only the layout at stop.

**Edge cases and errors**
- Window closed during recording → recording stops automatically, keeps what was captured, toast "Window closed — recording saved".
- Display disconnected → same as above for Display/Region mode.
- Screen Recording permission missing → `permission_missing` (`screen`) with guidance; nothing starts. A missing permission for a requested sensor (camera, microphone, screen) is checked before the Confirm dialog opens and refuses the start with `permission_missing` (`data.permission`), so the user is never asked to confirm a recording that cannot run; the caller then offers to continue without that sensor (CAM-4.AC3, AUD-1.AC5). A cursor-clicks permission problem is not an error: it becomes a warning (REC-7.AC4).
- Stop, Discard or Pause during the countdown cancels the start: no files exist, `recording.discarded { capture_id: null, reason: "countdown_cancelled" }` is published, the waiting `start_recording` call fails with `user_declined` and `data.reason: "countdown_cancelled"`, and the stop call itself succeeds with `{ cancelled: true }`.
- A second start while one runs (or counts down) is refused with `busy` before any dialog opens.
- Window closed, display gone, disk below the floor or `max_duration_s` reached → the recording stops by itself with that `reason` (see the Agent interface tab for the list) and keeps what was captured; a later `stop_recording` returns the same result with `already_stopped: true`.

**Agent interface** — MCP `start_recording { target, size?, fps?, codec?, quality?, camera?, mic?, system_audio?, cursor?, max_duration_s?, countdown_s?, title?, exclude? }` → `{ recording_id, capture_id, state, live_at_ms, tracks, warnings, indicator }` (**Confirm** tier, see AGT-2; the full input and output are in the Agent interface tab; `countdown` is accepted as an alias of `countdown_s`). `stop_recording`, `pause_recording`, `resume_recording`, `get_recording_status` take an optional `recording_id`. CLI `capturemann record start|pause|resume|stop|status|watch` and `capturemann devices`. Events: `recording.countdown`, `starting`, `started`, `paused`, `resumed`, `stats` (1 Hz), `health` (5 s), `warning`, `stopped { reason }`, `discarded`, `recovered`; the full list with payloads is in the Agent interface tab. `recording.warning` codes: `capture_stalled` (no frame delivered for over 1 s), `track_failed` (a track can no longer be written and ends where its file ends), `mic_silent`, `disk_low`, `camera_lost`, `screen_degraded`, `fps60_unverified_with_camera`, `screen_stopped_early`, `audio_device_*`. The `indicator` object in `started` and in the status carries what the HUD, the area brackets, the agent toast and the tray need (area bounds in physical px, output size, camera bubble facts, `started_by`), so no client has to guess.

**Out of scope** — Recording several displays at once; recording a single app's windows as a group (P2).

**Test notes** — Integration tests on CI runners with virtual displays where possible; manual matrix: macOS 13/14/15, Windows 10/11, 1×/2× displays.

**Verification** — Verified on macOS hardware: native-size, region (1280×720) and 1080p (1662×1080) screen files from the real display; the state machine, countdown, busy refusal and cancel paths are tested with fakes in `cm-daemon`. Not verified: Windows; AC2 window moving and AC4 bubble exclusion by eye (checks 1 and 2 in `tests/manual/REC-1.md`); macOS 13 and 14 (runs were on macOS 26).

---

### REC-2 Resolution up to native, with no cap
**Priority** P0 · **Status** In progress (macOS: native, 1080p and region sizes built and run on hardware; no software H.264 fallback (removed Oct 24, 2026): a rejected hardware encoder fails the start, tested with fakes, not run on a real rejection; 5K and the 2-hour run not run) · **Crates** cm-capture, cm-encode · **Depends on** REC-1, Spike S6

**User story** — As a creator, I want to record at my display's full resolution (4K/5K) or a size I choose, so that my YouTube uploads are sharp and nothing is ever capped at 720p.

**Description**
- Output size options: **Source** (native physical px, default), 2160p, 1440p, 1080p, 720p. Downscaling keeps aspect ratio; heights are exact, widths rounded to even numbers.
- Codec: HEVC by default when output width > 1920, else H.264; user can force either. If width > 4096 (5K), HEVC is required and H.264 is disabled with a tooltip "H.264 can't encode frames wider than 4096 px".
- *Set-up retries (Oct 29, 2026, GitHub issue 16, an Intel MacBook Pro 16" 2019):* on macOS AVFoundation rejects encoder settings it dislikes by throwing an Objective-C exception, which is caught. The still-hardware encoder is then asked again without the optional compression properties, and for HEVC up to 4096 px wide as H.264 (the size never changes); the manifest records the codec the file holds. This is not a software fallback. When every try fails the start fails with the AC6 error and the reasons go to the system log. Tested with a width AVFoundation rejects on Apple Silicon; **not confirmed on the reporter's Intel Mac**.
- Scaling happens on the GPU (ScreenCaptureKit output size on macOS; GPU scaler before the encoder on Windows).
- Master bitrate targets per the Recording quality spec (e.g. 2160p30 HEVC 40 Mbps) for the **High** quality; **Balanced** is 0.6× and **Small** is 0.35× of the High target (product decision recorded here Oct 21, 2026, because the Recording quality table only lists High). The settings panel shows estimated disk use per minute (the Confirm dialog already shows it).
- No size, length or quality limit anywhere; no watermark.

**Acceptance criteria**
- **AC1** Given a 3024×1964 display and Source size, then `screen.mp4` is 3024×1964 HEVC.
- **AC2** Given a 5K display (5120×2880) and Source, then H.264 is unavailable and the file is HEVC 5120×2880.
- **AC3** Given 1080p on a 3024×1964 display, then the file is 1662×1080 (even width) with correct aspect ratio.
- **AC4** Given 2160p30 HEVC for 60 s of moving content, then file size is 300 MB ± 30% (target 40 Mbps).
- **AC5** Given any setting, then the output has no watermark and no length limit (a 2-hour test recording completes).
- **AC6** *(added Oct 24, 2026; the software-fallback behaviour that this edge case used to describe, never an AC, is *Withdrawn*, see Edge cases)* Given the platform's hardware encoder rejects the plan, then `start_recording` fails with `platform_rejected` and `platform_code` `hardware_encoder_unavailable`, no bundle is left and no track keeps running; there is no software fallback and no `encoder_fallback` warning.

**Edge cases and errors** — Hardware encoder unavailable for the chosen codec/size: **no software fallback in v0.1** (maintainer decision Oct 24, 2026, "OpenH264 option (c)": the project does not take on the H.264 patent exposure of a source-built OpenH264; every supported Mac has a hardware H.264 encoder through VideoToolbox, Windows has Media Foundation's licensed software encoder, and the bundled LGPL FFmpeg has no software H.264 encoder). Macs without a hardware H.264 encoder (before about 2016, for example the 2013 Mac Pro) are not supported in v0.1. **As built (macOS):** when VideoToolbox rejects the codec or size, or cannot start, the recording fails at the start, before any file is kept and with the other tracks stopped, with `platform_rejected`, `data.platform_code` = `hardware_encoder_unavailable` and the message "This Mac has no usable hardware H.264 encoder for this recording. Try a lower resolution, update to macOS 13 or later, or use a Mac from about 2016 or later." The app shows it translated (message id `hardware_encoder_unavailable`); the CLI and MCP print the English message and the code. Any other error (disk full, permission) is reported as before. The HEVC choice, the width caps and the size rules are unchanged. Tested with fakes (AC6); a real rejection was never forced on hardware. The earlier behaviour (OpenH264 software encoder, an `encoder_fallback` warning, `software_encoder` in the track details) was withdrawn on Oct 24, 2026 and is gone from the code.

**Agent interface** — `start_recording { size: "source"|"2160p"|"1440p"|"1080p"|"720p", codec?: "auto"|"hevc"|"h264", quality?: "high"|"balanced"|"small" }` (defaults from `recording.size`, `recording.codec`, `recording.quality`); `get_settings` shows encoder capabilities (`recording.capabilities`).

**Out of scope** — HDR capture (P2), ProRes masters (P2).

**Test notes** — Capability probe unit tests; S6 benchmark harness reused as a regression test on release candidates.

**Verification** — Verified on macOS hardware: native 3024×1964 HEVC and 1080p (1662×1080, even width). Tested with fakes: size and bitrate math, the 4096 px H.264 refusal. Not verified: AC2 on a real 5K display, AC4 file size at 2160p30, AC5 (2-hour run), a real hardware-encoder rejection (AC6 is tested with fakes only), Windows.

---

### REC-3 Frame rate 30 or 60 fps
**Priority** P0 (30 fps) / P1 (60 fps) · **Status** In progress (30 and 60 fps recording, the health monitor and the daemon-side 60 fps self-test and gate are built and tested with fakes; the setup-panel UI is not built and the real-hardware run is pending) · **Crates** cm-capture, cm-encode · **Depends on** REC-2, NFR-2

**User story** — As a creator, I want 60 fps for smooth demos when my machine can handle it, so that scrolling and animations look fluid.

**Description**
- 30 fps always available. 60 fps is offered after a **10 s self-test** at the chosen size/codec that measures late frames; it runs once per (display, size, codec) combination and when the user asks.
- Self-test pass = ≤ 1% late frames, ≤ 1% dropped frames, no more than 500 ms of capture stalls (below) and CPU < 50%; fail shows "60 fps isn't stable on this machine at 2160p — try 1440p" with a one-click switch.
- Health monitor during recording (every 5 s): if late frames exceed 2% over the last 30 s, the control bar shows a warning dot; the recording continues.
- Recording is constant frame rate: static moments repeat the last frame (as proven in S3). The recorder snaps every frame to a fixed slot grid (frames landing on an occupied slot are coalesced; empty slots repeat the previous frame), so a 60 s file has exactly the expected frame count. Repeats are counted separately from late frames. The ScreenCaptureKit interval is set to 1.1× the frame time (S6 finding, measured on a 120 Hz display only).
- **Status of the self-test (Oct 21, 2026): built in the daemon, tested with fakes; not run on hardware yet. The tray shows the indicator and the HUD shows a Stop-only bar while it runs; the setup-panel flow of AC1 is not built.** `run_fps_selftest { target?, size?, codec? }` records about 10 s of the target at 60 fps on the real sources (screen only: no microphone, camera, system audio or cursor), counts late and dropped frames, **deletes the test video** and stores the verdict. It runs for the user directly and, for an agent, behind the same on-screen confirmation as `start_recording` (it captures real pixels, NFR-11); the dialog says nothing is saved. While it runs it is exclusive with a recording (`busy` both ways), publishes `recording.selftest` events whose first one carries the same `indicator` facts as `recording.started` (so the HUD shows the usual border and state), shows up in `get_recording_status` as `selftest`, and `stop_recording` cancels it (output deleted, nothing stored). CLI: `capturemann record selftest`; MCP tool `run_fps_selftest`.
- **Pass rule:** at most 1% late frames (NFR-2) and at most 1% dropped frames, with at least one frame written, **and at most 500 ms of capture stalls**: a stall is a gap of more than 1 s with no delivery, and a run with more than 500 ms of stalls fails even at 0% late frames. The result carries `stalls` and `stall_ms`. During a real recording the same condition raises the `capture_stalled` warning. The CPU < 50% half of the rule above is **not measured yet** (open). **Stored result:** setting `recording.fps60_selftest` = `{ version: 1, results: { "<display>:<w>x<h>:<codec>": { passed, late_pct, dropped, dropped_pct, frames, at, display, size, codec, app_version, duration_s } } }`, written only by the self-test (`set_settings` refuses the key, so a client cannot forge a pass).
- **The gate:** `start_recording { fps: 60 }` is accepted only if a result for the same display and codec is fresh (at most 30 days old) and from the same app version, and passed at this output size or a larger one (a fail at this size or a smaller one counts against it; the newest applicable run decides). Otherwise it is `invalid_input` before any dialog, with `data.suggestion` = `{ fps: 30, reason, size?, run_selftest: { method, params, cli } }` and these messages: after a fail, "60 fps isn't stable on this machine at 2160p — try 1440p" (`size: "1440p"`; at 720p: "... — record at 30 fps"); otherwise "60 fps hasn't been checked on this machine at 1080p yet — run the 60 fps self-test (10 s) or record at 30 fps" (`reason`: `not_validated`, `stale`, `version_changed`, `settings_changed`). A **default** of 60 coming from the `recording.fps` setting never blocks a recording: it falls back to 30 fps with a `fps60_not_validated` warning until a self-test passes. A restart keeps the settings of the running take and is not re-gated. **The 60 fps verdict is screen-only:** the self-test records no camera, so a 60 fps recording with the camera on starts with the warning `fps60_unverified_with_camera` (S6 saw one failing camera + screen run).
- **Not built:** the setup-panel flow of AC1 (run the test when the user selects 60, then save the setting), the CPU measurement, the "display refresh below 60 Hz" check (the display list has no refresh rate), (the HUD wiring of `recording.selftest` is built). Real-hardware check: `cargo test -p cm-daemon --test real_selftest -- --ignored --nocapture`.
- The health monitor is built: `recording.health` every 5 s with `status: "degraded"` when late frames exceed 2% over the last 30 s, plus `late_frame_pct` (whole recording) and `recent_late_frame_pct` (last 30 s) in the status.

**Acceptance criteria**
- **AC1** Given a fresh install, when the user selects 60 fps, then the self-test runs before the setting is saved. *(Daemon half met by test with fakes: without a pass `start_recording { fps: 60 }` is refused and a default of 60 falls back to 30; the setup-panel flow is not built.)*
- **AC2** Given the self-test fails, then 60 fps stays off and the suggested lower size is offered. *(Met by test with fakes.)*
- **AC3** Given 60 fps recording of 60 s, then the file has 3600 ± 2 frames and reports 60 fps.
- **AC4** Given late frames > 2% during recording, then `recording.health` reports `degraded` and the control bar shows the warning.

**Edge cases and errors** — Display refresh below 60 Hz → 60 fps disabled with the reason.

**Agent interface** — `start_recording { fps: 30|60 }`; if 60 isn't validated, returns `invalid_input` with `data.suggestion`. `get_recording_status` includes `late_frame_pct`.

**Out of scope** — 120 fps, variable frame rate output.

**Test notes** — Synthetic load test; S3 metrics code reused for the self-test.

**Verification** — Verified on macOS hardware (`record_e2e`, Oct 21, 2026, as reported): screen 0.17% to 0.57% late frames at 1080p30 and at 3024×1964 60 fps; S6 measured 0.26% to 0.33% at 3024×1964 and 3840×2160 60 fps without the camera. AC3 (3600 ± 2 frames) is checked by `nfr2_twenty_seconds_at_60fps` (1200 ± 2 frames at 20 s). Not verified: AC1 and AC2 (no self-test), AC4 on a degraded machine, a 60 Hz or true 4K display, worst-case content (full-screen video or game), Windows.

---

### REC-4 Pause, resume and countdown
**Priority** P0 · **Status** In progress (built on macOS; pause accounting verified on hardware, A/V sync after a resume unmeasured; Windows not built) · **Crates** cm-capture, cm-encode, cm-daemon · **Depends on** REC-1

**User story** — As a teacher, I want to pause while I set up the next example, so that my lesson has no dead time and I don't need to edit it out.

**Description**
- Pause stops writing all tracks (screen, camera, mic, system audio, cursor) and records a gap in the session; Resume continues in the same files. The timer shows recorded time only, counted from `live_at_ms` (the moment every track is live). **The media files include a pre-roll before `live_at`** (each track starts at its own offset after the session origin, Technical design → One clock), which the HUD timer does not count, so a file can be a few hundred ms longer than the timer's last reading; `live_at_ms` is exposed in the start result, the status and the manifest so a reader can align them. The HUD shows the paused state (DESIGN §5.2; REC-5).
- **File time.** The screen, camera, mic and system-audio files have continuous timestamps: file time = session time minus the pause gaps before it, so no file has a hole. The session keeps the gap list (`manifest.gaps`, session time in `MediaTime` ticks), and each track repeats it in `details.pauses` (every track, camera, mic and system audio included; audio `gaps` are dropouts only and never hold pauses); together they map file time back to session time, which the editor and exporter must do to align tracks (EDT-1, AUD-4).
- **Documented exception: the cursor log stays in session time.** `cursor.jsonl` events are sparse and stamped by event time, so nothing is made continuous; no event falls inside a gap except the re-anchor `move` and `cursor` lines written at the resume time. A reader maps a cursor event to file time by subtracting the gaps that precede it.
- Gaps are written to `recording.lock` on every resume, so a crash after a pause still recovers with the right mapping (REC-8). Stopping while paused closes the final gap at the stop time, and that final gap is also listed under `pauses` for every track.
- Countdown 3 s before start (`recording.countdown_s`) and 1 s before resume (`recording.resume_countdown_s`, default 1); both 0–10 s. A pause during the start countdown cancels the start (AC4); a pause during the resume countdown cancels the resume and the recording stays paused; a stop during it ends the recording as usual.

**Acceptance criteria**
- **AC1** Given 10 s recorded, 20 s paused, 10 s recorded, then the file is 20 s ± 1 frame plus the pre-roll before `live_at` (see Description) and the timer shows 00:20.
- **AC2** Given a pause and resume, then audio and video stay within NFR-3 (≤ 50 ms) after the resume point.
- **AC3** Given paused state, then nothing on screen during the pause appears in any track.
- **AC4** Given the pause shortcut is pressed during the countdown, then the countdown cancels.
- **AC5** Given a recording with a pause (including one still open when the recording stopped), then `manifest.gaps` lists the gap in session time, each track's `details.pauses` agrees with it, and no media file contains the paused interval (the cursor log has no events inside it except the re-anchor lines).

**Edge cases and errors** — Max 500 pauses per recording (beyond that, show "Too many pauses — stop and start a new recording").

**Agent interface** — `pause_recording` / `resume_recording { recording_id? }` (Free; the recording itself was already confirmed); pause returns `{ state, changed, recorded_ms }`, and pausing twice is a no-op with `changed: false`. Events `recording.paused`, `recording.resumed` (and `recording.countdown` with `context: "resume"`).

**Out of scope** — Retaking the last segment ("rewind and re-record", P2).

**Test notes** — Sync test with the S3 flash/beep video across a pause.

**Verification** — Verified on macOS hardware: pause accounting (2 s + 3 s paused + 2 s is a 4 s file, one gap in the details; `rec4_pause_resume_real_display` and `record_e2e --pause-at`). Tested with fakes: gap math and the cursor exception, the 500-pause limit, countdown cancel and resume-countdown paths. Not verified: AC2 (A/V within 50 ms after a resume; needs the flash/beep method on a finished recording), Windows.

---

### REC-5 Recording indicator and stop control
**Priority** P0 · **Status** In progress (built on macOS: the daemon indicator facts, events and HUD actions (tested with fakes) and the HUD, brackets, outline, countdown and bubble UI in `apps/desktop` (unit tested); not verified on hardware: AC1 latency, AC4 snapping, AC9/AC10 by eye, AC15 meter source; Windows not built) · **Crates** apps/overlay, cm-daemon · **Depends on** DESIGN §5.2 (kit screen `Recording`)

**User story** — As anyone being recorded, I want an always-visible sign that recording is on and a one-click stop, so that I'm never recorded without knowing.

**Description**
- Recording HUD (DESIGN §5.2, kit `Recording`): a floating bar, radius 16, with Stop (`rec` fill, `ink` square), timer (mono, 8 px `rec` dot), divider, Pause, Mic, Webcam, Draw, divider, **Restart**, Discard. Restart is the one deviation from the kit's button order (the kit ends with Discard); it sits immediately left of Discard so the two destructive actions stay together at the end. Draggable, remembers position per display (`recording.hud_positions`), snaps to screen edges. The HUD is a non-activating panel: it never takes keyboard focus when it opens (the recorded app keeps it), so without a click the keyboard route to every HUD action is the global hotkeys (stop ⌥⇧R, pause ⌥⇧P, Discard ⌥⇧D, Restart ⌥⇧T, CAP-2) and Tab does not reach the HUD. A click on the HUD makes it key, after which Tab reaches the buttons (NFR-13 keyboard operation therefore goes through the hotkeys while recording).
- Selected area (Display/Region): the dimmed scrim outside it, the kit's white crop brackets (4 px, radius 6) at the corners, **and** a thin `rec` outline (2 px, 70% opacity) along the selection edge, and the size chips above the top-left corner (`1920 × 1080`, `2× · native`) in Geist Mono. The outline keeps the area visible on busy backgrounds. Window mode outlines the window with the same brackets and outline. All of it is excluded from capture (product maintainer decision OD-3, Oct 21, 2026).
- Paused state: the Pause button becomes Resume, the timer dot stops pulsing and turns `mist`, the timer stops, and the `rec` outline turns dashed (same 2 px width and 70% opacity). The brackets do not change.
- Tray/menu-bar icon shows recording state (CAP-2). With the OS "Reduce motion" setting on, the pulse and ring animations stop: the countdown ring stays full and the timer dot is static (no blink).
- **During the 60 fps self-test** the same indicator and tray show, and the HUD is a Stop-only bar (Stop cancels the test and deletes its output).
- **Discard** (kit) stops the recording and deletes the take after confirmation.
- **Restart** (decision OD-4, Oct 21, 2026) discards the current take and immediately starts a new recording with the same settings (source, region or window, size, fps, codec, camera, mic, system audio) after a new countdown (REC-1 countdown setting; no countdown if it is set to 0). It asks for the same confirmation as Discard ("Discard this take and start over?"). The discarded take is deleted and is not recoverable; the new take is a new recording id. Esc cancels the new countdown (DESIGN §5.3) and ends the attempt with nothing recorded. Per NFR-11, restarting is still starting a recording: a user pressing Restart in the HUD needs no further confirmation beyond the Discard dialog (the user is at the machine and acted directly); an agent that triggers it needs the Confirm-tier dialog of `start_recording` (AGT-2). No MCP tool or CLI command for Restart exists in v0.1; an agent reaches the same result with `stop_recording` and a new `start_recording`, which is Confirm tier.
- **Draw** (kit) is on-screen drawing during recording, specified as the P2 Draft REC-9. In v0.1 the button stays in the HUD, disabled, with the hint "Coming soon" (decision OD-5, Oct 21, 2026).
- **Mic level meter** (decision OD-10): the HUD keeps a small level meter inside the Mic button (five bars). The collapsed dot-and-timer pill is an optional P2 item and is not part of v0.1 acceptance.
- The indicator can't be hidden by settings or by agents (agent safety, NFR-11). When an agent started the recording, an agent toast ("Started by agent via MCP" plus the tool name, DESIGN §4) shows top-right.

**Acceptance criteria**
- **AC1** Given any recording (manual or agent), then the HUD, the area brackets and the tray state are visible within 200 ms of start.
- **AC2** Given the stop shortcut or Stop button, then recording stops and the bundle is closed within 2 s.
- **AC3** Given Discard, when the user confirms, then the take is deleted and recording ends; cancelling keeps recording.
- **AC4** Given the HUD was dragged to a corner, then it snaps to the edge and the next recording opens there.
- **AC5** Given any API or setting, then there is no way to start a recording with the indicator hidden.
- **AC6** Given the HUD, then Stop is the first control, its fill is `rec` and its glyph is `ink`, the buttons are 44 px, and the timer uses Geist Mono 15 (DESIGN §5.2).
- **AC7** Given a recording is running, then the tray icon is the kit's recording asset and is not a template image on macOS (`brand/tray/png/tray-recording-on-*.png`), or the matching `tray-recording-on-{dark,light}.ico` on Windows; when it stops the template icon returns.
- **AC8** Given the app is built for release, then the bundle carries the kit app icon (macOS `AppIcon.icns`, Windows `app.ico`) and the recording HUD has no dependency on a blurred background (DESIGN §4 native caveat).
- **AC9** Given a Display or Region recording, then the selection shows the four white crop brackets and a continuous 2 px `rec` outline at 70% opacity, and neither appears in `screen.mp4`.
- **AC10** Given the recording is paused, then the outline is dashed (2 px, 70% opacity) and the HUD shows Resume and a `mist` timer dot; when resumed, the outline is continuous again.
- **AC11** Given the HUD, then the button order is Stop, timer, Pause, Mic, Webcam, Draw, Restart, Discard, with Restart immediately left of Discard.
- **AC12** Given the user presses Restart and confirms, then the current take is deleted, a new countdown runs, and a new recording starts with the same settings and a new recording id; cancelling the dialog keeps the current recording running.
- **AC13** Given an agent stops a recording and calls `start_recording` again (the agent path to a restart), then the Confirm dialog shows and nothing starts until the user accepts.
- **AC14** Given the HUD, then the Draw button is disabled with the hint "Coming soon", and activating it does nothing.
- **AC15** Given the HUD, then the Mic button shows a level meter that follows the live mic input.

**Edge cases and errors** — Full-screen apps and presentations: the control bar stays above them (collapsible), on macOS via the overlay window level.

**Agent interface** — None to hide or move the indicator (by design). No Restart tool in v0.1; see Restart above for the Confirm-tier rule.

**Daemon contract for the HUD** (built; first-party clients only). The HUD is a client of the same bus and does not read engine state directly:
- `recording.started` and `get_recording_status` carry an `indicator` object: `area` (`kind` display, region or window; `display_id`, `window_id`, `bounds` as `[x, y, w, h]` physical px in the global desktop space, `source_size`, `scale`), `output` (width, height, fps, codec), `camera` (bubble `shape`, `size`, `position`, `mirror`, or null), `mic`, `system_audio`, `cursor`, and `started_by` (`kind` user or agent, `client`, `tool`). There is no action that hides or changes it.
- Internal actions (no MCP tool, no CLI command; `INTERNAL_ACTIONS`, AGT-1.AC4): `discard_recording` (Discard), `restart_recording` (Restart) and `skip_countdown` (Space during the countdown). Pause, Resume and Stop are the public `pause_recording`, `resume_recording` and `stop_recording`. In process as the user they run directly (the HUD asks "Discard this take?" itself); over the socket as an agent, discard and restart are Confirm tier.
- Restart discards the take (`recording.discarded { reason: "restart" }`), keeps the job holds across the gap so exports do not flap, and starts a new run with a new `recording_id` and `restart_of` set to the old one.
- Live numbers arrive in `recording.stats` (1 Hz) and `recording.countdown` (1 per second). The mic meter source is `mic_level_dbfs` (peak) and `mic_rms_dbfs`, produced at about 30 Hz in `cm-audio`; the 1 Hz event is too slow for AC15, so the HUD needs polling `get_recording_status` or a faster path. Open for the HUD work.

**Out of scope** — Custom indicator styles; on-screen drawing during recording (P2, REC-9); the collapsed dot-and-timer pill (optional P2); an MCP tool or CLI command for Restart.

**Test notes** — Manual matrix over full-screen apps, PowerPoint/Keynote presenting, multiple Spaces/virtual desktops.

---

### REC-6 Excluded windows
**Priority** P0 · **Status** In progress (macOS exclusion built and tested with a real filter; the library-window exception (AC8) is tested with fakes, not on hardware; the default list is unverified and exclusion of a late-launched app is not verified on hardware; screenshots exclude own windows on macOS 14+; Windows not built) · **Crates** cm-capture, cm-daemon · **Depends on** REC-1

**User story** — As a professional, I want password managers and private apps never to appear in my recordings, so that I don't leak secrets by accident.

**Description**
- Always excluded: CaptureMann's recorder windows (HUD, countdown, area indicator, webcam bubble, start sheet, toasts, confirmation dialogs, launcher). The library window (Studio, including the editor and the setup and permissions screens, which are the same window) is **not** excluded: a recording and a screenshot show everything visible on screen except the recorder UI.
- Default exclusion list (editable in Settings → Privacy): 1Password, Bitwarden, KeePassXC, Dashlane, macOS Passwords app, Windows Credential Manager dialogs. Matching by bundle id (macOS) or executable name (Windows).
- **As built (macOS).** The built-in list is these bundle ids: `com.1password.1password`, `com.agilebits.onepassword7`, `com.bitwarden.desktop`, `org.keepassxc.keepassxc`, `com.dashlane.dashlanephonefinal`, `com.apple.Passwords`. **To be verified:** they were written from memory and have not been checked against the installed apps. The Windows entries do not exist yet. The list is the union of the built-in ids, the setting `privacy.excluded_apps` (an array of bundle ids or executable names), and `exclude.bundle_ids` in the request; a request can add, never remove.
- Exclusion is by application: every window of the app is hidden, including windows opened later. A window id given in `exclude.window_ids` excludes the window's owning process, because the ScreenCaptureKit display filter cannot mix "excluded applications" and "excluded windows"; windows are never excluded one by one on macOS. CaptureMann's own process is always excluded this way (by pid, in recordings), and the HUD, bubble, countdown, toast, start-sheet, confirmation and launcher windows also set `sharingType = none` (S6: use both). **The library window is the one exception.** The host passes its window id (`EngineOptions::kept_windows`, `DaemonBuilder::kept_windows`; the desktop app answers the Studio window's `NSWindow.windowNumber` while it is visible, nothing else); the screen track builds the filter with `initWithDisplay:excludingApplications:exceptingWindows:` and those ids as `exceptingWindows`, which ScreenCaptureKit documents as "Any windows that are an exception to the filter will be shown if their owning application is in the provided list". The ids are asked again by the 1 s watcher, so opening, closing or minimizing the library window mid-recording changes the filter within about a second. An id ScreenCaptureKit does not list yet stays excluded (fail-safe: recorder UI never leaks), so a library window opened mid-recording may be missing for up to about 1.5 s. Without a provider (the standalone `cm-daemon`, agent-only setups) behavior is unchanged: pure pid exclusion. The exclusion list is refreshed while recording: a 1 s watcher adds an excluded app that launches after the start (AC2), except for a **Window** target, which ignores exclusions because it captures one window only.
- Users can add any running app; excluded windows render as fully transparent (desktop shows through) on macOS (ScreenCaptureKit exclusion) and as black on Windows (`WDA_EXCLUDEFROMCAPTURE`).
- Exclusion applies to screenshots (CAP-1) too: on macOS 14+ a screenshot is a ScreenCaptureKit single-frame capture that excludes CaptureMann's own windows (except the library window, as in recordings) and the excluded apps; on macOS 13 it falls back to `xcap`, which cannot exclude windows (known gap).

**Acceptance criteria**
- **AC1** Given 1Password is open on screen, when recording or capturing, then its windows don't appear (macOS: transparent; Windows: black).
- **AC2** Given the user adds Slack to the list, when recording, then Slack windows are excluded from that moment, including windows opened later.
- **AC3** Given the list, then CaptureMann's own windows can't be removed from it.
- **AC8** Given the library window is visible on top of another app, when a display or region recording or screenshot runs, then the library window is in the video or image, and the HUD, countdown, area indicator, webcam bubble, start sheet, toasts, confirmation dialogs and launcher are not. Given the library window is closed or minimized, then nothing of CaptureMann is in the capture. A window id that is not in the keep set stays excluded.

**Edge cases and errors** — Windows can only exclude windows of processes that allow display affinity; for others, Settings marks the app "Can't exclude on Windows" and the recording warns at start.

**Agent interface** — `get_settings`/`set_settings` keys `privacy.excluded_apps`. Agents may add exclusions but not remove the defaults (Never tier for removals of defaults).

**Out of scope** — Blurring windows instead of hiding them (P2).

**Test notes** — Manual matrix with each default app on both OSes.

**Verification** — AC8 is tested with fakes: `filter_spec` with keep windows, the watcher firing when a kept window appears or disappears (`record/exclude.rs`), the engine passing the provider into the plan (`rec6_ac8_*`), the screenshot backend call carrying the keep set, and the desktop provider returning only the library window (`studio_window`). Not verified on hardware: the manual steps in `tests/manual/REC-1.md` and `REC-5.md`. Tested on the real display: a filter with a pid, a bundle id and an unknown window id starts and records (`rec6_exclusions_build_a_valid_filter`); the union and no-removal rules are tested with fakes. Not verified: AC1 with a real password manager, AC2 on hardware (a password manager launched after the recording started; the watcher is tested with fakes only), the bundle ids above, Windows.

---

### REC-7 Cursor telemetry saved with every recording
**Priority** P0 · **Status** In progress (macOS built; AC1 and AC2 accuracy unmeasured; Windows code written, never run) · **Crates** cm-cursor · **Depends on** REC-1, Spike S4

**User story** — As a creator, I want my cursor movements and clicks recorded separately, so that I can enlarge, smooth or auto-zoom on them later without re-recording.

**Description**
- During recording, sample the cursor position at 120 Hz (only on change), every mouse button down/up, scroll events (direction only), and the cursor image + hotspot whenever its shape changes.
- Stored as `cursor.jsonl` (one event per line, `MediaTime` ticks) and `cursors/<id>.png` with an `index.json`. Line 1 is a session header (`type: "session"`, schema, tick rate, clock, platform, the target mapping, `clicks` and `scroll` availability, `move_source`, `permission_missing`); every later line is an event. Image ids are stable: `c_` plus 12 hex digits of a SHA-256 over the pixels, size, hotspot and scale, so the same shape has the same id across runs and recordings.
- **Coordinates** are integer **physical px relative to the top-left of the recorded area** (display, region or window), y down, so they equal pixel coordinates in `screen.mp4`; the hotspot position is reported. A cursor outside the area keeps its out-of-range coordinates and carries `outside: true` (also for another display). A window target is fixed to the window's rectangle at start; window moves are not tracked in v0.1.
- Cursor events are in **session time**, not file time: after a pause they must be mapped with `manifest.gaps` (REC-4).
- In v0.1 the cursor is also baked into the screen track (default); "Record cursor separately" (P1, EDT-4; built Oct 22, 2026 as the `recording.cursor_separate` setting, `start_recording { cursor_separate }` and `record start --cursor-separate`, with the desktop start sheet toggle not built) hides it from the video so it can be redrawn.
- **Keystrokes are never recorded.** Modifier keys held during clicks are recorded as flags only (shift, ctrl/cmd, alt) to support click effects.
- macOS needs **Input Monitoring** for click and scroll events: the listen-only event tap is gated by Input Monitoring. **Accessibility is not needed** for recording; it is only needed to post synthetic events (the injection test). Without Input Monitoring, positions still record by polling at 120 Hz, clicks and scroll are marked unavailable, the session carries a `permission_missing(input_monitoring)` warning, and the header says `clicks: "unavailable"`, `move_source: "poll"`. Windows uses a low-level mouse hook and needs no permission (code written, never run).

**Example line**
```json
{"t": 1834000, "type": "move", "x": 1210, "y": 644}
{"t": 1842000, "type": "down", "button": "left", "x": 1212, "y": 646, "mods": ["shift"]}
{"t": 1842000, "type": "cursor", "id": "c_9f2a", "hotspot": [3, 2]}
```

**Acceptance criteria**
- **AC1** Given a 10-minute recording, then the redrawn cursor (from telemetry) lands within one frame and 2 physical px of the baked cursor in sampled frames (S4 criterion).
- **AC2** Given 20 clicks during recording, then 20 `down` and 20 `up` events exist.
- **AC3** Given the user typed text during recording, then no key characters exist anywhere in the bundle.
- **AC4** Given Input Monitoring permission is denied on macOS, then the recording still succeeds with moves only, `cursor.jsonl`'s header marks `clicks: "unavailable"`, and the bundle's `session.json` marks `cursor.clicks: "unavailable"`.

**Edge cases and errors** — Cursor outside the recorded region → events still recorded with out-of-range coordinates (used for zoom-out decisions), flagged `outside: true`. A torn last line after a crash is reported as truncated and skipped; at most about 1 s of events is lost.

**Agent interface** — MCP resource `capturemann://capture/{id}/cursor`; tool `get_cursor_track { capture_id, from?, to? }` → events in video coordinates (Free).

**Out of scope** — Keystroke overlay display (P2, would need explicit opt-in and its own spec).

**Test notes** — S4 harness as regression; privacy test scanning the bundle for typed strings.

**Verification** — Tested with fakes: writer, reader, pause handling and the permission fallback. Verified on macOS hardware: the real event tap, cursor images and CPU (a few percent while moving; 0.36% idle in S4), and the denied-path polling fallback (`rec7_*` ignored tests). Not verified: AC1 (timestamp accuracy and the 10-minute baked-cursor comparison) and AC2 (click completeness) because the injection test needs `CM_ALLOW_INPUT_INJECTION=1` plus Accessibility and Input Monitoring; AC3 by a typed-string scan on a real recording; Windows. See `tests/manual/REC-7.md`.

---

### REC-8 Crash-safe recording
**Priority** P0 · **Status** In progress (macOS: kill -9 recovery verified on hardware; the disk guard, maintainer lock, quarantine and audio recovery tested with fakes; Windows and the CI kill test not built) · **Crates** cm-encode, cm-library, cm-daemon · **Depends on** REC-1, NFR-6

**User story** — As a teacher recording a 45-minute lesson, I want a crash or power loss to cost me seconds, not the whole recording.

**Description**
- All media tracks are fragmented MP4 (2 s fragments) or WAV with headers rewritten every 2 s; telemetry is append-only JSONL.
- A `recording.lock` file in the bundle marks an incomplete bundle: its presence means the recording did not finish. It holds what recovery needs to rebuild the manifest (id, title, creation time, the settings as used, the owning process id, the pause gaps so far, warnings, `live_at`). It is removed only after `manifest.json` is written. A bundle owned by a live process is left alone. "Owned by a live process" is decided by an advisory lock on a `recording.maintainer` file in the bundle (the kernel drops it when the process dies), not by a pid, so a reused pid cannot keep a dead recording from being recovered.
- On launch, any bundle with a lock file whose maintainer is gone is recovered: tracks are truncated to the last complete fragment, durations fixed, a manifest is written (`revisions[0].action` is `recover`, `recovered: { at, lost_ms_max }`), lock removed, and the user sees "Recovered recording from `<time>` (lost ≤ 2 s)". Stale locks next to a finished manifest are simply removed. A bundle with nothing usable is **quarantined**: its lock is marked `recovery_failed` with the reason, later launches skip it and list it, and it is never deleted. Audio tracks are rebuilt from hints in the lock (start offsets, the denoised file's name) and repaired with `repair_wav`, so a crashed mic recording keeps its `start` and its denoised twin.
- Expected loss is at most one fragment, 2 s, per track. The first fragment of a track is shorter than 2 s by that track's start offset (tracks start 30 to 500 ms after the session origin, see Technical design → One clock). Telemetry is cut at its last whole line; WAV headers are patched about once per second, so a crashed WAV is at most about 1 s stale and is repaired from the file length.
- `manifest.json` records `stop_reason` (`user`, `max_duration`, `disk_full`, `screen_halted`, `screen_encoder_failed`, `shutdown`, `engine_failed`, `no_frames`, `engine_dropped`, `error`) and `started_by`; a recovered bundle has `recovered` instead.
- Disk-space guard (NFR-7): the estimate assumes 1.25× the target bitrate (encoders overshoot). Below 10 minutes of space the plan and the Confirm dialog show a `disk_low` warning and a `disk_space` row **before** recording; during recording `recording.warning { code: "disk_low" }` repeats it. **Below 1 GB free a start is refused with `disk_full`**, and a running recording stops cleanly at 1 GB. The segment writer's queue is bounded (10 segments of 2 s); if the disk cannot keep up for 5 s the writer fails the track with `disk_slow` instead of growing memory.
- **Stop.** The stop timeout is no longer a hard limit: when finishing the hardware encoder takes longer, the engine stays `busy` in the `stopping` phase, and the late result is still published (`recording.stopped`) when it arrives. WAV files use an 80-byte header with a `JUNK` chunk so the writer can switch to **RF64** past 4 GiB; a track that stops being writable is kept and truncated (`details.truncated_at`), never deleted; telemetry and media writers `sync_data` at each fragment or header patch. A recording that produced no video frame stops with reason `no_frames`; a capture session the OS drops ends with `engine_dropped`.

**Acceptance criteria**
- **AC1** Given a recording is running, when the process is killed (`kill -9` / Task Manager), then on next launch the recording opens with ≥ (elapsed − 2 s) of media in every track.
- **AC2** Given the recovered bundle, then tracks stay within NFR-3 sync.
- **AC3** Given free space drops below 1 GB, then recording stops cleanly with a toast and the file is valid.
- **AC4** Given free space is below the 10-minute threshold at start, then the plan and the Confirm dialog warn before recording (`disk_low`, `disk_space` row); the setup panel does not show it yet. Below 1 GB free the start is refused with `disk_full`.

**Edge cases and errors** — Recovery fails for one track → keep the others, mark the track missing in the manifest, tell the user which.

**Agent interface** — Event `recording.recovered { capture_id, lost_ms, title, duration_s, created, missing_tracks }` (one per recovered bundle, published at daemon start, followed by `capture.created`; `lost_ms` is the upper bound, 2000); `get_recording_status` includes `disk_left_minutes`.

**Out of scope** — Recovering after disk corruption.

**Test notes** — Automated kill test in CI (macOS and Windows runners) at random times; disk-full test with a small RAM disk / VHD.

**Verification** — Verified on macOS hardware: AC1 (a child process killed with SIGKILL during recording recovers with at least elapsed minus 2 s of media; measured loss up to 1.32 s as reported from `record_e2e --kill9-test`, and `rec8_ac1_kill9_then_recover`). Tested with fakes: truncated-fixture recovery, a missing track kept out of the media list with `missing: true`, gap mapping after a pause, the disk guard (AC3, AC4) and the lock life cycle. Not verified: AC2 (sync of a recovered bundle), AC3 on a real small disk (manual check 4 in `tests/manual/REC-1.md`), Windows, the CI random-time kill test.

---

### REC-9 Draw on screen while recording
**Priority** P2 · **Status** Draft (not in 0.2.0; detail before work starts) · **Crates** apps/overlay, cm-capture, cm-render

**User story** — As a teacher, I want to draw arrows and circles on my screen while I record, so that viewers can follow what I'm pointing at without me editing it in afterwards.

**Summary** — The HUD's Draw button (kit `Recording`) turns on a transparent drawing layer over the recorded area with a pen, highlighter and arrow in the annotation palette (ANN-1 colors). Strokes fade out after a few seconds or clear with a shortcut. Drawing appears in the recording; the layer window and its toolbar are excluded from capture except the strokes themselves, which are stored as timed vector events so they can be hidden or restyled in the editor. In v0.1 the button exists in the HUD disabled, with the hint "Coming soon" (REC-5).

**Key criteria (to refine)** — Toggling Draw does not interrupt capture or drop frames (NFR-2); pointer events pass through when Draw is off; strokes are in the export and match the live view; stroke events are in the bundle with `MediaTime` stamps; works over full-screen apps; agent interface (a `draw_on_screen` style tool and its tier) to be defined before work starts.
