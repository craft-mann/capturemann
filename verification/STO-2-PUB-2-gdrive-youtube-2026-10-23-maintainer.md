# STO-2 (Google Drive) and PUB-1/PUB-2 (YouTube) report, first real-account run

Reported by the maintainer in conversation, nothing else stored: no screenshots, no job output, no video
ids. The wording in the specs is therefore "verified on hardware (as reported)", not "verified".

- **Date:** 2026-10-23
- **Who:** the maintainer
- **Machine:** MacBook Pro, Apple M4 Pro, macOS 26 (as in CLAUDE.md; not re-read for this run)
- **Build:** release bundle installed in `~/Applications/CaptureMann.app`, git `a4028dc`
- **Accounts:** the maintainer's own Google account, own Google Cloud project, one Desktop-app OAuth client
  for Google Drive and a separate one for YouTube (as the setup panels instruct)
- **FFmpeg:** the bundled LGPL build in the app bundle (not re-read for this run)
- **Fixture:** none; a real recording made in the app

## Steps

| Step | Result | Notes (as reported) |
| --- | --- | --- |
| Google Drive: guided setup, client ID and secret, browser sign-in | PASS | "connected successfully"; the account shows as a `google_drive` destination (`dest_google_drive`, sends export) |
| YouTube: guided setup, client ID and secret, browser sign-in | PASS | "connected successfully" |
| Setup panels display (the step text wrapped inside the panel) | PASS after fix | overflow found on the first look, fixed in `a4028dc`, "display issue fixed" |
| Record, then upload the video to Google Drive | PASS | "work OK so far"; not reported: whether it was automatic or by `upload`, file size, whether the share link opened logged out |
| Record, then publish the video to YouTube | PASS | "work OK so far". Library record of the first upload: privacy requested `private`, reported `private`, status `processing` (a lock would not show). Second upload, asked as **Unlisted**: Studio showed **Unlisted**, so no private lock for this project (reported by the maintainer; the consent-screen status and a mail from YouTube were not reported) |

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| STO-2.AC1 (connect Google Drive with the user's client ID; account shows; "CaptureMann" folder exists) | PASS as reported for connect; the folder was not reported | maintainer message |
| STO-2.AC2 (a large export uploads with matching size) | not run (size not reported; no 2 GB file) | |
| STO-2.AC3 (network drop at 40% resumes) | not run | |
| STO-2.AC4 (app restart mid-upload resumes) | not run | |
| STO-2.AC5 (revoked token shows Reconnect, job waits as `auth_expired`) | not run | |
| STO-4.AC1 (share link opens in a logged-out browser) | not reported | |
| PUB-1 (connect YouTube with the user's own credentials) | PASS as reported | maintainer message |
| PUB-2.AC1 (publish with the sheet fields) | PASS as reported; privacy as chosen: Unlisted stayed Unlisted | maintainer message, library record |
| PUB-2.AC3 (resume after a drop) | not run | |

## Problems found

- The setup steps ran past the right edge of the panel and were cut off (Google Drive and YouTube
  panels). Fixed in `a4028dc` (the step text needed a minimum width of 0 to wrap).
- Spotlight did not index `target/bundle/CaptureMann.app`; a copy in `~/Applications` is used instead.

## Open questions this run can settle

- ANSWERED (maintainer, same day): an Unlisted request came back Unlisted, so videos from this unverified project are
  not locked to private. Public was not tried; the code keeps its warning for the case that YouTube does keep a
  video private.
- Did the Drive share link open for a logged-out viewer, and does the "CaptureMann" folder exist?

## Verdict

PARTIAL: connect and one upload to each service pass as reported, and the YouTube privacy question is answered
(Unlisted stayed Unlisted); the resume, restart, revoked-token, large-file and logged-out-link criteria are not run.
