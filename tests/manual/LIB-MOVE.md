# LIB-MOVE manual script: change the library folder (LIB-2.AC5 to AC13)

Covers Settings -> General -> Library folder -> **Change...**, the `move_library` action, its job
`library_move`, the restart prompt and the optional Trash step. The logic (checks, copy, verify,
resume, cancel, Trash step, the Confirm tier) is tested with fakes and temp folders in
`crates/cm-daemon/tests/library_move.rs` and the unit tests of `crates/cm-daemon/src/library_move.rs`
and `apps/desktop/src/library_move.rs`; **nothing here was run on a real external drive or seen by
eye.** Status of the author's runs: **not run by a human yet.**

## Setup (never your real library or settings)
- A scratch config folder and library: `CAPTUREMANN_CONFIG_DIR=/tmp/cm-move-cfg` and **no**
  `CAPTUREMANN_LIBRARY` (that variable beats the setting, so the move would not take effect). Put
  the starting library in `/tmp/cm-move-cfg` by running the app once, then quit it, or pre-set
  `{"library.path": "/tmp/cm-move-src"}` in `/tmp/cm-move-cfg/settings.json`.
- Fill the source library with about 2 GB of captures: the synthetic recording is enough
  (`cargo run -p cm-daemon --example make_fixture -- --library /tmp/cm-move-src --secs 60`, run it a
  few times) plus a few screenshots.
- An external drive (or a disk image: `hdiutil create -size 8g -fs APFS -volname CMBig /tmp/cmbig.dmg`
  and open it) called `CMBig`, and a small one (`-size 300m`, `CMSmall`).
- Start the app from the bundle or `cargo run -p cm-desktop` with the variables above, open
  Settings -> General.

## Checks
1. **The row (board R11).** The Library group shows "Library folder", the path with your home as `~`,
   `<size> used · <size> free` (first "Calculating..." for a moment, decimal MB/GB), the Available pill
   and a **Change...** button. Open another page and come back within 60 s: no new "Calculating...".
   Pass: the numbers match Finder's "Get Info" (decimal GB) within a few percent.
2. **Move 2 GB to an external drive (AC5).** Change... -> choose `/Volumes/CMBig/CaptureMann`. The
   question offers "Move my captures to the new folder" (default), the tick box "Start with an empty
   library there instead" and Cancel; its rows show From, To, Size and Free space. Press Move. Pass:
   the row turns into "Moving the library" with a progress bar that climbs, Cancel is there, and the
   old library still opens and records meanwhile. When it ends a dialog says "The library moved to
   /Volumes/CMBig/CaptureMann. Restart CaptureMann to use it." and the row offers Restart CaptureMann
   and "Move the old copies to the Trash...". `/tmp/cm-move-src` is untouched (same files).
3. **Restart and see everything intact.** Press Restart CaptureMann. Pass: the app reopens on the new
   library (Settings shows the new path), every capture is there, recordings play, an export runs, and
   `capturemann library list --json` (with the same `CAPTUREMANN_CONFIG_DIR`) lists them all.
4. **Unplug mid-copy (AC4, AC11).** Move back to a new folder on `CMBig` and eject the drive while the
   bar is moving. Pass: the row says "The move stopped" with the old library still in use, a toast
   says nothing was deleted, no capture is missing from the old library, no error dialog loops. Plug
   the drive in and press **Retry**: it carries on and finishes; captures that were already copied are
   not copied again (watch the drive's activity, or the progress starting above 0).
5. **Cancel (AC12).** Start another move and press Cancel at about half way. Pass: "The move was
   cancelled"; the old library is still the active one (Settings shows the old path after a restart);
   the new folder holds the captures copied so far, each complete, and no `*.cm-copying` folder.
6. **Use that library (AC9).** Change... -> the folder from check 2 (it holds captures). Pass: the
   question says "This folder already holds a CaptureMann library with N captures" and offers "Use that
   library" (no move, no tick box). Accepting asks for a restart and copies nothing.
7. **Start with an empty library (AC10).** Change... -> an empty folder -> tick "Start with an empty
   library there instead" -> Move. Pass: nothing is copied, the dialog says the old files stay where
   they are, and after the restart the library is empty and the old folder is untouched.
8. **Not enough space (AC8).** Change... -> `/Volumes/CMSmall/Lib` with a library bigger than the disk
   image. Pass: the question says there is not enough room (needs X free, has Y) and only offers
   "Start with an empty library there"; nothing is copied. Also try: a folder inside the library, the
   library's parent, the library itself, a file, a read-only folder: each shows its own sentence and
   "Choose another folder...".
9. **Busy (AC8).** Start a recording (or an export, or an upload), then Change... -> any folder. Pass:
   "Not right now" with a sentence that names what is running and a **Check again** button; nothing was
   cancelled. Stop the recording, press Check again: the question appears.
10. **Trash of the old copies (AC13).** After check 2 (before or after the restart; before it, the
   button is in the Library row), press "Move the old copies to the Trash...". Pass: its own
   confirmation says how many captures and that they can be restored; afterwards the Trash holds those
   bundle folders, the new copies are untouched, a toast says how many moved. Edit one old capture
   first (rename its title in a text editor on `manifest.json`): that one stays and the toast says one
   capture stayed because it changed. Nothing is ever deleted permanently.
11. **CLI and agent confirmation (AC6).** With the app running: `capturemann library move
    /Volumes/CMBig/Agent --wait`. Pass: the confirmation dialog appears (agent chip, tool
    `move_library`, rows From, To, Size, Captures, Result, Restart), the CLI waits for it; declining
    exits 3 and nothing is copied; approving prints the progress and ends with "Restart CaptureMann to
    use the new folder". `capturemann library move ... --fresh` and `--use-existing` behave like checks
    7 and 6. Run the same through an MCP client (`move_library`): the same dialog. A second call while
    one runs gets `busy` with `reason: "move_running"`.
12. **Agents cannot write the setting.** `capturemann settings set library.path /tmp/x` is refused
    (`app_only_setting`). In Settings -> Agents & CLI the permission "Change the library folder" shows
    **Ask first** and cannot be switched to Allow.
13. **Vietnamese.** Switch the language (Settings -> General -> Language) and repeat checks 1, 2 and
    8: no clipped or overlapping text in the row, the question, the buttons or the toasts.

## Cleanup
Delete `/tmp/cm-move-*`, eject and delete the disk images.
