# Editing (EDT)

All edits change `manifest.edits` only; master tracks are never modified (Technical design → Data model). Every edit is one action and one undo step, from the GUI or an agent. Times are `MediaTime` ticks internally; the UI shows `mm:ss.ff` (frames) and the API accepts seconds (float) or ticks (`"t:<int>"`).

### EDT-1 Trim, cut and split
**Priority** P0 · **Status** In progress (model, actions, MCP tools and CLI built on macOS and tested with fakes; the editor view in `apps/desktop` is built (preview, timeline with trim handles, hatched cuts, split markers, playhead, the I O S Delete R N and arrow keys, undo and redo) and its requests are tested against the real daemon, but the window itself is untested by eye; the preview plays: `cm_media::decode` streams the screen and camera masters (one FFmpeg process per track, a frame cache, 500 to 1,900 fps measured for 1080p and 4K sources on an Apple M4 Pro, 45 to 130 ms per scrub) and the sound is the exporter's mix (`cm_audio::mix`) played through `cm_audio::playback`, with the audio device as the master clock, tested with fakes and FFmpeg fixtures but not heard or seen; silence-boundary snapping is not built; the export of trims and cuts is built (EDT-1.AC1 and AC2 tested with FFmpeg and a GPU on this Mac)) · **Crates** cm-core, apps/desktop · **Depends on** REC-1, DESIGN §5.5

**User story** — As a creator, I want to trim the start and end, cut out mistakes, and split the recording, so that the final video only has what I meant to show.

**Description**
- **Trim**: drag the in/out handles at the ends of the timeline, or set them at the playhead with I and O.
- **Cut**: select a range (drag on the ruler or Shift+click two points) and press Delete; the range shows as hatched (DESIGN §5.5) and is skipped in preview and export. Cut ranges can be restored by selecting the hatched area and pressing Restore.
- **Split**: S at the playhead creates a segment boundary used by speed (EDT-2) and layout (EDT-3) settings.
- All tracks (screen, camera, audio, cursor) are cut together; they never drift apart.
- Edit times (`in`, `out`, cuts, zooms) are **session time**. A track's first sample is at its own start offset (`TrackInfo.start`, 30 to 500 ms after the origin), and after a pause the media files are compacted (file time = session time minus the pause gaps before it, except `cursor.jsonl`, which stays in session time). The editor and exporter map every track through `TrackInfo.start` and `manifest.gaps`; pauses are removed from every track and from the output length (`EditList::without_pauses` adds each pause to the cuts, and the daemon's reported duration, the export and the audio mix all use that view); the default in-point is the manifest's `live_at` (REC-1, Technical design → One clock).
- Snapping (toggle N): to the playhead, segment boundaries, silence boundaries (from AI-4 when available) and frame boundaries.
- Precision: edits land on exact frames of the screen track; keyboard ←/→ moves one frame, Shift+←/→ one second.
- *As built (Oct 22, 2026):* the editor is `Edit` on a recording's details card (route `RecordingEditor`). **Keys:** I and O set the in and out points at the playhead, S splits, Delete or Backspace cuts the selected range or segment (or restores a selected cut), R restores, N toggles snapping, Space plays, ←/→ and Shift+←/→ step a frame and a second, Home and End jump to the ends, Cmd/Ctrl+Z and Shift+Cmd/Ctrl+Z undo and redo (Y also redoes on Windows). **Pointer:** click the ruler to seek, drag on the ruler or Shift+click twice to select a range, drag the playhead, drag the in and out handles, click a segment or a hatched cut to select it, scroll to pan and Cmd/Ctrl+scroll to zoom. Every edit is sent to the daemon as the action of the same name (one at a time, with a fresh `request_id`) and the answer replaces the edit list, so the window never holds an edit the CLI would not see. **Raised for the owner:** the timeline axis is **session time**, so a cut stays a visible hatched block that can be restored; EDT-2 says the timeline shows "segment width at output duration", which would hide cuts. The preview and the transport time are on the output timeline (cuts skipped, speed applied, pauses removed: the timeline strip shows the stored list, the transport uses `EditList::without_pauses`). **Playback:** Space plays and pauses; **L** toggles loop (off by default: playback stops at the end, and Space at the end plays from the start). The clock is the audio device while it plays and the wall clock without audio, with the mix still being rendered, or when the device stalls (it falls back and restarts the sound if the two differ by more than 0.25 s). A recording with a microphone or system audio plays with the AUD-4 gains and mutes of `edits[0].audio`; an edit that changes what is heard (cut, speed, trim, gain, mute) starts a new mix in the background and the sound switches when it is ready. The device\'s own output latency (about 10 to 20 ms) is not subtracted. The preview does not apply denoise switching or a loudness target.

