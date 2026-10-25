# Webcam (CAM)

The camera is recorded as its own track (`camera.mp4`). During recording the user sees a live bubble window that is **excluded from the screen capture**; the bubble is drawn into exports by `cm-render` from the camera track and the layout in the edit list. This is what makes the bubble editable after recording. Spike S6 confirmed window exclusion on macOS and the camera plus 4K60 screen load (conditional Go; the camera/screen sync of 50 ms is still unproven, see CAM-2).

**Build status (Oct 21, 2026, macOS only).** `cm-camera` is built: enumeration, permission state, a live preview, and the camera recorder (`camera.mp4`), wired into the recording session. The bubble window (live camera, 4 px `rec` ring, drag with snapping, hover toolbar with size, shape, mirror and hide) is built in `apps/desktop` and unit tested; it has no stored hardware verification. The bubble in exports comes from the interim `quick` exporter (a round bubble, layout at stop only); the editable layout and the editor are not built. Windows is not built (Media Foundation is not started).

### CAM-1 Webcam bubble while recording
**Priority** P0 · **Status** In progress (built on macOS: camera track, bubble facts, the bubble window and its toolbar, the interim `quick` export of a round bubble, the `camera.connected`/`camera.disconnected` events and the Record sheet's list refresh (tested with fakes, never run with a real unplug); unverified on hardware: AC1 preview latency, AC2, AC6, AC7 by eye; not built: layout changes in the edit list (AC3, AC4), exports that match the editor preview (AC5), the editor inspector; Windows not built) · **Crates** cm-camera, apps/overlay, cm-render · **Depends on** REC-1, DESIGN §5.4

**User story** — As an online teacher, I want my face in a small floating bubble while I record my screen, so that students see me while I explain, like in Loom.

**Description**
- Turn the camera on in the recording setup panel (or with the Camera toggle on the control bar, which also works mid-recording).
- Bubble shapes: **circle** (default) or **rounded rectangle** (4:3, radius 24 px); **Off** hides it (see hover toolbar). The ring is 4 px `rec` (kit `Recording`), part of the bubble's outer size, and is drawn into exports. The earlier 3 px white ring is retired. **No shadow and no backdrop** (accepted deviation from the kit's drop shadow, Oct 21, 2026): the bubble is only the camera shape and its ring, with no rectangular fill, glass or window shadow; its transparent corners are click-through.
- Sizes: three presets, **S 160, M 240 (default), L 320 px** diameter/width, logical px (product owner decision OD-1, Oct 21, 2026). The kit's inspector slider (8–40%) is not built in v0.1. Size is stored as a fraction of the output frame so a slider can be added later without a format change.
- Position: **every recording starts with the bubble in the bottom-left corner** of the recorded area (24 px margin; stored as center fractions of the area), in the setup preview too (product owner decision, Oct 21, 2026, replacing the earlier bottom-right default). The user can drag it anywhere, before or during the recording; it snaps to the 4 corners and edge midpoints with a 24 px margin only when released within 32 px of one, and always stays inside the recorded area. The dragged position is stored in `camera.position` but is **ignored by the next recording** unless the setting `camera.remember_position` (bool, default `false`; Settings → Webcam & audio, "Remember where I put the bubble") is on. The position at stop is what the quick export uses.
- Toolbar (board R18, built Oct 24, 2026, tested with pure-logic tests, not seen by eye: a pill above the bubble in a taller window, flipped below when less than 46 px is free above; size S/M/L, circle/rounded, mirror, hide; manual steps `tests/manual/REC-5.md` section 4b): size, shape, mirror, hide. Hiding mid-recording records a layout change (bubble hidden from that time) without stopping the camera track.
- The bubble's position and size are stored in the edit list as fractions of the recorded area at each change, so exports reproduce exactly what the user saw.
- The bubble is excluded from the screen track (ScreenCaptureKit exclusion; `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` on Windows).
- Camera-only mode: a "Camera full" button in the bubble toolbar records a layout segment where the camera fills the frame (screen still recorded underneath).
- **Who owns the camera (decision, Oct 21, 2026).** The daemon owns the single capture session per camera (`AVCaptureSession` on macOS). The recorder and the preview are two outputs on that one session, so the device is never opened twice. The bubble shows **small RGBA frames** from the daemon (640 px long edge by default, 15 fps, about 0.6 MB per frame, mirror applied for display only), sent over the local socket or shared memory and uploaded as a texture by the overlay. Rejected: looking up a shared `IOSurface` by id across processes (camera surfaces are not global; a transfer needs a Mach port or XPC service, too much machinery for a 240 px bubble) and letting the overlay open its own session (some external, virtual and Continuity cameras are single-client or silently lower the resolution or frame rate for the first client, which would damage `camera.mp4`, and it would show different frames than the ones recorded). The desktop app, which embeds the daemon, uses the in-process `CameraPreview` and may take frames zero-copy through `PreviewFrame::io_surface_id`.
- The bubble facts of the request (`shape`, `size`, `position`, `mirror`) are stored in the manifest settings and handed to the HUD in `recording.started` (`indicator.camera`); the engine does not draw the bubble.

