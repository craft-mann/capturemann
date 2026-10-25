# STO-4 manual script: share link after upload, link modes and Copy link

Covers STO-4 AC1 to AC3 and the Copy link menu. The link creation per provider is tested against
fake servers, and the decision to copy (setting on or off, no link, clipboard refusing) with a fake
clipboard (`apps/desktop/src/recording/uploads/copy_tests.rs`); the library menu entry and the
details-page buttons are logic-tested in `library.rs` and `published.rs`. This script is for what
needs a real Drive, a real clipboard and a logged-out browser. Status of the author's runs: **not run
by a human yet** (the first real run did not report the link).

## Privacy and safety
- Use a throwaway Google account and a fixture recording (`make_fixture`), never a real recording:
  "Anyone with the link" makes the file readable by anyone who has the URL. Remove the sharing and
  delete the file when you are done (Drive: Share, Remove access or delete; empty the trash).
- Do not paste the share link into a report or a chat. Write "opened" or "did not open".
- A private (incognito) window or a second browser profile that is signed out of Google is the
  logged-out browser.

## Setup
- Google Drive connected as in `STO-2.md`, a scratch library with a fixture recording and its export
  (`cargo run -p cm-daemon --example make_fixture -- --library ~/cm-sto4-lib --secs 20`, then Export
  MP4). Settings, Destinations, Manage on Google Drive: note the "Link after upload" choice
  (Anyone with the link, or No link; People in my organization appears only for work accounts and
  other providers).

## Checks
1. **Anyone with the link (AC1).** Choose "Anyone with the link". Clear the clipboard (copy any other
   text). Upload the export. Expected: when the upload finishes the toast says the link was copied,
   the clipboard holds a Drive link (paste it into a text field to look), and opening it in a
   logged-out private window shows the video player (not a sign-in page or "request access").
2. **Copy link when done off.** In Settings, Destinations, turn off "Copy the link when an upload
   finishes". Copy other text, upload again. Expected: the clipboard is unchanged; the toast still
   offers Copy link and Open; pressing Copy link copies it and shows "Link copied". Turn the setting
   back on. (An agent may change this setting: `capturemann settings set storage.copy_link_when_done
   false` works without a confirmation and is visible in the app.)
3. **No link (AC2).** Choose "No link". Upload. Expected: the toast has no Copy link or Open, and the
   clipboard is unchanged. In Drive, open Share on the new file: it is still Restricted (only you),
   and no permission was added. Compare with the file from step 1, which is "Anyone with the link".
4. **In the library (AC3).** Open the library. Right-click the recording's card. Expected: Copy link
   is in the menu, after Reveal in Finder, and copies the link of the newest upload (check the paste
   is the link of step 3's file if that one had one, else step 2's; step 3's file has no link). A
   capture that was never uploaded, or only copied to a local folder, or uploaded with "No link", has
   no Copy link entry.
5. **Details page.** Open the recording's page. Expected: under Exports one line per finished copy
   ("Google Drive, Video, size, date"), with Copy link and Open on the ones that have a link; Open
   shows the video in the browser.
6. **CLI and agent view (AC3).** `capturemann library list --json` (or the MCP tool `list_library`)
   shows `uploads` for the recording with `destination`, `what`, `share_link` and `at`; the entry
   from step 3 has `share_link: null`. `get_capture` shows the same.
7. **Organization policy (edge case).** With a work account whose admin forbids public links:
   upload with "Anyone with the link". Expected: no failure; the toast says the link is an
   organization link (a note), and the stored link is the organization one. Skip with a note if you
   have no such account.
8. **Clean up.** Remove the files and sharing on Drive, empty the trash, delete the scratch library.

## Pass criteria
- AC1: step 1 (the clipboard holds a link that opens in a logged-out browser).
- AC2: step 3 (no permission added; no link).
- AC3: steps 4 to 6 (Copy link in the menu and on the details page, `uploads` with the link in
  `list_library`).
- The setting: step 2.

## Report
Write `verification/STO-4-<date>-<who>.md` from the template in `verification/README.md`. For each
link mode record only "opened logged out: yes or no" and what the Drive Share dialog showed. Do not
paste links. Anything you could not run is `skipped`.
