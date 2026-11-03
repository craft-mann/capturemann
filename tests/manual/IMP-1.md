# IMP-1 manual script: importing media (boards I1 to I4)

Covers what only a person (and a Mac) can check: how the Import sheet, the drop overlay, the progress
cards and the editor of an imported file look and feel; the macOS Files and Folders prompt; Dock drop
and Open With; a real external drive; the real Trash. The logic is covered by `cargo test -p cm-desktop
--lib import_flow` (rows, footer, request, drop counts, progress cards), `cargo test -p cm-desktop --test
import_request` (the sheet against the real daemon), `cargo test -p cm-daemon --test import`, `cargo test
-p cm-media --test import`, `cargo test -p cm-cli --test cli import` (the engine and the actions with
ffmpeg fixtures). Behaviour: `specs/features/18-import.md` (IMP-1 to IMP-5). Boards:
`capturemann-redesign 6/screens/png/Import*.png`. Surface: `docs/development/design-surfaces.md` 5.12.
Status: built, tested with fakes and fixtures, **never seen by eye** before this script is run.

Set up a second instance on its own data (`CAPTUREMANN_LIBRARY=<dir>` `CAPTUREMANN_CONFIG_DIR=<dir2>`;
never your own library) from a development bundle (`make bundle-dev`; the bundle, not the bare binary,
because the permission prompts belong to the bundle id). Make the fixtures in a scratch folder (the
files are small unless noted; none is a recording of you):

```
ffmpeg -f lavfi -i testsrc=size=1920x1080:rate=30:duration=20 -f lavfi -i sine=frequency=440:duration=20 -c:v libx264 -pix_fmt yuv420p -c:a aac a.mp4
ffmpeg -display_rotation 90 -i a.mp4 -c copy phone-rotated.mov        # a rotated phone-style clip (HEVC: use hevc_videotoolbox instead of libx264)
ffmpeg -f lavfi -i testsrc=size=1280x720:rate=30:duration=20 -vf "select=not(eq(mod(n\,4)\,1))" -fps_mode vfr -c:v libx264 vfr.mkv   # variable frame rate in MKV
ffmpeg -f lavfi -i "sine=frequency=300:duration=3600" -c:a aac interview-1h.m4a                    # about 1 hour
ffmpeg -f lavfi -i "sine=frequency=440:duration=15" memo.wav                                         # a 15 s voice memo
ffmpeg -f lavfi -i "sine=frequency=440:duration=3" no.flac                                           # recognised, not supported yet
```

For the protected file use an iTunes-style `.m4v` you own (FairPlay). Without one, skip step 9 and say so.

1. **Open the sheet three ways.** Library header **Import…** (shows the chord), `⌘I`, menu bar
   **CaptureMann > Import file…**, and the launcher (⌥⇧Space) command **Import media**. Expected: the
   file picker opens (files only: a folder cannot be chosen), then the sheet. Cancel the picker: nothing opens.
2. **Rows.** Choose `a.mp4`, `phone-rotated.mov`, `vfr.mkv`, `interview-1h.m4a`, `memo.wav`, `no.flac`.
   Expected: one row each with an editable title (the file name), the mono meta line (name, duration,
   size in px, codec, audio, size), and a chip: `a.mp4` Ready (green); `phone-rotated.mov` and `vfr.mkv`
   Prepared for editing (blue); the audio files Ready; `no.flac` Can't import (amber) with "This kind of
   file isn't supported yet". A blue note names the first prepared file and why. The heading says "Import
   6 files", the button "Import 5 files". Remove a row with its x: the counts follow. **Add files** adds more.
3. **Formats text.** The drop overlay and the sheet list only "MP4, MOV, M4V, MKV" and "M4A, WAV". WebM, AVI,
   MP3, FLAC are not listed as supported.
4. **Drop overlay.** Drag `a.mp4`, `memo.wav` and a folder over the window (do not release). Expected: a dashed
   full-area zone with the 84 px tile, "Drop to import", "1 video · 1 audio file", "1 folder skipped" and the
   sentence about folders. Release: the sheet opens with two rows (the folder is skipped, with the message in
   the sheet). Drag away without releasing: the overlay goes.
5. **After import.** Transcribe is off by default; turning it on shows nothing else. Find speakers is dimmed
   with "Needs the speaker model" when the speaker models are missing. The **Remove the originals after
   importing** box is unchecked, with the hint "... Frees N GB".
