# STO-2 manual script: Google Drive (own Desktop-app client)

Covers STO-2 AC1 to AC5 for Google Drive, the only cloud drive offered in v0.1 (OneDrive and Dropbox
say "Coming soon"; AC6 is checked only if you re-enable that provider in a dev build). The protocol
is tested against in-process fake servers (`cargo test -p cm-storage`); nothing in CI talks to
Google. The first real-account run (connect and one upload) is in
`verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-maintainer.md`. Status of the full script:
**AC2 to AC5 not run by a human yet.**

## Privacy and safety
- Use a Google account and a Google Cloud project you own. A throwaway account is best. The
  app asks only for the `drive.file` scope: it can see only files it created.
- The client ID and secret you paste go to the system keychain, never to a file. Do not paste them
  into the report or a screenshot. Remove the client and the app grant from your Google account when
  you are done (https://myaccount.google.com/permissions).
- Use a fixture recording (`make_fixture`, see below), not a real one. Delete the uploaded files from
  Drive afterwards, and empty the Drive trash.
- Step 7 uploads about 2 GB. Check your Drive quota first.

## Setup
- Scratch library and a fixture: `cargo run -p cm-daemon --example make_fixture -- --library
  ~/cm-sto2-lib --secs 30`. For step 7 make a big file: record the fixture longer
  (`--secs 1500`) or export it at a high bitrate until the export is 2 GB; note the size in bytes.
- The release bundle in `~/Applications` (a signed build keeps its permissions), pointed at the
  scratch library with `CAPTUREMANN_LIBRARY`.
- A way to cut the network: Wi-Fi off in the menu bar, or Network Link Conditioner.

## Checks
1. **Connect (AC1).** Settings, Destinations, Connect on Google Drive. Follow the guided steps in
   the panel (create a project, enable the Drive API, an OAuth client of type Desktop app, paste the
   client ID and secret). Sign in in the browser. Expected: the account email shows on the row, and
   the Drive web page shows a folder "CaptureMann" (and no other folders or files were touched).
   Check the keychain contains the client secret and that `settings.json` and `destinations.json` do
   not (`grep -c secret ~/Library/Application\ Support/CaptureMann/*.json` gives 0 for each).
2. **Test connection.** Press Test connection. Expected: "Works: a small test file was uploaded and
   deleted." Check Drive: no leftover test file (also not in the trash of the CaptureMann folder).
3. **Upload (AC1).** `capturemann upload <capture id> --to <drive destination id>` (or the Upload
   action). Expected: progress on the recording's details page ("Uploading to Google Drive: n%"),
   then a toast "Uploaded to Google Drive" with Copy link and Open, and with the setting "Copy the
   link when an upload finishes" on (default) the toast says the link was copied (STO-4). The file
   is in the CaptureMann folder.
4. **Resume after a network drop at 40 percent (AC3).** Use a 300 MB or larger export so there is
   time. Start the upload; when the details page shows about 40 percent, turn the network off for
   a minute. Expected: the upload does not fail; the job retries with backoff. Turn the network on.
   Expected: the percentage continues from about 40, not from 0. Note the percentage before the
   drop and the first percentage after. If you can, watch the sent bytes (Activity Monitor, Network
   tab) to confirm the first 40 percent is not sent again.
5. **Restart mid-upload (AC4).** Start another upload of the same size. At about 30 percent quit
   the app (Cmd+Q) or force-quit it. Start the app again. Expected: within a few seconds after launch
   the job continues (details page shows progress from about 30 percent) and finishes; the file in
   Drive has the right size and plays.
6. **Revoked token (AC5).** In https://myaccount.google.com/permissions remove "CaptureMann" (your
   client's name). Start an upload. Expected: no silent failure and no endless retry. The Drive row in
   Settings shows Reconnect, one toast "Reconnect needed for Google Drive" appears (once), and the
   job is waiting, not failed (`capturemann jobs list --type upload` shows `paused`, reason
   `auth_expired`). Press Reconnect, sign in again. Expected: the waiting job continues by itself
   and completes, without pressing Retry.
7. **A large file (AC2).** Upload the roughly 2 GB export. Expected: progress, and the remote size
   equals the local size in bytes (Drive web page, Details, or `ls -l` against the file info). Note
   the time taken. Play the file from Drive.
8. **Quota (edge case).** If you have a nearly full account: upload more than the free space.
   Expected: the job fails with `platform_rejected` and Google's message, no retry loop.
9. **Disconnect.** Disconnect Google Drive. Expected: the keychain entries are gone and the row
   says Not connected; the files already on Drive stay.
10. **Clean up.** Delete the files and the folder on Drive and empty the trash, remove the app grant,
    delete the scratch library.

## Pass criteria
- AC1: steps 1 and 3 (email, folder, a first upload; no secret in the JSON files).
- AC2: step 7 (progress shown, remote size equal to local size).
- AC3: step 4 (continues from the confirmed chunk, not from 0).
- AC4: step 5.
- AC5: step 6 (Reconnect, one toast, job waits, continues after sign-in).

## Report
Write `verification/STO-2-<date>-<who>.md` from the template in `verification/README.md`. Record the
percentage before and after the drop (step 4), the file sizes in bytes in step 7, and what the job list
showed in step 6. Do not paste client IDs, secrets, tokens or file ids. Anything you could not run is
`skipped`.