**Acceptance criteria**
- **AC1** Given a 60 s recording, when the user trims to 2.0–58.0 s, then the export is 56.0 s ± 1 frame.
- **AC2** Given a cut from 10.0 to 15.0 s, then preview and export skip it and audio/video stay in sync after the cut (NFR-3).
- **AC3** Given a cut, when the user restores it, then the original content returns (one undo step either way).
- **AC4** Given two cuts and a trim, then `master` files are byte-identical to before editing.
- **AC5** Given an agent call `cut_segments` with three ranges, then all three are applied as one undo step.

**Edge cases and errors** — Overlapping cut ranges merge; a range outside the duration is clamped; a cut leaving < 0.5 s total → `invalid_input` "Nothing left to export".

**Agent interface** — MCP `trim { capture_id, start, end }`, `cut_segments { capture_id, ranges: [[start, end], …] }`, `restore_segments { capture_id, ranges }`, `split { capture_id, at }` → `{ revision }` (Free). CLI `capturemann edit trim <id> --start 2 --end 58`, `edit cut <id> 10-15 40-42`.

**Out of scope** — Multi-clip timelines, inserting other media.

**Test notes** — Frame-accurate tests with a frame-counter fixture video; sync test after cuts.

**As built (Oct 22, 2026, macOS; tested with fakes, no GUI yet).** The edit model is `cm_core::edit` (`EditList`, `Timeline`, `track_time`): pure data and arithmetic, used by the daemon today and by the compositor and exporter later. It is stored in `manifest.edits[0]`:

```json
{ "version": 2, "in": 36000, "out": 7236000, "speed": 1.0,
  "cuts": [[1200000, 1800000]], "splits": [2400000],
  "speeds": [{"from": 2400000, "to": 3600000, "factor": 2.0, "keep_audio": false}],
  "background": {"kind": "color", "value": "#112233", "padding_pct": 8.0, "radius_px": 12, "shadow": true},
  "layout": {} }
```

All times are session ticks. The recorder's original entry (`{in, out, speed, layout}`, no `version`) still loads: its `out` is a file time and is converted through `manifest.gaps`, its `in` becomes `live_at`, and unknown keys such as `layout` are kept. `Timeline::at_output(t)` answers "which session time, speed, segment and audio flag for output time `t`"; `track_time(gaps, track_start, t)` maps a session time into a track file (file position 0 is the track's first sample; the cursor log stays in session time). Each edit is one daemon action (`trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`), one revision tagged `user` or `agent:<name>` and one step in `manifest.edit_history` (both full `edits[0]` entries per step); `undo` and `redo` on a recording walk those steps. The actions are refused with `busy` while the recording is being recorded or exported and with `invalid_input` on a screenshot. Segments are the spans between `splits`; a speed range adds splits at its edges.

Built and tested with fakes: AC1 (the timeline measures 56.0 s), AC2 (the cut is skipped in the timeline; audio sync after a cut is not testable until the compositor exists), AC3, AC4 (the master is byte-identical) and AC5. "Snapping" and the keyboard moves are UI and not built. Cut ranges that overlap merge, ranges outside the kept part clamp, and a result under 0.5 s fails with `invalid_input` "Nothing left to export".

---

