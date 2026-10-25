# EXP-1 manual script: the interim `quick` export (one combined MP4 after recording)

Covers what the automated tests cannot: a real recording with a real camera and microphone, the
bubble seen by eye, and the toasts and library card in the real window. Everything else (filter
graph, offsets, ring and circle pixels, mirror, A/V alignment on `lavfi` fixtures, cancel, missing
FFmpeg, auto-export order) is covered by `cargo test -p cm-media -p cm-daemon` (EXP-1.AC3, AC5 to
AC8, EXP-4.AC2/AC4, REC-1.AC7). These steps record your screen, camera and microphone: run them
only when you mean to, keep them short, and delete the output.

Use the bundle (`make run-bundle`), not `target/release/cm-desktop`, when permissions matter. A bundled app contains
its own FFmpeg (`make ffmpeg`, then `make bundle`); a development build run with `cargo run` needs
`brew install ffmpeg` (or `CM_FFMPEG`).

1. **Auto-export.** Settings -> Capture & recording: "Create a single MP4 after recording" is on.
   Record 10 s with the camera bubble, the microphone, and (if you like) system audio, moving the
   bubble to another corner once before you stop. Stop. Expected: the "Recording saved" toast
   becomes "Recording saved - exporting..." with a percentage, then "Exported <title>" with Play and
   Reveal.
2. **The file.** Press Play. Expected: one video with the screen, a round camera bubble with a red
   (`#FF4F3A`) 4 px ring, your voice in sync with your lips and with the clicks on screen (within
   about 50 ms), and the system audio mixed in. The bubble is at the place where you left it (the
   layout at stop; a bubble moved halfway shows at its final place for the whole video: known gap).
   Mirror on in Settings -> Webcam & audio flips the camera in the export, off does not.
3. **Library.** Open the recording. Expected: an "Exports" row (`quick`, size, duration, encoder),
   "Play export" as the main button, "Reveal export", "Play original" (the screen master), and the
   masters still under "Tracks". Reveal opens Finder on `exports/<title> - quick.mp4`.
4. **Setting off.** Turn the setting off, record 5 s. Expected: no export; the card shows "None
   yet" and an "Export MP4" button that renders it on demand with progress.
5. **FFmpeg missing.** Start the app with `CM_FFMPEG=/nonexistent/ffmpeg` and record 5 s. Expected:
   the toast "FFmpeg not found" with the Homebrew hint; the recording is saved and plays; the job is
   listed as failed once (`capturemann jobs list --type export`), not retried.
6. **Recording while exporting.** Record a longer take (60 s), stop, and start a new recording
   while the export is running. Expected: the export pauses (the job is `paused`, `hold:type:export`)
   and starts again from the beginning after the second recording stops.
7. **CLI.** `capturemann export <id> --wait` prints the file; a second run replaces the `quick`
   entry in `manifest.json` (`exports` keeps one entry per preset).

Delete the test recordings from the library when done.
