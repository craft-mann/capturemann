# COUNT-LATENCY: how late do the sidebar counts follow a capture (LIB-1.AC7)

Real hardware only (it takes a real screenshot and a real recording). Not run yet. The numbers measured with fakes are in the 0.3.2 report; this script gets the ones that need a real screen and encoder.

## Setup

1. Start the app from a terminal with debug logging: `RUST_LOG=cm_daemon=debug,cm_desktop=info target/bundle/CaptureMann-dev.app/Contents/MacOS/cm-desktop 2> /tmp/count.log` (any build; the debug lines cost nothing at the default level).
2. Open the library (All captures) and note the counts in the sidebar.

## Screenshot

1. Take a full-screen screenshot of the biggest display (hotkey or launcher).
2. Stopwatch (phone video of the screen works): from the shutter to the moment the Screenshots count goes up by one.
3. In the log find `screenshot stages (cumulative ms): capture=A png=B bundle=C poster=D`. The `capture.created` event is published right after `poster`; the count is on screen a few ms after that.
   - `capture` is the OS capture; `png - capture` the PNG encode; `bundle - png` the folder, `original.png` and the index row; `poster - bundle` the thumbnail.
4. Repeat on the Screenshots page, on the Settings page and with a search typed in the library: the count must change on all of them without opening the library (LIB-1.AC7).

## Recording and audio

1. Record about 10 s of screen, then stop. Repeat with an audio-only recording.
2. Stopwatch from the Stop click to the Recordings (or Audio) count changing.
3. In the log find `recording finalized: capture <id> stop_ms=X finish_ms=Y`. `stop_ms` is the encoders' own stop (the hardware encoder flush, camera, microphone), `finish_ms` the manifest and the index row. The event follows at once.

## Pass

- The count follows the event: stopwatch time minus (`poster` for a screenshot, `stop_ms + finish_ms` for a recording) is under 0.2 s.
- Record the stage numbers, the display size and the machine in `verification/` (template in its README). Without a stored report, do not write "verified".
