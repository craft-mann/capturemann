# Webcam & audio page: camera and audio check (board R13b)

Status: **Built Oct 25, 2026** (commits `b2d8b10`, the page merge and `2f0b3cb`); tested with fakes; the maintainer checked the camera, microphone and system audio test on their Mac and reported them good (as reported, not stored in `verification/`). Not yet checked by eye: unplug, camera in use, permission blocks, narrow layout, Vietnamese, VoiceOver (`tests/manual/DEVICE-CHECK.md`). Thresholds are untuned guesses. Plan revised the same day. Inputs: the design spec `R13-webcam-audio-spec.md` and the mockup `NewDeviceCheck` (board R13b), removed from the repo on Oct 25, 2026 once built; this file, `specs/features/04-webcam.md` and `tests/manual/DEVICE-CHECK.md` are now the record. Replaces the layout built earlier on Oct 25 (`tests/manual/CAM-DEFAULTS.md`). Every existing setting keeps its key and behaviour; new are the mic level meter and the system audio check. "Hear yourself" (test recording and playback) is **dropped**.

## Maintainer decisions (Oct 25, 2026)

- **Camera: off by default.** Opening the page does not start it; the card shows the "Camera is off" block and the user presses "Turn on camera". It stops when they leave, and "Turn off camera" stops it earlier.
- **Microphone: off by default, like the camera.** The sub-section opens on "Microphone is off" with a "Test microphone" button; pressing it starts the meter. It stops when they leave, and a "Turn off microphone" button stops it earlier. Opening the page opens no device.
- No Hear yourself, no test clip, nothing recorded on this page.
- The system audio chime is played by a separate process (`/usr/bin/afplay` on a temp WAV), because CaptureMann's own process audio is always excluded from system capture (`cm-audio/src/system.rs`).
- The spec wins over the mockup where they differ (see below).

## Rules that follow from auto-start

- **Permission first.** Nothing is asked by opening the page. Permission not granted shows the "No permission" block (after the user presses Turn on camera or Test microphone; system audio after Play test sound) with one button, **"Allow access"**, which goes to CaptureMann's own Settings → Permissions page, where the grant is requested or the System Settings pane is opened (that page already does both). Back on this page the check starts again by itself once the permission is there.
- **Stops, not only on leaving:** the page is left, the app window is not the active window (existing rule for the camera preview, now also for the mic), Turn off camera or Turn off microphone, or a recording starts. While a recording or the Record sheet's bubble preview holds the device, the card shows a plain note ("Recording in progress, the check is paused") instead of "In use", and resumes when it ends.
- **System audio is never automatic.** It needs Screen Recording and shows the purple capture indicator, so it runs only for "Play test sound" (about 2 s), then stops.
- **The orange mic dot and the camera light** show only after the user starts that check. The footnotes say so ("the camera turns off when you leave this page", "the mic turns off when you leave").

## What exists

| Need | Code |
|---|---|
| Live camera, start/stop rules, mirror | `apps/desktop/src/ui/settings/webcam_view.rs`, `webcam_model.rs` (`LiveInputs`, `preview::start`); today it starts only on a toggle, so the toggle becomes automatic start plus Turn off |
| Meter-only mic stream, levels, clip events | `cm_audio::MicMonitor`, `LevelHandle`, `Level` |
| System audio capture | `cm_audio::system::SystemAudioRecorder` (ScreenCaptureKit; excludes this process) |
| Noise suppression switch | `ui/settings/recording.rs` (`KEY_NOISE`); moves here |

## Behaviour to build

**Camera card.** Starts only on Turn on camera (a camera that is off is the default state). On: Live chip, real size and measured fps (`1920 × 1080 · 30 fps`), 16:9 feed, "Mirrored" tag when Mirror is on, device dropdown, Turn off camera, footnote. Changing the dropdown switches the feed within about 1 s. Blocks: Off (after Turn off), No permission, In use, Not found. Unplugging mid-preview shows Not found, no crash; Look again recovers.

