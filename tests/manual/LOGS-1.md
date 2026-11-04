# LOGS-1: a normal session leaves no ERROR or WARN in the logs

Release 0.2.7 requirement: after ordinary use, `desktop.log`, `daemon.log` and `overlay.log`
(folder `~/Library/Application Support/CaptureMann/logs`) contain no `ERROR` and no `WARN` line.

Privacy: this script records nothing. The check script keeps only line counts.

## Setup

1. Quit CaptureMann. Start the build under test (`make run-bundle-dev` for a development bundle).
2. `scripts/check_logs_clean.sh mark` (remembers where each log ends).

## Session (about 10 minutes)

Do each step once, at a normal pace, and leave the toast steps to finish by themselves.

1. Open the Library. Open Import (sidebar), pick a short audio or video file, let it import.
2. Let a toast appear and go by itself (about 5 s). Cause a second toast within a second of the
   first one's end (for example import two files one after the other, or copy a capture twice).
3. Dismiss one toast with its x button and one with its action button.
4. Open the imported capture in the editor; play for a few seconds; open the Transcript tab and
   start a transcription of the short file; wait until it finishes. The percentage must move
   through the whole run (no long stop at 99 %).
5. Open Settings and each page once; switch the language to Vietnamese and back.
6. Open the record sheet (do not record) and cancel it.
7. Quit from the tray menu while a transcription is still running (start one on a longer file
   first). Quit must close the app within a few seconds with the toast "Stopping background
   work...".

## Pass criteria

- `scripts/check_logs_clean.sh check` prints `clean: ...` and exits 0.
- `grep -E ' (ERROR|WARN) ' desktop.log daemon.log overlay.log` shows nothing from this session.
- If a line shows, copy it (it holds no private data) into the issue, with the step it followed.

## Notes

- The font notice `skipping duplicate font ... .AppleSystemUIFont` is written at debug level
  (`cm_core::logging`, `effective_level`); `RUST_LOG=gpui_apple=debug` shows it.
- Toast windows: the window stays one pixel in the corner for 2 s after the last toast and a new
  one is never opened within 600 ms of a close (`ui/app/toasts.rs`, `window_plan`), because GPUI
  logged `window not found` 120 times a second for a window opened right after a close. This is
  not proven on hardware by an automatic test; step 2 is the check.
