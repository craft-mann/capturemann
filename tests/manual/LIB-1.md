# LIB-1 manual script: library card context menu, filter chips and the remote-copy question on Delete

Covers the parts of LIB-1 (AC4, AC5) that need a screen, plus the Uploaded and Published chips and the
"also delete the copies online" question of Delete (checks 8 to 14; they use your own Google Drive and YouTube
accounts, so use throwaway files and a private video). The daemon side (`rename_capture`,
`delete_capture`, `busy`, events) is tested with fakes in `crates/cm-daemon/tests/library_ops.rs`.
Status of the author's runs: **not run by a human yet.**

## Setup
- A library with at least one screenshot and one recording (use a scratch library:
  `CAPTUREMANN_LIBRARY=/tmp/cm-lib-test`). Open the library window.

## Checks
1. **Menu opens, card does not.** Right-click a card (or use its menu button). Expected: the context menu
   shows Open, Rename, Reveal in Finder, Delete; the detail view does NOT open, and clicking the card
   area behind the menu while it is up does nothing (the menu occludes mouse events). Esc or a click
   outside closes the menu.
2. **Rename does not navigate.** Choose Rename. Expected: the rename box opens with the current title
   filled in and focused; the library stays on the grid (it must not jump to the detail view). Type
   "K8s probes", press Enter (or "Rename"). Expected: the card shows the new title; in Finder the
   bundle folder is named `K8s probes.cmshot` or `.cmrec` (a suffix if that name is taken). Cancel
   and Esc close the box and change nothing. An empty title does nothing.
3. **Rename survives a restart.** Quit and start the app: the title and folder are still the new ones.
4. **Delete confirms.** Choose Delete. Expected: a confirmation names the capture and says it moves to the
   Trash, with the normal primary button. Cancel keeps the capture. Confirm: the card
   disappears and the bundle is in the Finder Trash (restorable with "Put Back"), not deleted for good.
5. **Delete busy.** Start a recording and try to delete or rename that recording from the library:
   expected a plain "busy" message, nothing changes. Same for a capture with an export running.
6. **Open and Reveal.** Open opens the capture in its viewer. Reveal in Finder selects the bundle folder.
7. **CLI cross-check.** `capturemann library rename <id> "New name"` should update the open library window
   (note whether it needs a restart); `capturemann library rm <id>` asks in the app (Confirm), and on Allow the card
   disappears and the bundle is in the Trash. `capturemann library rm <id> --also-remote` asks with the
   remote copies listed (see checks 11 to 14). `capturemann library list --status uploaded` (and `published`,
   `not_uploaded`, `not_published`) prints the same captures as the chips (checks 8 to 10).

## Filter chips and remote copies (checks 8 to 14)

Setup for these: a scratch library with three recordings, A, B and C, and Drive and YouTube connected in
Settings (Destinations and Publishing). Upload A's export to Drive and its original bundle to Drive
(Upload to...), and publish A to YouTube as Private. Upload only B's export to Drive. Leave C alone.

8. **Chips and counts.** Open the library. Expected: the chips read All, Recordings, Screenshots, Uploaded 2,
   Published 1 (counts of the whole library). Click Uploaded: only A and B stay. Click Published: only A.
   Click Published again: it turns off. Click All: every chip filter is off and all captures show.
9. **Chips combine.** Click Uploaded, then Recordings: only recordings that are uploaded. Type part of A's
   title in the search box: only A (or nothing if the text does not match). Clear the search. A chip with no
   match (for example Published after deleting A's video record, or on a library with no uploads) shows
   "No matches / Nothing in this filter yet." with a Clear search button that resets the chips.
10. **Chips follow changes.** With Uploaded on, upload C's export: after the upload finishes C appears in the list
    (or after a moment/refocus) and the count reads 3. Quit and start the app: the counts are the same (the
    index rebuilds them from the bundles). Deleting the `.capturemann` folder in the library and starting
    again gives the same counts after the rescan.
11. **Delete asks the remote question.** Right-click A, Delete. Expected: the confirmation lists the capture and one
    "Online copy" row for each of: the Drive file, the Drive folder of the original, the YouTube video (title,
    link, channel), and says the copies stay online unless the box is ticked. The box reads "Also delete the
    copies on Google Drive / YouTube (permanent)" and is OFF. Cancel keeps everything.
12. **Delete without the tick.** Confirm with the box off. Expected: A disappears from the library and is in the
    Trash; the toast says "Capture deleted" and nothing about online copies. Check Drive (the file and the
    folder are still there) and YouTube Studio (the video is still there).
    Put A back from the Trash (Finder, Put Back) and rescan: its uploads and publish record are still in
    the capture (the Uploaded and Published chips count it again).
13. **Delete with the tick.** Delete A again with the box ticked. Expected: the toast says it also deleted the
    online copies for good; A is in the Trash. Check Drive (file and folder gone, and not in the Drive Trash)
    and YouTube Studio (the video is gone). A capture with no online copy (C before check 10) shows
    today's single confirmation with no box. B (a Drive export only) shows only the Drive row and the label
    "Also delete the copies on Google Drive (permanent)".
14. **Failure keeps the capture.** Put A back with its records, or use a new capture D with a Drive upload and a
    YouTube video. In your Google account settings (Security, Third-party access) remove CaptureMann's access
    so the tokens are revoked. Delete D with the box ticked. Expected: the capture stays in the library; a
    toast that stays until dismissed says "Some online copies weren't deleted", names what was deleted
    online (if anything) and what was not, and says to reconnect the account in Settings. Reconnect the
    account and delete again with the box ticked: only the copies that are left are tried, and then D goes to
    the Trash. Also try with the network off: the toast says to check the connection and the capture stays.