6. **Import (copy).** Tick nothing, press **Import N files** (Enter does the same when the button is enabled, also while typing a title; Esc closes the sheet). Expected: the sheet closes; a toast
   "Importing N files" with a progress bar and **Cancel** stays while it runs; cards appear above the grid with
   the "Imported" badge and a strip (Copying into the library, Preparing for editing for the MKV and the MOV,
   Reading the sound, Checking the copy) with a percentage; then the captures are in the grid with the
   Imported badge, and a toast "Imported N files" closes after 5 s. The originals are untouched. The Imported
   filter chip shows only them, with a count.
7. **Open them.** Open `a.mp4`: the editor shows an **Imported** chip, the source line (date, original path,
   codec, "original kept in the bundle") and **Show original** (Finder shows the master file inside the bundle: `screen.mp4`). The Camera and Cursor tabs are dimmed (35%), with the reason in the tooltip; their panels say so.
   Captions says "No transcript yet" and points to Speakers, Translate and Report. Trim, export and play work;
   the exported file keeps the length (compare with `ffprobe`). Open `vfr.mkv`: it plays smoothly in sync with
   the sound (the editing copy). Open `memo.wav`: it opens the audio editor unchanged.
8. **Cancel.** Import `interview-1h.m4a` and `vfr.mkv`; press Cancel in the toast while it copies. Expected:
   the running file stops, no half capture appears, the finished ones stay, the originals are untouched, a
   toast says "Import cancelled".
9. **Protected file.** Add the `.m4v` with FairPlay. Expected: Can't import, lock icon, "Protected by the
   store it came from (DRM); it can't be opened". It is not counted in the button.
10. **A file on the Desktop.** Put `memo.wav` on the Desktop and choose it with the picker (a picker choice
    grants access). Then drop another file from the Desktop on the window. If macOS asks "CaptureMann would
    like to access files in your Desktop folder", the sentence is the one in Info.plist; Allow: the sheet
    continues. Deny (System Settings > Privacy & Security > Files and Folders): the sheet shows "CaptureMann
    isn't allowed to read these files. Allow it in System Settings..." and nothing is imported. From a terminal,
    `capturemann import ~/Desktop/memo.wav` exits with code 4 and `permission_missing` when it is denied.
11. **An external drive.** Copy `a.mp4` to an external or USB volume and import it from there: the same
    prompt for removable volumes may appear (the text names the library folder); the import works after Allow.
    Pull the drive while a large file copies: that file fails with a reason, nothing partial is in the library.
12. **Folder drop.** Drop only a folder: the overlay shows "1 folder skipped" and the message; after the drop
    no sheet rows appear (an empty sheet is not left open). `capturemann import <folder>` exits 2 with
    `folder_not_supported`.
13. **Disk full.** With a small disk image mounted as the library (or a library on a nearly full volume), choose
    a file larger than the free space: the footer's space line shows the need, the Import button is dimmed, and
    `capturemann import <file>` exits 5 with `disk_full` and the needed and free bytes. Nothing is copied.
14. **Remove the originals.** Tick the box with two copies of a small file: the footer line changes to "Each
    original goes to the Trash after its copy is checked". Press Import: no dialog (it is your own action). After
    the import the originals are in the Trash (restore one from the Trash to be sure it is the same file) and
    the captures are in the library. Repeat with a read-only volume: the capture is kept, the original stays, a
    note says the Trash was not possible. **An agent:** `capturemann import copy.mp4 --remove-originals --wait`
    must ask on screen first; the dialog lists every file that goes to the Trash; Cancel imports nothing.
15. **Dock drop and Open With.** Drag `memo.wav` onto the Dock icon (the app running, then not running): the
    sheet opens with the file. In Finder choose **Open With > CaptureMann** on `a.mp4`: the same. `.avi`, `.mp3`
    and `.flac` are not offered by Open With.
16. **After import (AI).** Import `memo.wav` with Transcribe on and a model installed: the strip then says
    "Transcribing" and the transcript appears in the editor; without a model nothing is started and the import
    is unaffected. With Find speakers on and the speaker models installed, speakers follow.
17. **Long file.** Import `interview-1h.m4a` and note the time and the library size growth (the copy plus
    `mic.wav`, 48 kHz 32-bit float, about 0.7 GB per mono hour). Quit during the import and start the app
    again: the job resumes, no duplicate capture.
18. **Language.** Switch to Tiếng Việt in Settings: the sheet, chips, toast and cards read in Vietnamese
    (drafts, marked fuzzy in `locales/vi.po`); with `CAPTUREMANN_PSEUDO_LOCALE=1` nothing is cut off.
19. **Light theme** and **keyboard**: Tab reaches every control of the sheet; VoiceOver reads the chips and the checkbox; Esc closes the sheet on opening without a click, and Enter
    does nothing while the button is dimmed.

Report in `verification/` (template in its README). Delete the scratch files and anything imported.
