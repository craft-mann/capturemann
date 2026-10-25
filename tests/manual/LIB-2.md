# LIB-2 manual script: the library watcher (bundles changed outside the app)

Covers LIB-1 "Edge cases" (bundles added by copying into the library folder appear without a manual
rescan), LIB-1.AC3 (the index is only a cache of the folders) and the parts of LIB-2.AC2 and AC3
that concern an unplugged and reconnected library drive. The state machine, settling rules, own-write
suppression and the unavailable-folder rule are tested with fakes in
`crates/cm-daemon/tests/library_watch.rs`. Status of the author's runs: **not run by a human yet.**

The watcher polls every 2 s and acts on a bundle after it has stopped changing for 1 s, so allow
about 3 s for each change to show up (a malformed manifest: about 13 s).

## Setup
- A scratch library, never your real one: `CAPTUREMANN_LIBRARY=/tmp/cm-lib-watch` for the app
  (`make run-bundle` or `cargo run -p cm-desktop`). Make two screenshots so it is not empty.
- A second scratch library with other captures to copy from (`CAPTUREMANN_LIBRARY=/tmp/cm-lib-src`
  with `cm-daemon` and `capturemann capture screenshot`, or the synthetic
  `cargo run -p cm-daemon --example make_fixture -- --library /tmp/cm-lib-src`).
- Open the library window and keep it visible and in the foreground for the checks (the app's
  focus rescan must not be what makes things appear: do not click away and back).
- For check 6, an external drive or a disk image that holds a library (`hdiutil create -size 200m
  -fs APFS -volname CMDrive /tmp/cmdrive.dmg`, then `CAPTUREMANN_LIBRARY=/Volumes/CMDrive/Lib`).

## Checks
1. **Copy in (LIB-1 edge case).** In Finder, copy a `.cmshot` folder from the source library into the
   scratch library folder. Expected: within about 3 s its card appears in the grid, the sidebar count
   goes up by one, no "Rescan library" was needed. The scroll position and the selected card do not
   change and nothing flickers (the existing cards do not blank out).
2. **Big copy settles.** Copy a `.cmrec` with a large screen track. Expected: no card appears while
   Finder is still copying (or, if it appears because the manifest landed first, it appears once and
   its size is not stuck at the partial value a few seconds after the copy ends); one card at the end.
3. **Delete (LIB-1 edge case).** In Finder, move a bundle in the scratch library to the Trash.
   Expected: within about 3 s the card is gone and the count goes down; the rest of the grid keeps its
   scroll position and selection (if the selected card was the one deleted, the selection moves
   to a neighbour). Put Back from the Trash: it reappears within about 3 s.
4. **Rename.** In Finder, rename a bundle folder (`A.cmshot` to `B.cmshot`). Expected: still one card
   for that capture (same title, it does not vanish and reappear as a new one); opening it works.
5. **Edit outside.** Open a `manifest.json` in a text editor, change the `"title"`, save. Expected:
   the card shows the new title within about 3 s.
6. **Unplug and re-plug (LIB-2.AC2, AC3 as far as the watcher).** With the app running on the drive
   library, eject the drive. Expected: no error dialogs, no flood of messages in the terminal (one
   line "the library folder is unavailable; watching paused"), the cards that were listed are not
   wiped by the watcher. Re-attach the drive (`hdiutil attach /tmp/cmdrive.dmg`) after adding a
   bundle to it and deleting another from a second Mac or the Finder. Expected: one line "reachable
   again", and within about 3 s the added card appears and the deleted one goes.
7. **Unfinished recording is ignored.** Start a recording in the app (a short one). While it runs
   the new `.cmrec` folder must not show as a card; after Stop it appears once. In a copy of that
   folder with a `recording.lock` file inside, copied into the library, no card appears until the
   lock file is removed.
8. **Own exports do not loop.** Record 10 s, wait for the automatic export. Expected: the card gets
   its export once (no repeated reloads: the terminal stays quiet, CPU near idle in Activity Monitor
   while the library window is idle).
9. **Rescan still works.** Use the "Rescan library" menu item after a manual change: it completes and
   the grid is correct.
10. **Headless daemon.** Quit the app, run `cm-daemon --library /tmp/cm-lib-watch` and, in another
    terminal, a JSON-RPC client subscribed to `capture.*` with `events.subscribe` (no CLI command
    streams capture events yet; `capturemann mcp` clients get them as log notifications). Copy a
    bundle in. Expected: a `capture.created` event with `by: "watcher"` and `external: true`, and
    `capturemann library list` lists it.

## Pass criteria
- LIB-1 edge case (copy-in, delete, rename and manual edit appear without a rescan): checks 1 to 5, 10.
- LIB-1.AC3 (the folders, not the index, are the truth): check 9 and the app still opens every card.
- LIB-2.AC2 and AC3, library unavailable and reconnected, watcher side: check 6.
- Not a regression: checks 7 and 8.

## Report
Copy the template from `verification/README.md` into `verification/LIB-2-<date>.md`: machine and
macOS version, build (bundle or `cargo run`), pass or fail per check with the latency you saw
(seconds from the Finder action to the grid), anything that flickered or lost the scroll position or
selection, and the terminal output of check 6.
