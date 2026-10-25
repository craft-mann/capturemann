# EDT-4 manual script: editable cursor and zoom regions

Covers the cursor and zoom controls in the window, the look of the rendered cursor, easing and
Follow cursor in the preview and in the exported file. Golden frames (cursor at 2x at three
timestamps, a zoom at 1.8x with 400 ms easing, the dead zone) and the undo steps are meant to be
covered by `cargo test -p cm-render -p cm-core -p cm-daemon` once the feature lands. Nothing is
recorded: use the synthetic fixture from [`M4-GATE.md`](M4-GATE.md). It has a `cursor.jsonl` (a
smooth loop with left clicks at screen file times 2.0, 6.5 and 9.0 s, so session 2.39, 6.89 and
9.39 s) but **its screen track has no cursor drawn into it**, which is what "Record cursor
separately" produces; so the editable options are available. A fixture made with `--no-cursor`
has no telemetry: the cursor options are disabled with a tooltip, zooms still work.

Status of the feature: built for M4 (Oct 22, 2026) except auto-zoom suggestions (step 8), which
need the AI-3 click analysis from M5. **Fixture:** `make_fixture` stores the screen track as "cursor recorded separately" (`tracks.screen.cursor` is `false` in `manifest.json`) whenever it writes a cursor log, so the options below work on a fresh fixture.

1. **Cursor controls available [needs EDT-4].** Open the recording in the editor. Expected: the
   inspector offers cursor size (1x to 3x), smoothing (off, light, strong), click effect (none,
   ring, ripple) and "hide when idle for 2 s". With `--no-cursor` they are disabled with a
   tooltip that explains why.
2. **Size (AC1) [needs EDT-4].** Set size 2x. Expected: a cursor drawn at the telemetry positions
   (the moving loop), twice as large as at 1x; compare the preview and an exported frame at 3
   moments: same size and position (within a few pixels).
3. **Smoothing [needs EDT-4].** Off, light and strong. Expected: the path is the raw telemetry for
   off, a gently smoothed one for light, a spring-like follow for strong; the cursor never lags
   behind the click it makes by more than a few frames (watch the ring appear at the click).
4. **Click effect [needs EDT-4].** Choose ring, then ripple. Expected: at session 2.39, 6.89
   and 9.39 s a ring or ripple appears under the cursor and fades within about half a second.
5. **Hide when idle [needs EDT-4].** The fixture's pointer never rests, so this step needs a real
   recording where you left the pointer still for a few seconds (short, screen and cursor only):
   with the option on, the cursor fades out after 2 s without movement and returns when it
   moves. Skip it with "no suitable recording" if you have none.
6. **Zoom region (AC2) [needs EDT-4].** Add a zoom at 15 s for 3 s at 1.8x on a 60 s fixture
   (`make_fixture --secs 60 --no-pause`), focus on the frame centre. Expected: the picture eases
   in over 400 ms, holds at 1.8x from 15.4 to 17.6 s, eases out over 400 ms; the digit's size
   shows the 1.8x; the preview and the export agree. Scale limits 1.2x to 3x are enforced.
7. **Follow cursor (AC3) [needs EDT-4].** Turn it on for the zoom. Expected: the cursor stays
   inside the visible frame during the zoom, and the view only pans when the cursor leaves the
   central 80 % (10 % dead zone), with no jitter.
8. **Suggestions (AC4) [needs EDT-4 and AI-3].** Not for M4: with 5 suggested zooms shown dashed,
   "Accept all" adds 5 zooms as one undo step.
9. **Edge cases [needs EDT-4].** A zoom over a cut range is clipped to the remaining content; a
   new zoom that overlaps another shortens its neighbour; zooms keep their place when the
   recording is trimmed.
10. **Undo [needs EDT-4].** Each zoom or style change is one Cmd+Z.
11. **CLI [needs EDT-4].** The same through MCP and the CLI (`set_zooms`, `set_cursor_style`):
    one call, one revision, `capturemann undo ID` reverts it.

Pass criteria: size, easing time and dead zone as written, preview and export agree, undo works.
Record screenshots of steps 2, 4 and 6.

## Privacy

No screen is recorded; the cursor path is synthetic. A real recording's cursor log contains
positions and clicks only, never keys.

## Status of the author's runs

(none yet)
