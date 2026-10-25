# STO-1 manual script: local folder, external drive and NAS destinations

Covers the parts of STO-1 (AC1, AC2, AC3 and the disk-full edge case) that need a real volume. The
provider and the job are tested with temp dirs and fakes (`cargo test -p cm-storage`, the `upload`
job tests in `crates/cm-daemon/tests/`). Status of the author's runs: **not run by a human yet.**

## Privacy and safety
- Use a scratch library (`CAPTUREMANN_LIBRARY=~/cm-sto1-lib`) and a short fixture, not a real
  recording: `cargo run -p cm-daemon --example make_fixture -- --library ~/cm-sto1-lib --secs 20`.
  Nothing here uses the screen, camera or microphone.
- Use an empty folder or a spare USB stick or share for the destination. Do not point it at a
  folder with files you care about. Delete the copies and the scratch library afterwards.
- No network is needed except for the NAS or UNC step. No account is used.

## Setup
- A build of the app (`make run-bundle`, or `cargo run -p cm-desktop`) pointed at the scratch library.
  Open Settings, Destinations.
- Note the git commit and the volume names for the report.

## Checks
1. **Add a folder (AC1).** Under "Save to" choose "Add a folder", pick an empty folder, give it a
   name. Expected: the row shows the real path, the state "Online", and the folder pattern
   (default `CaptureMann/{yyyy}/{MM}/{title}`). Set "What to copy" to the export.
2. **Copy an export (AC1).** Open the fixture recording, press Export MP4, then
   `capturemann upload <capture id> --to <destination id>` (the id is in `capturemann destinations`).
   Expected: a toast "Copied to <name>" with Open; Open shows the file in Finder or Explorer at the
   pattern path. `shasum -a 256` of the copy equals the export's (find it under the bundle's
   `exports/`). No file with a temp name is left beside it.
3. **Never overwrites (AC1).** Upload the same export again. Expected: a second file named
   `<title> (2).mp4`; the first is untouched.
4. **External drive (AC1, AC2).** Add a folder on a USB drive. Repeat step 2: it lands on the drive.
   Eject the drive while it is idle. Expected: within about 30 s the row says "Offline" (or on the
   next volume event).
5. **Offline wait and return (AC2).** With the drive still ejected run
   `capturemann upload <id> --to <drive destination>`. Expected: the job does not fail; the jobs
   list (`capturemann jobs list --type upload`) shows it waiting with "Destination offline". Plug
   the drive back in. Expected: within about 30 s the job runs and completes, the toast appears, the
   file is there and its hash matches. Note the seconds from plugging in to completion.
6. **NAS, mounted path (AC2).** Mount an SMB share in Finder (`/Volumes/<share>`), add a folder on
   it, upload. Expected as step 2. Unmount the share mid-idle and repeat step 5.
7. **UNC path (AC3, Windows only; mark not run on macOS).** Add `\\server\share\CaptureMann` as
   the folder. Upload a large export (1 GB or more). While it runs, look at the share from another
   computer: Expected: only a temp name is visible until the end, then the final name appears at
   once; a partial file never has the final name. Pull the network cable at 50 percent: Expected:
   the job retries or waits and ends with the complete file or a clean failure, never a short file
   under the final name.
8. **Disk full (edge case).** Make a small volume (macOS: `hdiutil create -size 20m -fs APFS
   -volname CMFULL ~/cm-full.dmg` and attach it; Windows: a 20 MB VHD). Add it as a destination and
   upload a larger export. Expected: that job fails with `disk_full` and a plain message ("not enough
   space"), no partial file stays, other jobs and the app are unaffected, the destination stays
   usable. Free space (or use a smaller file) and press Retry on the toast: Expected: it completes.
9. **Permission denied (edge case).** Add a folder, then remove write permission
   (`chmod a-w <folder>`). Upload. Expected: the job fails with the operating system's message and
   a Retry button; restore the permission, press Retry: it completes.
10. **Pattern cannot escape (AC1).** Edit the destination's folder pattern to `../../escape/{title}`.
    Expected: the app refuses it or the copy stays below the destination root. Nothing is written
    outside the chosen folder.
11. **Clean up.** Delete the destination copies, detach the disk image, delete the scratch library.

## Pass criteria
- AC1: steps 1 to 3, with matching hashes and no overwrite.
- AC2: steps 5 and 6 (the wait and the return within the polling interval, no failure while offline).
- AC3: step 7, Windows only.
- Edge cases: steps 8 and 9.

## Report
Write `verification/STO-1-<date>-<who>.md` from the template in `verification/README.md`. Record the
volume types used (USB, SMB, UNC), file sizes, the seconds in step 5, and the exact messages of
steps 8 and 9. Steps you could not run (no NAS, no Windows) are `skipped`, not `PASS`.
