# EDT-3 manual script: background, padding, corner radius and shadow

Covers what goldens cannot: how it looks in the window and in a real player (colour, edge quality,
shadow), and the presets. The geometry (padding, radius, image cover, edge to edge) is covered by
golden frames in `cargo test -p cm-render` and the exported frames in `cargo test -p cm-media`
(EDT-3.AC1 to AC3), and by the preview/export parity test (`cargo test -p cm-desktop --test
parity`). Nothing is recorded: use the synthetic fixture from [`M4-GATE.md`](M4-GATE.md) (the
default 12 s copy is enough; the camera is on, which also shows the bubble over the background).

1. **Off by default (AC3).** Open the recording in the editor. Expected: the screen fills the
   preview edge to edge, no padding, square corners. Export (`capturemann export ID --wait`) and
   play: same, the pattern reaches all four edges.
2. **Presets.** In the inspector, background: **None**, **Soft gray**, **Lens Violet gradient**,
   **Custom**. Choose **Soft gray**. Expected: the screen shrinks inside a light gray frame with
   about 8 % padding, rounded corners (12 px) and a soft shadow; the preview updates at once. The
   third preset is a violet gradient (`#4b2bd6` to `#9a6bff`). docs/DESIGN.md retires "Lens Violet" for
   the app's own chrome; a background is content, note whether the name or the colours look wrong
   to you.
3. **Padding and radius (AC1).** `capturemann edit background ID --kind color --value "#e8e9ec"
   --padding 8 --radius 12 --shadow true`, export, and open a frame at 1280x720 in Preview. Expected:
   the screen is inset by 8 % of the width at the left and right (102 px of 1280) and 8 % of the
   height above and below (58 px of 720; the border is proportional, the screen keeps its 16:9
   shape), corners rounded about 12 px at the export size, a soft shadow below (it makes the
   bottom margin look about 5 px smaller).
   Check by zooming in on a corner: the radius looks like a clean arc, not jagged.
4. **Ranges.** `--padding 25` and `--radius 30` fail with `invalid_input` (limits 0 to 20 % and 0
   to 24 px), exit code 2; `--padding 20 --radius 24` works. A colour that is not `#rrggbb` fails.
5. **Gradient.** `--kind gradient --value "#4b2bd6,#9a6bff"`. Expected: a smooth diagonal
   gradient (135 degrees), no banding you can see at normal size, shadow visible against it.
6. **Image (AC2).** Make a 1200x800 PNG (any picture) and run `--kind image --value /path/to.png`.
   Expected: the picture fills the 16:9 frame without distortion (cover: cropped, not stretched);
   a circle in the picture stays a circle. A missing file, or one that is not an image, is
   refused when you set it (`invalid_input`, exit 2, naming the path) and nothing is stored;
   PNG and JPEG are accepted (by absolute path, or relative to the recording's folder).
7. **Bubble over the background.** With the camera on, the bubble (red ring, mirrored) sits over
   the frame as in the recording's layout and stays visible against every background.
8. **Preview matches the export.** Look at the preview, then at the exported frame at the same
   time. Expected: same padding, radius, colours and shadow. Differences that are only the
   H.264 softness are fine; a different bubble position is a known gap (CAM-3).
9. **Undo.** `capturemann undo ID` returns to the previous background in one step; the editor
   preview follows without being reopened.
10. **9:16 export with padding.** `capturemann export ID --preset vertical-1080 --wait`. Expected:
    the 16:9 screen is fitted by width and centered with the background above and below
    (EDT-5 reframe will take priority when set); nothing is stretched.

Pass criteria: presets and numbers as described, edges clean, preview and export agree except
for the bubble, limits enforced. Record screenshots of steps 3, 5 and 6.

## Privacy

No camera, microphone or screen is used. The background image is yours: use a harmless picture.

## Status of the author's runs

(none yet)
