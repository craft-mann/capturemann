# CAP-1 / CAP-3 manual script: the overlay picker (`apps/overlay`, `cm-overlay`)

Covers the overlay-side acceptance criteria of CAP-1 (AC2 to AC7) and CAP-3 that need a real
display and a human: window placement, pointer mapping, hover, keys, two monitors, scaling.
The pure logic (selection geometry, click vs drag, hit testing, painting, params sent to the
daemon) is unit tested: `cargo test -p cm-overlay`. The library side of CAP-1 (AC1, AC4) is in
`tests/manual/CAP-1.md`.

## Setup
1. Build: `cargo build --release -p cm-daemon -p cm-overlay -p cm-cli`.
2. macOS: grant Screen Recording to the terminal app that runs the binaries (System Settings >
   Privacy & Security > Screen Recording), then restart the terminal.
3. Start a daemon on a SHORT socket path (Unix socket paths are limited to about 100 bytes):
   `target/release/cm-daemon --library "$TMPDIR/cm-lib" --socket /tmp/cmov.sock &`
4. Run the overlay with tracing: `CM_OVERLAY_TRACE=1 target/release/cm-overlay region --socket /tmp/cmov.sock`
   - stdout: the capture JSON (`capture_id`, `path`, `width`, `height`, `rect`, `pixel_identical`).
   - stderr (trace): one `mapping` line per display and one `first_frame` line.
   - Exit code: 0 on success, 3 on Esc (stdout `{"cancelled": true}`), 4 permission missing.
5. A golden pattern helps: open a full-screen image with a 1 px divider or a checkerboard
   (for example a PNG with a known grid) before starting the overlay.
6. Freeze captures are stored in the library with the title "CaptureMann picker frame
   (temporary)"; ignore or delete them (known gap, see the report).

## CAP-1.AC2 region is exact (1x and 2x)
1. `cm-overlay region`, drag from a known edge (for example the start of the 1 px divider) to a
   rectangle, watch the size readout, adjust with the handles, press Enter.
2. Pass when stdout has `"pixel_identical": true`, `width`/`height` equal the readout in
   physical px (e.g. a 420 x 649 readout gives a 420 x 649 image), and `rect` is the readout's
   origin. Open `path` and confirm the divider is in column 0.
3. Repeat on a 1x display and a 2x display. Repeat with `--confirm-on-release` (capture happens
   when the mouse is released).
4. `pixel_identical: false` while the screen shows changing content (video, clock) is expected:
   the daemon captures the region again after the overlay closes. Report it if the screen was
   static.

## CAP-1.AC3 covers the whole display (notched Mac)
1. On a MacBook with a notch run the overlay with `CM_OVERLAY_TRACE=1`.
2. Pass when the `mapping` line says `"covers_display_1_to_1": true`, `window_px` equals
   `capture_px` (e.g. 3024 x 1964), `offset_px` is `[0, 0]`, and by eye the menu bar and the
   notch strip are dimmed too (no bright strip at the top, no Dock visible above the overlay).
3. The overlay must appear on the current Space and also above a full-screen app.

## CAP-1.AC5 hover and click
1. `cm-overlay region` with several windows open. Move the pointer over a window: it is
   highlighted (bright, outlined); the rest stays dimmed.
2. Click without moving: pass when the JSON has `"mode": "region"`, a `capture_id`, and the image
   is that window only (at physical resolution).
3. Press, move 3 px, release: still a click (window captured). Press, move 5 px, release: a
   selection, not a window capture.
4. `cm-overlay window`: same hover and click; dragging does not start a selection.
5. With a selection on screen, a click outside it only clears the selection (no capture).

## CAP-1.AC6 Esc
1. Start the overlay, press Esc (with and without a selection): pass when every overlay window
   closes, exit code is 3, stdout is `{"cancelled": true}`, and no new capture appears in the
   library besides the temporary freeze captures.

## CAP-1.AC7 two monitors
1. With two displays (try different scales, for example Retina + 1x external), start
   `cm-overlay region`: pass when each display shows its own dimmed frozen frame, the hint pill
   follows the pointer, and `mapping` lines show `covers_display_1_to_1: true` for both.
2. Drag from the middle of display A past its edge onto display B: pass when the selection stops
   at the edge of A and never appears on B.
3. Draw a selection on A, move the pointer to B and press Enter: the selection on A is captured
   (keys go to the display the pointer was last on, so press Enter with the pointer on A, or
   draw on B and Enter there; check both behave sensibly).
4. Unplug (or disable in Displays settings) one display while the overlay is open: that display's
   overlay closes within about 0.5 s, the other stays usable. Unplug the last one: the overlay
   exits with code 3.
5. `cm-overlay display`: hover highlights the whole display under the pointer; clicking
   returns that display's capture (`width` x `height` equal its physical size).

## Handles and keys
- Handles at the 4 corners and 4 edge midpoints resize; dragging inside moves the selection;
  dragging a corner past the opposite one flips it; nothing leaves the display.
- A drag under 2 px on one side is discarded.
- Arrow keys during a drag move the active edge by 1 px, Shift+arrow by 10 px (CAP-3.AC2:
  press right-arrow 5 times: the readout width grows by exactly 5). With no selection the
  arrow keys move the pointer.

## CAP-3 magnifier
1. AC1: the loupe label shows `x, y` of the pixel under the pointer in physical px (compare
   against the readout of a 1 px selection starting there) and the loupe's center cell is that
   pixel (outlined).
2. AC3: move the pointer to within 160 px (logical) of the right edge: the loupe is on the left
   of the pointer and fully visible. Same near the bottom edge and in the corners.
3. AC4: press C: the clipboard holds `#RRGGBB` of the center pixel (paste into a text field),
   the picker stays open.
4. Drag at 4K: the selection follows the pointer smoothly (about 60 fps); note if it stutters.
   (Pixel grid dropping is not implemented; report stutter.)

## Excluded from capture
1. While the overlay is open run `capturemann capture screenshot --display 1 --out /tmp/x.png`
   from another terminal. Pass when `/tmp/x.png` shows the normal screen, with no dimming, no
   selection and no hint pill.
2. Windows: same check (uses `WDA_EXCLUDEFROMCAPTURE`, needs Windows 10 2004 or later).

## Permission missing
1. Revoke Screen Recording for the terminal, run `cm-overlay region`: pass when no window opens,
   stderr has `{"error": {"code": "permission_missing", ...}}` and the exit code is 4.

## Timing (NFR-1 / S2: visible within 150 ms)
- Read `first_frame.ms_since_start` and the `frozen` line from the trace. With the current
  daemon API the freeze (capture, PNG encode, library write, base64, decode) dominates (about
  0.65 to 1.2 s on an M4 Pro, 3024 x 1964); the window and first frame take about 100 ms after
  it. The 150 ms budget is not met until the daemon offers a raw/shared-memory freeze path.

## Windows 10/11 (not run by the author)
Repeat AC2, AC3 (taskbar covered), AC5, AC7 and the capture-exclusion check at 100% and 150%
display scaling, and with a 100% + 150% monitor pair. Pass when `mapping` shows
`covers_display_1_to_1: true` on every monitor and the region image equals the readout size in
physical px.