**Audio check card** (one card, two sub-sections divided by a line, each with its own chip):
- *Microphone:* off by default (block + Test microphone; Turn off microphone next to the dropdown while running). Running: heading and verdict chip, device dropdown, prompt "Speak as you would while recording", 32-segment meter with the good-range band, peak hold, 18 px dB readout. Verdict from the peaks of the last 2 s: Too quiet below the band, Good level inside about −30 to −8 dBFS, Too loud above it (thresholds tuned on hardware). A clip event shows the Clipping copy (Sound settings button) and keeps the meter. Silent (peak below −60 dBFS for 2 s) and No permission replace only this sub-section. Dropdown change restarts the monitor within about 1 s.
- *System audio:* heading and status chip ("Heard" once detected), a 16-segment meter and "Play test sound". Idle until pressed. Not heard and No permission replace only this sub-section (Open Permissions).
- Card footnote: "Level only. Nothing is recorded; the mic turns off when you leave."

**Settings.** Webcam bubble card: Shape, Size, Mirror the preview, Remember where I put the bubble (icon `pin`); Position row gone, its hint under this card. Microphone card: Noise suppression (moved from the Recording page; same key `audio.noise_suppression`). Camera and Microphone dropdowns live in the check cards (same keys `camera.device`, `audio.mic_device`).

## Spec vs mockup, and spec lines to fix

- The mockup draws the Position hint under the Microphone column; the spec puts it under the Webcam bubble card. We follow the spec.
- Mockup tag "Mirrored, like the bubble"; spec "Mirrored". We use the mockup text, only when Mirror is on.
- **Spec/mockup to update (maintainer):** the spec's "Audio (mic)" rows need a **Microphone is off** block (like Camera, Off) with Test microphone, and a Turn off microphone button in the running state; the failure table's action "Open Permissions" becomes "Allow access" for camera, mic and system audio; the footnote "the mic turns off when you leave" stays. The first acceptance check ("Opening the page doesn't turn on the camera or mic") is now correct as written.
- Mockup has no mic Off block, Test microphone or Turn off microphone: we add them in the camera card's style.
- The mockup shows the camera Live; the real page opens on the Camera is off block. The mic shows Microphone is off, then Starting and its level after Test microphone; the system chip starts empty.

## Work packages

1. **Spike D0 (first, short; replaced by `cargo run -p cm-audio --example chime_probe`, which the maintainer ran through the page).** On the maintainer's Mac with the bundled app: play a chime with `afplay` while `SystemAudioRecorder` runs; confirm it is heard with built-in speakers and with headphones or Bluetooth, and that a muted output reads as silent. Record the result here. If it fails, the system audio sub-section ships as "Not available" and we revisit.
2. **A. Page and camera** (`webcam_view.rs`, `webcam_model.rs`, `widgets.rs`): new layout, auto-start rules, camera states, fps/size readout (measure from frame arrival if the preview does not report them; add a busy-vs-missing distinction in `cm-camera` if the error does not carry it), settings cards, Position hint.
3. **B. Audio check** (new `ui/settings/audio_check.rs` plus a pure model, and `cm-audio/src/chime.rs`): mic meter mapping, peak hold, verdicts, states, `MicMonitor` lifecycle; chime WAV, `afplay` child, capture, verdict, cleanup.
4. **E. Cleanup and docs:** remove the Noise row from `recording.rs`; strings through `tr!` with Vietnamese; width tests; update `docs/project/plan.md`, `docs/user/user-guide.md`, `docs/user/macos-permissions.md`, `specs/features/04-webcam.md` and `05-audio.md`, `CHANGELOG.md`; manual script `tests/manual/DEVICE-CHECK.md` replacing `CAM-DEFAULTS.md`.

Order: D0, then A in parallel with B (worktree agents, disjoint files; A owns the camera card and page layout, B owns the audio card module and exposes one view function), then E. I merge and run fmt, clippy, full tests, `cargo deny`, `cm-i18n check`.

## Tests (fakes unless noted)

- Opening the page starts neither camera nor mic; Turn on camera and Test microphone start them. Without permission, pressing them starts nothing and shows the block; Allow access routes to Settings → Permissions (fakes count opens and prompts).
- Leaving the page, losing the window, Turn off camera and a recording starting each close the streams; they resume as specified.
- Segment mapping, peak hold, the 2 s verdict windows, Silent and Clipping rules.
- Chime: temp file removed in every outcome; `afplay` missing or failing gives Not heard, not a crash; system capture runs only during the check.
- State machine for each failure block; action buttons route to Permissions or the Sound pane.
- Every existing setting still saves and loads (round trip).
- English and Vietnamese width tests for the new rows and chips.
- By eye and on hardware (maintainer): real unplug, camera light, orange mic dot, chime on speakers and headphones, VoiceOver.

## Out of scope

Windows paths, per-app audio, a recording-time mic gain control, any change to how recordings pick devices.
