# IMP-WIN manual script: importing media on Windows (IMP-1 to IMP-5)

What only a Windows machine can check about the 0.2.7 Import: the editing copy made by the Media
Foundation hardware encoder, the Windows file picker, drag and drop from Explorer, **Open with**, a file
named on the command line of a second launch, the Recycle Bin, "access denied" and "open in another
program", and the installer's per-user Open with entries. The macOS script is `IMP-1.md` (same sheet,
rows, cards and editor; read it for what the sheet should look like). The logic is covered by
`cargo test -p cm-media --lib import::` (encoder family, arguments, error mapping),
`cargo test -p cm-desktop --lib instance::` (the second-launch request) and `import_flow::`.
Behaviour: `specs/features/18-import.md`. Status: built, tested with fakes on a Mac, **never run on
Windows** before this script is run. Nothing here is "verified" until a report is stored in
`verification/` (machine, Windows build, app build, GPU and driver).

Use a second instance on its own data (`CAPTUREMANN_LIBRARY=C:\lab\imp-lib`, `CAPTUREMANN_CONFIG_DIR=C:\lab\imp-cfg`;
never a library you care about) from the CI zip or the installed build. The zip's FFmpeg has no `libx264`
and no `lavfi`, so **make the fixtures on a Mac or Linux machine** with a full `ffmpeg` and copy them over
(`scripts/testlab/lab.sh push windows <files>`). They are synthetic; none is a recording of a person:

```
ffmpeg -f lavfi -i testsrc=size=1920x1080:rate=30:duration=20 -f lavfi -i sine=frequency=440:duration=20 -c:v libx264 -pix_fmt yuv420p -c:a aac a.mp4
ffmpeg -display_rotation 90 -i a.mp4 -c copy phone-rotated.mov
ffmpeg -f lavfi -i testsrc=size=1280x720:rate=30:duration=20 -vf "select=not(eq(mod(n\,4)\,1))" -fps_mode vfr -c:v libx264 vfr.mkv
ffmpeg -f lavfi -i "sine=frequency=440:duration=15" memo.wav
ffmpeg -f lavfi -i "sine=frequency=440:duration=3" no.flac
ffmpeg -f lavfi -i testsrc=size=1280x720:rate=30:duration=20 -c:v libx265 -tag:v hvc1 hevc.mp4   # HEVC source
```

1. **Open the sheet.** Library header **Import…** (the chord shown is `Ctrl+I`), `Ctrl+I`, the tray menu
   item **Import file…**, and the launcher command **Import media**. Expected: the Windows file picker opens
   for files only (a folder cannot be chosen); then the sheet. Cancel the picker: nothing opens.
2. **Rows.** Choose `a.mp4`, `phone-rotated.mov`, `vfr.mkv`, `memo.wav`, `no.flac`. Expected as in IMP-1
   step 2: `a.mp4` Ready; `phone-rotated.mov` and `vfr.mkv` Prepared for editing; `memo.wav` Ready;
   `no.flac` Can't import ("isn't supported yet").
3. **Import and the editing copy.** Import the prepared files. Expected: both finish; the library shows
   them with the Imported chip; opening one plays it upright and at a steady frame rate; the bundle holds
   `screen.edit.mp4` and the original as `tracks.screen.master`. Note which encoder ran
   (`cm-daemon` log: `h264_mf`) and the GPU and driver of the machine. `hevc.mp4` is also prepared as H.264
   (the Windows copy is always H.264).
4. **No hardware encoder.** On a machine or VM with no hardware H.264 encoder (Settings of the VM: no GPU;
   or Device Manager: disable the display adapter), import `phone-rotated.mov`. Expected: the file fails
   with `unsupported_format`, `data.reason_detail: "no_hardware_encoder"`, message "This computer can't
   prepare this file for editing (no hardware video encoder)." and **no** `screen.edit.mp4` is left; Microsoft's
   software encoder is not used (`-hw_encoding 1`). If the encoder starts anyway, the copy was made in software:
   record that as a failure.
5. **Drag and drop from Explorer.** Drag `a.mp4`, `memo.wav` and a folder over the window without
   releasing: the dashed overlay shows "2 files" and "1 folder skipped". Release: the sheet opens with two
   rows and the note "1 folder skipped". Try dragging from a network share and from a OneDrive file that is
   online-only; say what happens.
6. **Open with.** Install with the installer (the component **Offer CaptureMann in the Open with menu...**
   is ticked). Right-click `a.mp4` > Open with: CaptureMann is listed, and the default program is unchanged.
   Choose it with CaptureMann **closed**: the app starts and the sheet opens with the file. Choose it again for
   `memo.wav` with the app **running**: no second window, no second tray icon, the sheet gains the row.
   Select three files and choose Open with: say how many sheets and rows result (Explorer may start one
   process per file).
7. **Command line.** `cm-desktop.exe "C:\lab\a b.mp4"` starts the app with the file in the sheet; run it again
   with another file while the app runs: the file is added. `cm-desktop.exe --launcher` still opens the launcher;
   `cm-desktop.exe nothing-here.mp4` (no such file) only shows the app.
8. **Access denied.** Remove your account's read permission from a copy of `a.mp4` (Properties > Security) and
   try to import it, and again with a file in a folder under **Controlled folder access**. Expected:
   `permission_missing` with `data.permission: "files"` and the Windows wording (Controlled folder access, the
   file picker), nothing imported. Then open `a.mp4` in a program that locks it (for example copy it
   while a script holds it open with `[IO.File]::Open(..., 'Open', 'Read', 'None')`): the row says the file is
   open in another program.
9. **Remove the originals.** Import with "Remove the originals after importing" ticked: each original goes to
   the **Recycle Bin** after its copy is verified (restore one from the Bin to prove it). A failed file is
   never trashed. On a USB stick or a network drive without a Recycle Bin the capture is kept, the original
   stays and the result says the move failed.
10. **Uninstall.** Run the uninstaller: the Open with entries are gone (`reg query
   HKCU\Software\Classes\Applications\cm-desktop.exe` finds nothing) and no file association of yours changed.
11. **Long paths and names.** Import a file whose path is over 260 characters and one with accented and
    Vietnamese letters in its name; say what happens.

Pass criteria: steps 1 to 3, 5 to 7 and 9 behave as described; step 4 never produces a software-encoded
copy; step 8 gives the Windows wording; step 10 leaves no entry behind. Store the report with the machine,
the Windows build, the app build, the GPU and driver, the FFmpeg build line, and the steps not run.