**Acceptance criteria**
- **AC1** Given camera on, when recording starts, then `camera.mp4` exists and the bubble shows the live camera with < 100 ms preview latency. (Measured delivery latency of the camera pipeline alone is 60 to 75 ms on the built-in camera, so the preview budget is tight; the preview path through the overlay is unmeasured.)
- **AC2** Given the bubble is visible during recording, then `screen.mp4` doesn't contain it.
- **AC3** Given the user drags the bubble at 00:30, then the edit list has a layout change at 00:30 and the export shows the bubble moving at 00:30 (± 1 frame).
- **AC4** Given the user hides the bubble at 01:00 and shows it at 01:20, then the export has no bubble between 01:00 and 01:20 and the camera track still covers that time.
- **AC5** Given a circle bubble of size M during recording, then in every export the bubble's diameter and position, as fractions of the frame, equal its diameter and position as fractions of the recorded area during recording (± 1 px at the export size), and the export matches the editor preview frame for frame.
- **AC6** Given the bubble is released within 32 px of a corner or edge midpoint (for example the bottom-right corner), then it snaps to 24 px margins; released anywhere else, it stays exactly where it was dropped.
- **AC7** Given the bubble is visible (live or in the editor preview), then it has a 4 px ring in `rec` and no white ring (DESIGN §5.4); the editor inspector offers Circle / Rounded / Off.
- **AC8** Given any new recording (including the setup preview in the start sheet and a start without a `position`), then the bubble starts in the bottom-left corner with a 24 px margin, wherever it was dragged last time, unless `camera.remember_position` is on; then it starts at the last dragged position. The position at stop (default or dragged) is the one the quick export uses.

**Edge cases and errors**
- Window exclusion unsupported (old Windows build) → fall back to "bubble baked in": the bubble is part of the screen track; warn before recording that it won't be editable later.
- Camera disconnected mid-recording → bubble shows a camera-off placeholder; camera track gets a gap; toast; recording continues.
- Camera busy (used by Zoom/Teams) → `device_busy` at start, with the option to record without camera. Cameras the OS shares record normally.
- **Start timing.** Starting the camera blocks until its first frame, up to 3 s; if none arrives the camera is reported busy or dead and the recording starts without it, with a warning. Because all tracks start together, a slow camera shows up as pre-roll before `live_at_ms` (REC-1), not as a delay for the other tracks.
- **Permission.** The camera permission can be `NotDetermined`, `Authorized`, `Denied` or `Restricted`. Checking never prompts. A start with `NotDetermined` fails with `permission_missing` (`camera`); the **UI must request access before starting** (`request_camera_access`, from a bundle that has `NSCameraUsageDescription`) and then start.
- Reconnect after an unplug is not built on macOS yet: the file stays valid, the track gets a gap and a `camera_disconnected` warning, and no `camera_restored` is raised.

**Agent interface** — `start_recording { camera: { on: true, device_id?, shape?: "circle"|"rounded", size?: "s"|"m"|"l", position?: "bottom-left" (default)|"bottom-right"|… | [x, y] } }`. `set_camera_layout` after recording (CAM-3). Events `camera.connected` and `camera.disconnected` (`{ device_id, name, kind, is_default }`) are emitted by the daemon's camera watcher while idle and while recording (built Oct 23, 2026, tested with a scripted watcher; macOS polls the AVFoundation device list every 2 s because the headless daemon has no run loop for the connect notifications, never run with a real USB camera: `tests/manual/CAM-1.md`); a lost camera during a recording additionally raises the `recording.warning` code `camera_lost` and the session continues. If the OS removes camera access mid-recording the camera track gets a gap and a `recording.warning` `permission_revoked` (`permission: "camera"`); the screen recording continues (PRM-1).

**Out of scope** — Multiple cameras at once; green screen.

**Test notes** — Automated export-vs-preview frame comparison (AC5); manual matrix with built-in cameras, USB webcams, Continuity Camera (iPhone), OBS Virtual Camera. `tests/manual/CAM-2.md` has the hardware steps.

