# CAP-4 manual script: the clipboard (PNG and TIFF) and "Also save a PNG to a folder"

Covers what the automated tests cannot: pasting into real apps, a real unplugged drive and the
toasts and the settings row seen by eye. The pattern expander, the never-overwrite rule, the
write errors (missing folder, not a folder, read-only, disk full codes), the clipboard flavors
and the Windows retry (with a fake clipboard that refuses twice) are covered by
`cargo test -p cm-desktop` (CAP-4.AC2 to AC4). These steps capture your screen and replace what
is on your clipboard: run them only when you mean to, and delete the test files afterwards.

Privacy: the captures show whatever is on your screen. Use an empty desktop or a plain window.
Use the bundle (`make run-bundle`), not `target/release/cm-desktop`, when permissions matter.

1. **Setting.** Settings -> General -> After capture. "Also save a PNG to a folder" is off by
   default and no folder row shows. Turn it on: the folder picker opens first. Cancel it:
   the switch stays off. Turn it on again and choose a new folder `~/Pictures/CM-Test`: the switch
   is on and the row shows the path and the default name `CaptureMann yyyy-MM-dd at HH.mm.ss.png`.
2. **AC2: file in the folder.** Take a region capture. Expected: the toast "Copied to
   clipboard" appears, and `~/Pictures/CM-Test` holds one PNG named like
   `CaptureMann 2026-10-23 at 14.05.09.png` with the pixels of the capture (open it).
3. **AC3: no overwrite.** Take two display captures within one second (the shortcut twice, or
   hold the launcher's key). Expected: both files exist; the second is `... (2).png`; a
   third one is `... (3).png`. Copy any file in the folder to the name the next capture will
   get (`... (2).png` of a future second is impractical; instead set `capture.save_pattern` to
   `Fixed` in `settings.json`, capture twice, and expect `Fixed.png` and `Fixed (2).png`, with
   the first file's content unchanged). Remove the key afterwards.
4. **After annotation saves.** Open a capture in the editor, draw a rectangle. Expected: one
   more PNG appears in the folder (the rendered image with the rectangle). Draw three more
   shapes quickly: one more file for the burst, not three.
5. **Toast Copy button.** Press Copy on a capture toast. Expected: no new file in the folder.
6. **AC4: unplugged drive.** Use a USB stick or a disk image (`hdiutil create -size 20m -fs
   exFAT -volname CMUSB /tmp/cmusb.dmg`, then open it, remove it again later). Choose
   `/Volumes/CMUSB/Shots` (create `Shots` first) as the folder. Capture: the file is written.
   Eject the drive. Capture again. Expected: the capture is in the library and on the clipboard,
   and a second, warning toast says "Couldn't save to /Volumes/CMUSB/Shots: drive not
   available". Nothing named `CMUSB` is created under `/Volumes`.
7. **AC4: other reasons.** Choose a folder, then `chmod 555` it and capture: the warning says
   "permission denied". (A full disk: capture to a tiny disk image filled with `mkfile`; the
   warning says "disk full".) Restore the permissions afterwards.
8. **Agents cannot set it.** `capturemann settings set capture.save_folder /tmp/x` is refused
   with `user_declined` (`app_only_setting`); `capture.delay_s` is accepted.
9. **TIFF flavor (macOS).** After a capture with the clipboard on, run
   `osascript -e 'clipboard info'`. Expected: both `PNGf` and `TIFF` entries. Paste (Cmd+V)
   into Preview (File -> New from Clipboard), TextEdit, Slack and a browser's image field: each
   shows the image at full size. Also paste into an older or TIFF-only tool you have (for
   example an old Office or Keynote version, or `pbpaste -Prefer tiff > /tmp/x.tiff && open
   /tmp/x.tiff`).
10. **Windows retry (not run).** The 3 tries over 300 ms for a clipboard another app holds
    are tested with a fake only. The Windows build has not run on a machine: when it does, lock
    the clipboard from another app (a clipboard manager that holds it) and expect either a
    paste after a short delay or the toast "Copied to library only".

Delete the test files, `~/Pictures/CM-Test` and the test captures from the library when done.

## Pass criteria

| AC | Steps | Pass when |
| --- | --- | --- |
| CAP-4.AC1 | 2, 9 | the clipboard holds the image within 300 ms (PNG, and TIFF on macOS) and the toast offers Annotate, Copy, Save as and Delete |
| CAP-4.AC2 | 1, 2, 4 | a PNG with the pattern name exists in the chosen folder |
| CAP-4.AC3 | 3 | nothing is overwritten; later files end in (2), (3) |
| CAP-4.AC4 | 6, 7 | the capture is in the library and the warning toast names the folder and the reason |
| TIFF flavor | 9 | `clipboard info` lists `TIFF`, and the old app pastes it |

## Report

Copy the template in `verification/README.md` to `verification/CAP-4-<date>-<who>.md`, fill the
step and AC tables (note the macOS version, the drive type and the apps pasted into), and commit
it with the change. Mark step 10 "skipped (no Windows machine)".
