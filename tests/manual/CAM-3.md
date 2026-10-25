# CAM-3 manual script: edit the camera layout after recording

Covers the bubble as an object on the preview canvas (drag, resize), layouts per segment, the
300 ms transition, and the preview and the exported file agreeing about where the bubble is. The
compositor geometry (circle and rounded bubble, ring, mirror, clamped inside the frame, camera
full, side by side) is covered by `cargo test -p cm-render`; the undo steps and the manifest by
`cargo test -p cm-core -p cm-daemon` once those pieces land. Nothing is recorded: use the
synthetic fixture from [`M4-GATE.md`](M4-GATE.md) with the camera on (the default). The camera
track is SMPTE colour bars with a white flash at the same moments as the screen's.

Status of the feature: built (Oct 22, 2026): layout segments, `set_camera_layout`
(`capturemann edit camera ID 0-5 --layout camera|screen|side_by_side|bubble [--smooth ...]`), the
300 ms transition and the editor Camera card. Dragging the bubble on the canvas is not built (the
card has corner buttons). The automated check that the preview draws the bubble the way the
export does is `cargo test -p cm-desktop --test parity` (no longer ignored).

1. **Bubble in the export today.** `capturemann export ID --wait`, play the file. Expected: a
   round camera bubble with a red ring at the lower right of the frame (the fixture's stored
   layout), showing the colour bars **mirrored** (the fixture's camera has mirror on), and flashing
   white together with the screen at 1.39, 4.39, 8.39 and 11.39 s.
2. **Preview shows the same (AC1) [needs CAM-3].** Open the recording in the editor. Expected: the
   bubble is in the same place as in the export, with the ring and the mirrored bars. Known
   today: the preview draws a fixed bubble at the lower left without ring or mirror.
3. **Drag [needs CAM-3].** Drag the bubble to the upper left on the canvas. Expected: it follows
   the pointer, stays fully inside the frame (a margin from the edges), and the preview and a new
   export both show it at the upper left for that segment (AC1).
4. **Resize and shape [needs CAM-3].** Choose small, medium, large, and circle or rounded square.
   Expected: the sizes are about 160, 240 and 320 points wide on a 1280 px frame scaled to the
   frame, a rounded square is 4:3 with 24 px corners, the ring stays 4 px.
5. **Layouts per segment (AC2) [needs CAM-3].** Split at 5 s (**S**), select the first segment and
   choose **Camera full**. Expected: for 0 to 5 s the export shows only the camera, full frame
   (the colour bars fill the frame), and from 5 s the screen with the bubble again.
   `capturemann edit camera ID 0-5 --layout camera` does the same (`set_camera_layout`). Also try **Screen only** (no bubble) and **Side by side** (the screen
   on two thirds, the camera on one third).
6. **Transition (AC3) [needs CAM-3].** Set the transition to smooth between two layouts, play
   across the boundary. Expected: the bubble glides for about 300 ms (count about 9 frames at
   30 fps), no jump; "cut" jumps in one frame.
7. **Vertical export (AC4) [needs CAM-3].** `capturemann export ID --preset vertical-1080 --wait`.
   Expected: the bubble is fully inside the 1080x1920 frame in every segment and out of the
   bottom 20 % and right 12 % (platform UI zone).
8. **Undo (AC5) [needs CAM-3].** Each layout change is one Cmd+Z. Change the layout three times
   and undo three times: the bubble returns step by step.
9. **No camera.** `make_fixture --no-camera`: the camera layouts are disabled in the editor with
   a tooltip; the export has no bubble.
10. **Preview against export.** With the same edit, take a screenshot of the preview and of an
    exported frame at the same time. Expected: bubble position, size, ring and mirror match;
    H.264 softness is the only difference. Run the automated check too:
    `cargo test -p cm-desktop --test parity -- --ignored --nocapture`.

Pass criteria: preview and export agree on the bubble (step 10 and the ignored test), every layout
change is one undo step, the bubble stays inside the frame in every export size. Record
screenshots of steps 2, 5 and 7.

## Privacy

No camera is used: the camera track is synthetic colour bars.

## Status of the author's runs

(none yet)