**Verification** — Verified on macOS hardware (built-in camera): the camera file records with a preview running on the same session. Tested with fakes: retiming, pause gaps, device removal, fragment cadence, mirror metadata. Not verified: AC2 and AC7 by eye on a real display, AC1 preview latency through the bubble window, USB, Continuity and virtual cameras, Windows.

---

### CAM-2 Camera recorded as a separate track
**Priority** P0 · **Status** In progress (built on macOS; the 50 ms sync (AC2) is not measured) · **Crates** cm-camera, cm-encode · **Depends on** CAM-1

**User story** — As a creator, I want my camera saved in full quality separately from the screen, so that I can reuse it, reposition it or switch layouts later.

**Description**
- Resolution: the camera's best mode up to 1080p (setting: 720p or 1080p).
- Frame rate: the camera runs at **30 fps by default** (the device is asked for 30; configurable per recording through the camera config), independent of the screen's rate. A camera whose native modes are 24 or 25 fps delivers at its own rate. Frames are placed by their capture timestamp, never by count, and the track records the rate it asked for (`details.fps`) and the rate it achieved (`details.measured_fps`, frames written over the file span). A camera that reports a non-standard mode is recorded at its native size.
- Codec: H.264, hardware encoder, ~8 Mbps at 1080p30, fragmented MP4 (REC-8 crash safety).
- Timestamps from the capture API on the session clock (`MediaTime`), so camera and screen align even at different frame rates. The camera's presentation timestamp is on the host clock (`CLOCK_UPTIME_RAW`), the same base as the session clock.
- **Delivery latency** (host clock at the callback minus the sample's timestamp) measures 60 to 75 ms on the built-in camera (median 73.5 ms, p95 76.8 ms, 1080p30, macOS 26) and about 75 ms later than the screen in S6. This is pipeline delay; frames are **placed by capture timestamp**, so it does not shift the track. Whether the timestamp itself matches the optical event is what the flash test measures (`tests/manual/CAM-2.md`, step 3); until it runs, `latency_compensation_ms` is 0. The right long-term home for a measured offset is a per-device table keyed by camera id.
- **Pause** follows REC-4: the file has continuous timestamps (file time = session time minus the pause gaps before it); `TrackInfo.start` and `end` are session time, `details.file_start` and `file_end` are file time, and `details.pauses` is the gap list in session time (the same list as `manifest.gaps`). No frames are written inside a gap.
- The first frame arrives after the session origin (about 500 ms on the built-in camera): that is start latency, not drift (Technical design → One clock).
- Mirroring is metadata only (applied at display and export); the stored track is never flipped.

**Acceptance criteria**
- **AC1** Given a 1080p webcam, then `camera.mp4` is 1920×1080 at the configured frame rate (30 fps by default; the achieved rate is in `details.measured_fps`).
- **AC2** Given a clap recorded on camera and mic, then the camera frame of the clap and the audio peak align within NFR-3 (≤ 50 ms) at start and after 30 minutes.
- **AC3** Given mirror on, then `camera.mp4` is unmirrored and the manifest has `mirror: true`.

**Edge cases and errors** — Camera reports a non-standard mode (e.g. 1280×960) → record native, letterbox in the bubble.

**Agent interface** — `get_capture` lists tracks with sizes and frame rates.

**Out of scope** — 4K camera recording (P2).

**Test notes** — Clap test fixture; long-run drift test. The optical sync procedure (flash window against the camera, then camera against screen, then a 30-minute soak) is in `tests/manual/CAM-2.md`.

---

### CAM-3 Edit the camera layout after recording
**Priority** P1 · **Status** In progress (built on macOS: layout ranges in `edits[0].layout.segments` (`cm_core::camera_layout`), `set_camera_layout` as a daemon action, MCP tool and `capturemann edit camera`, the 300 ms smooth move in the compositor and the export, the compositor's bubble, camera full and side by side, and the editor's Camera card with the preview following the layout; tested with fakes, goldens and FFmpeg+GPU exports on this Mac, the window itself untested by eye; not built: dragging the bubble on the preview canvas, a per-range mirror in the transition) · **Crates** cm-core, cm-render, apps/desktop · **Depends on** CAM-1, EDT-3

**User story** — As a creator, I want to move, resize, reshape or hide my camera bubble after recording, and switch to camera-only for the intro, so that I fix layout mistakes without re-recording.

**Description**
- In the editor, the camera bubble is a selectable object on the preview canvas for the current segment.
- Layout segments: split the timeline (EDT-1 split) and set a layout per segment: **Screen + bubble** (default), **Screen only**, **Camera full**, **Side by side** (screen 2/3, camera 1/3, P1).
- Transitions between layouts: cut (default) or 300 ms smooth move.
- Changes apply to all exports and presets; positions are fractions, so a 9:16 export keeps the bubble inside the frame (clamped to the safe area).

**Acceptance criteria**
- **AC1** Given a recording with the bubble bottom-right, when the user drags it to top-left in the editor, then the preview and every export show it top-left for that segment.
- **AC2** Given segment 00:00–00:15 set to Camera full, then the export shows only the camera full-frame in that range.
- **AC3** Given smooth transitions, then the bubble moves over 300 ms between layouts.
- **AC4** Given a 9:16 export, then the bubble is fully inside the frame for all segments.
- **AC5** Given each layout change, then it is one undo step.

**Edge cases and errors** — Recording without a camera track → camera layouts disabled with a tooltip.

**Agent interface** — MCP `set_camera_layout { capture_id, range: [start, end], layout: "bubble"|"screen"|"camera"|"side_by_side", shape?, rect?, ring?, mirror?, transition?: "cut"|"smooth" }` → `{ revision }` (Free). CLI `capturemann edit camera <id> 0-15 --layout camera [--shape rounded] [--rect x,y,w,h] [--ring 0.017|none] [--mirror true] [--smooth]` (alias `edit layout`).

*As built (Oct 22, 2026):* a range is stored as `{from, to, mode, shape?, rect?, ring?, mirror?, transition}` in session ticks; a later range overwrites the layouts it overlaps and equal neighbours join, so one call is one undo step (AC5). `rect` is `[x, y, w, h]` as fractions of the frame (`w` is the width; the shape fixes the height) and only the bubble layout takes `shape`, `rect` and `ring`; a missing one inherits the recorder's bubble. A recording without a camera track accepts only `screen` (`invalid_input`, "has no camera track"). `transition: smooth` moves from the layout before it (the previous adjacent range, else the default bubble) over 300 ms at the start of the range, in session time (a 2× segment shortens it in the output), eased; the camera grows from, or shrinks into, its own middle when it is absent on the other side, and the screen stays under it. On a preset frame (EXP-2) the bubble keeps to the safe area, so AC4 holds for every range. Camera full and side by side make the export decode the camera picture uncropped (at most 1920 px wide) instead of the cropped bubble picture. The editor changes the range of the selected segment or range, else the kept segment under the playhead; the bubble is moved with four corner buttons and sized S, M or L (not by dragging).

**Out of scope** — Keyframed bubble motion paths.

**Test notes** — Golden frames per layout; undo tests.

---

### CAM-4 Camera picker, preview and mirror
**Priority** P0 · **Status** In progress (built on macOS: enumeration, permission state, preview frames and the camera choice in the start sheet; unverified on hardware: AC1 to AC3 with two cameras; Windows not built) · **Crates** cm-camera, apps/desktop · **Depends on** CAM-1

**User story** — As a teacher, I want to choose my camera and see myself before I start, so that I know framing and lighting are right.

**Description** — Setup panel lists cameras (name, max resolution); the bubble shows a live preview as soon as the camera is turned on, before recording; mirror toggle (default on for the preview and the recording's display, see CAM-2); last camera remembered, falls back to the system default if missing.

**Acceptance criteria**
- **AC1** Given two cameras, then both are listed and switching changes the preview within 1 s.
- **AC2** Given the remembered camera is unplugged, then the system default is selected and a note says so.
- **AC3** Given camera permission is denied, then the panel shows the permission message with "Open System Settings" and recording can still start without the camera.

**Edge cases and errors** — No cameras → camera toggle disabled with "No camera found".

**Agent interface** — MCP `list_devices` → cameras `{ id, name, kind (built_in, external, continuity, virtual), is_default, max_size, max_fps, modes, in_use_elsewhere }` and `camera_permission`; `cameras_error` when enumeration failed. Continuity Camera is listed as `continuity` (instead of `external`) only when the app bundle's Info.plist sets `NSCameraUseContinuityCameraDeviceType` to true. Virtual cameras are listed on a best-effort basis. (`packaging/macos/Info.plist` does not set the key yet, so Continuity cameras are listed as `external` until it does.)

**Out of scope** — Camera adjustments (exposure, white balance).

**Test notes** — Manual matrix of devices; unit tests for fallback logic.

---

### CAM-5 Camera background blur
**Priority** P2 · **Status** Draft (detail before v0.2) · **Crates** cm-camera, cm-ai

**User story** — As a teacher at home, I want my background blurred in the bubble, so that my room stays private.

**Summary** — On-device person segmentation (Apple Vision on macOS; an ONNX model via `ort` on Windows) applied to the camera track at export, with a live preview during recording.

**Key criteria (to refine)** — Live preview ≥ 24 fps on a 16 GB machine; export matches preview; can be turned off after recording because the stored track stays unblurred.