### EDT-2 Per-segment speed
**Priority** P1 · **Status** In progress (the `set_speed` action, tool, command and duration math are built and tested with fakes; the export applies the speed with the pitch-preserving WSOLA stretch of `cm_audio::mix` (EDT-2.AC1 and AC2 tested on the exported file with FFmpeg and a GPU on this Mac: 300 Hz stays within 3% at 1.5x; AC3 mutes above 2x); the inspector's speed buttons (0.5×, 1×, 1.5×, 2×, 4×, Keep audio above 2×) and the timeline badge are built, untested by eye) · **Crates** cm-core, cm-render, cm-media · **Depends on** EDT-1

**User story** — As a creator, I want to speed up slow parts like installs or loading, so that viewers don't wait.

**Description** — Select a segment (between splits) and set speed 0.5×, 1×, 1.5×, 2×, 4× (or a value 0.25–8×). Audio is time-stretched with pitch preserved up to 2×; above 2× audio is muted by default with an option to keep it. Cursor and zooms follow the speed. The timeline shows segment width at output duration with a speed badge.

**Acceptance criteria**
- **AC1** Given a 20 s segment at 2×, then it lasts 10 s in the export (± 1 frame).
- **AC2** Given 1.5× on speech, then pitch is unchanged (fundamental frequency within 3% of the original on a test tone).
- **AC3** Given 4×, then the segment's audio is muted unless "Keep audio" is on.

**Edge cases and errors** — Speed on a segment with the camera bubble: camera plays at the same speed (it's one timeline).

**Agent interface** — MCP `set_speed { capture_id, range, factor, keep_audio? }` (Free). CLI `capturemann edit speed <id> 30-50 --x 2`.

**Out of scope** — Speed ramps, reverse.

**Test notes** — Duration and pitch tests in CI.

*As built:* AC1 (duration) and AC3 (the audio flag is off above 2x unless `keep_audio`) are tested in `cm_core::edit`; AC2 (pitch) needs the audio path and is not built. A speed range adds segment boundaries at its edges; setting a new speed over part of an existing one splits it; factor 1 clears it.

---

### EDT-3 Layout: bubble per segment, background and padding
**Priority** P1 · **Status** In progress (built, tested with goldens: the GPU frame compositor in `cm-render` draws the background, padding, corner radius, shadow and the screen for AC1 to AC3; the `set_background` action, tool, command and setting validation are built and tested with fakes; the export draws the stored background, padding, radius and shadow (tested with FFmpeg and a GPU on this Mac: padding is a fraction of the width at the sides and of the height above and below, then the screen is fitted inside (owner decision Oct 22, 2026)); the editor view has the preset list, padding, corner radius, shadow and colour swatch controls, untested by eye; not built: a free colour field and image backgrounds in the inspector) · **Crates** cm-render, apps/desktop · **Depends on** CAM-3, EXP-1

**User story** — As a creator, I want my screen framed on a background with padding and rounded corners, so that my videos look polished like Screen Studio demos.

**Description**
- **Background frame** (off by default): background = solid color, linear gradient (2 stops), or image; padding 0–20% of the frame; screen corner radius 0–24 px; screen shadow on/off.
- Camera layout per segment is defined in CAM-3; EDT-3 owns the background and how both fit in the frame.
- Background settings are capture-level (same for all segments) in v0.1.
- Presets: "None", "Soft gray", "Lens Violet gradient", "Custom".
- *As built, raised for the owner:* the editor offers these four presets, with the third labelled as the spec names it. docs/DESIGN.md retires the Lens Violet palette for the app's own chrome ("Don't use the Lens Violet palette"); a background preset is content, not chrome, so the name and its violet gradient (`#4b2bd6` to `#9a6bff`) are kept until the owner decides. Soft gray is `#e8e9ec`. Both use 8 % padding, a 12 px radius and a shadow; Custom starts from the current background and adds six swatches.

**Acceptance criteria**
- **AC1** Given padding 8% and radius 12 px on a 16:9 export, then the screen is inset by 8% of the frame width at the left and right and, proportionally, 8% of the frame height above and below (a screen with the frame's own aspect keeps it: 10% on 1280×720 gives 128 px at the sides and 72 px above and below), corners rounded 12 px at the export size, matching the golden frame.
- **AC2** Given a background image, then it fills the frame (cover) without distortion.
- **AC3** Given the background is off, then the screen fills the frame edge to edge.

**Edge cases and errors** — 9:16 export with a 16:9 recording and padding: the screen is fit by width inside the padded area and centered (a screen too tall for the area is fit by height); EDT-5 reframe takes priority when set.

**Agent interface** — MCP `set_background { capture_id, kind: "none"|"color"|"gradient"|"image", value, padding_pct?, radius_px?, shadow? }` (Free).

**Out of scope** — Per-segment backgrounds, animated backgrounds.

**Test notes** — Golden frames per preset and aspect ratio.

*As built:* the settings are stored and validated (`kind`, `padding_pct` 0 to 20, `radius_px` 0 to 24, `shadow`; a color is `#rrggbb`, a gradient two of them, an image a path to a PNG or JPEG, absolute or relative to the recording's folder, which must exist and decode or the call is `invalid_input`); fields left out keep their value. The presets ("Soft gray", ...) are UI and not built. AC1 to AC3 need the compositor.

---

### EDT-4 Editable cursor and auto-zoom
**Priority** P1 · **Status** In progress (built on macOS Oct 22, 2026, the pure math and the actions tested with fakes, the compositor and the export tested with goldens and with FFmpeg and a GPU on this Mac, the editor not seen by eye: the telemetry cursor drawn with size 1x to 3x, zero-phase smoothing, ring and ripple click effects and hide when idle, for recordings made with "Record cursor separately"; zoom regions with 400 ms easing and a follow-cursor camera with the 80% dead zone, in the preview and the export; `set_zooms`, `set_cursor_style`, `capturemann edit zoom|cursor`, the zoom lane and the Zoom and Cursor cards in the editor; the recording option `cursor_separate` (setting `recording.cursor_separate`, `start_recording { cursor_separate }`, `record start --cursor-separate`); **not built:** the auto-zoom suggestions and "Accept all" (AC4, they need AI-3, M5), the option in the desktop start sheet, dragging a zoom on the lane (steppers move and size it) · **Crates** cm-core, cm-cursor, cm-render, cm-media, cm-ai · **Depends on** REC-7, AI-3

**User story** — As a creator, I want a larger, smoother cursor with click highlights and the video zooming in where I click, so that viewers can follow what I'm doing on a 4K screen.

**Description**
- **Editable cursor** requires the recording to have been made with "Record cursor separately" (cursor hidden in the screen track). Then the editor offers: size 1×–3×, smoothing (off, light, strong: spring smoothing of the telemetry path), click effect (none, ring, ripple) and "hide when idle for 2 s".
- If the cursor was baked in, the editable options are disabled with a tooltip explaining why; zooms still work.
- **Zooms** are regions on a zoom lane above the screen track: start, length, focus point (fraction of frame), scale 1.2×–3× (default 1.8×), easing in/out 400 ms. "Follow cursor" keeps the focus on the cursor path while zoomed, with a dead zone of 10% of the frame to avoid jitter.
- Auto-zoom suggestions come from AI-3 and appear dashed until accepted (all or one by one).
- *As built (Oct 22, 2026):* the manual parts only; the suggestions wait for AI-3 (M5). The model is `cm_core::edit_view`; the math (a 120 Hz cursor path, zero-phase smoothing, idle fade, click effects, the zoom and follow camera) is `cm_cursor::view`, called by the preview and the export through `cm_media::edit_view::EditView`, so both draw a frame the same way. Smoothing is a forward-and-backward exponential filter (30 ms light, 90 ms strong per pass), so a smoothed cursor reaches a button when the real one does. The cursor and the click effects are drawn in the screen's own pixels, so they scale and move with a zoom. The zoom lane is a strip **below** the timeline on the same time axis (the spec says above the screen track; the tracks' geometry is not changed), zooms are selected on it and sized with steppers, not dragged. The zoom easing is 400 ms of session time, so on a 2x segment it looks 200 ms. A new zoom shortens the ones it overlaps and is clipped out of cuts and the trimmed ends. Recordings made before the option existed bake the pointer in: the Cursor card shows its controls off with the reason.

**Acceptance criteria**
- **AC1** Given a separate-cursor recording, when cursor size is 2×, then the rendered cursor is twice the size at its telemetry positions (golden frames at 3 timestamps).
- **AC2** Given a zoom at 00:15 for 3 s at 1.8×, then frames 00:15.4–00:17.6 show the focus area at 1.8× and transitions take 400 ms each.
- **AC3** Given Follow cursor, then the cursor stays inside the visible frame during the zoom, and the view only pans when the cursor leaves the central 80%.
- **AC4** Given 5 suggested zooms, when the user clicks "Accept all", then 5 zooms are added as one undo step.

**Edge cases and errors** — Zoom over a cut range: the zoom is clipped to the remaining content. Zoom regions can't overlap; a new one shortens its neighbor.

**Agent interface** — MCP `set_zooms { capture_id, zooms: [{ at, length, focus: [x, y], scale, follow_cursor? }], replace?: false }` and `set_cursor_style { capture_id, size?, smoothing?, click_effect?, hide_idle? }` (Free).

**Out of scope** — Motion blur (P2), keystroke overlays.

**Test notes** — Golden frames; telemetry-driven synthetic recordings as fixtures.

---

### EDT-5 Reframe to 9:16 and 1:1
**Priority** P1 · **Status** In progress (built on macOS Oct 22, 2026: the reframe window (Fixed per segment, Follow cursor with the EDT-4 dead zone, Fit) stored per aspect in `manifest.edits[0].reframe`, drawn by the compositor and the export (the vertical and square presets fill the frame with the window, with no padding), `set_reframe`, `capturemann edit reframe` and the Reframe card with a 16:9 / 9:16 / 1:1 preview; the pure math and the actions tested with fakes, the export tested with FFmpeg and a GPU on this Mac, the editor not seen by eye; not built: dragging the window on the preview (steppers move it), the bubble placed by the CAM-3 vertical rules beyond the preset safe area) · **Crates** cm-core, cm-cursor, cm-render, cm-media · **Depends on** EXP-2, REC-7

**User story** — As a creator, I want to turn my 16:9 recording into a vertical video for TikTok and Shorts, so that I don't record twice.

**Description**
- Choosing a 9:16 or 1:1 preset opens a reframe view: a crop window at the target ratio over the 16:9 frame.
- Modes: **Fixed** (user positions the window per segment), **Follow cursor** (window follows the cursor path with the same dead-zone smoothing as EDT-4), **Fit** (whole screen letterboxed with background).
- Camera bubble is placed inside the vertical frame (default: bottom third, centered) per CAM-3 rules.
- Reframe settings are stored per aspect ratio, so the 16:9 edit stays untouched.

**Acceptance criteria**
- **AC1** Given a 3840×2160 recording and a 1080×1920 preset in Fixed mode, then the export is 1080×1920 showing the chosen window area at full available resolution.
- **AC2** Given Follow cursor, then the cursor stays inside the frame for 99% of frames of the test fixture.
- **AC3** Given reframe settings for 9:16, then the 16:9 export is unchanged.

**Edge cases and errors** — Source narrower than the window at native resolution → upscale with a quality warning in the export sheet.

**Agent interface** — MCP `set_reframe { capture_id, aspect: "9:16"|"1:1", mode: "fixed"|"follow_cursor"|"fit", window?: [x, y, w, h], range? }` (Free).

*As built, raised for the owner (the spec and the code differ):* the tool is `set_reframe { capture_id, aspect, mode?, focus?, at? }`. `mode` is `fit`, `fixed` or `follow` (`follow_cursor` is accepted as a synonym); `focus` is the **centre** of the window as `[x, y]` fractions of the frame, not a `[x, y, w, h]` rectangle, because the window's size is fixed by the aspect (the largest window of that ratio); `at` is a time inside the segment the focus is for, in place of `range`. Settings are stored per aspect: `reframe: {"9:16": {mode, windows: [{from, focus}]}}`. The CLI is `capturemann edit reframe <id> --aspect 9:16 --mode fixed --focus 0.3,0.5 --at 40`.

**Out of scope** — Face tracking for the camera in vertical layouts (P2).

**Test notes** — Golden frames; cursor-inside-frame metric over fixtures.
