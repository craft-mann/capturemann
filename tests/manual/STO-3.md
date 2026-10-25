# STO-3 manual script: automatic upload after recording or export, hold, sleep and Retry

Covers STO-3 AC1 to AC4 and the sleep/wake edge case. The queueing, the dependency on the export,
the hold during recording and the retry rules are tested with fakes (`crates/cm-daemon/tests/auto_upload.rs`,
`recording_engine.rs`, `cm-jobs`). This script is for what needs a real recording, a real network and a
real laptop. Status of the author's runs: **not run by a human yet.**

## Privacy and safety
- Steps 1 to 4 record the screen for a few seconds. Record a plain window or an empty desktop
  with nothing private visible, keep each take under 20 seconds, no camera and no microphone unless
  step 4 asks for them. Delete the recordings and the uploaded files afterwards.
- Use a throwaway Google account or a scratch folder on a disk as the destination. A local
  folder is enough for steps 1 to 4 and 7; Drive is needed for steps 5 and 6.
- Do not run this on a machine you cannot sleep or disconnect.

## Setup
- The release bundle, a scratch library (`CAPTUREMANN_LIBRARY=~/cm-sto3-lib`), and one destination
  (a folder on disk, or the connected Google Drive from STO-2). In Settings, Destinations, open Manage
  on it: "Upload automatically" is Off to start.

## Checks
1. **After recording (AC1).** Set "Upload automatically" to "After recording". Record 10 seconds and
   stop. Expected: without touching anything an upload is queued within 2 seconds of the stop
   (`capturemann jobs list --type upload` shows it; the details page shows "Uploading..."); the
   export is made first when the destination takes the export. Note the seconds from Stop to the job
   appearing. A discarded take must not upload anything.
2. **After export (AC2).** Set it to "After export". Record 10 seconds, stop. Expected: the upload
   starts only after the export has finished (the job shows as waiting on the export). Then break the
   export: set `CM_FFMPEG` to a missing path, start the app, record, stop. Expected: the upload is
   not run; it fails with the reason "dependency ... failed" (a note, not a mystery); fix `CM_FFMPEG`,
   retry the export, and the upload follows.
3. **Hold while recording (AC3).** Use a 300 MB or larger export so the upload runs for a while.
   While it runs (Drive, so it takes time), start a new recording. Expected: within 2 seconds the
   upload pauses (the details page stops moving); it resumes after Stop and continues from where it
   was. Turn "Pause uploads while recording" off (`capturemann settings set` is refused for agents;
   change it in `settings.json` with the app closed: `"storage.pause_uploads_while_recording": false`)
   and repeat: the upload keeps running. Put the setting back.
4. **Retry after failures (AC4).** Make uploads fail: for a local folder remove write permission
   (`chmod a-w`); for Drive turn the network off. Record and stop. Expected: a 4xx-type failure
   (permission denied) stops with a readable reason and the toast "Upload to <name> failed" with Retry;
   a network failure retries with backoff by itself (first retry after about 1 s, growing to about
   5 minutes) and shows no failure toast until the attempts run out (20 of them; you do not have to
   wait: check `capturemann jobs list` shows the attempt count rising). Restore the cause and press
   Retry on the toast (or `capturemann jobs retry <job id>`). Expected: the job completes.
5. **Sleep and wake.** Start an upload of a large export to Drive. Close the lid (or `pmset
   sleepnow`) at about 40 percent, wait a minute, wake the machine. Expected: the upload resumes by
   itself after the network is back, continuing from the last chunk, and completes; no duplicate file
   in Drive (check the folder).
6. **Restart (supports STO-2.AC4).** Quit the app during an upload and start it again. Expected: it
   continues; the toast appears once, when it completes.
7. **No double upload.** Stop two recordings quickly one after the other. Expected: one upload per
   recording; the same capture is never uploaded twice to the same destination for the same item.
8. **Clean up.** Set "Upload automatically" back to Off, delete the copies, the recordings and the
   scratch library.

## Pass criteria
- AC1: step 1 (job queued within 2 s, no user action; nothing for a discarded take).
- AC2: step 2 (starts after the export, skipped with the dependency note when it fails).
- AC3: step 3 (pause within 2 s, resume after stop; no pause when the setting is off).
- AC4: step 4 (failure keeps the job in the list with Retry; Retry completes it).
- Edge case: step 5.

## Report
Write `verification/STO-3-<date>-<who>.md` from the template in `verification/README.md`. Record the
seconds in steps 1 and 3, the attempt counts seen in step 4, the destination kind, and how long the
machine slept in step 5. Anything you could not run is `skipped`.
