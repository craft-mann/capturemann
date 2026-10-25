# PUB-2 manual script: publish to YouTube with every field of the sheet

Covers what the fake YouTube server cannot: what a real channel does with the upload. Validation
and its wording, the made-for-kids gate, the form-to-request mapping, the Confirm dialog's rows, the
resumable upload and resume, the thumbnail, playlist and captions steps as warnings, the quota
counter and the privacy-locked result are covered by `cargo test -p cm-publish -p cm-daemon -p
cm-desktop` against fakes (PUB-2.AC1 to AC4 as far as a provider can meet them). This script also
**settles the open question about unverified projects** (answered for the owner's project on Oct 23, 2026: an Unlisted request stayed Unlisted; Public was not tried; record what you see anyway): `specs/03-integrations.md` says "unverified
projects are not restricted to private" (a current reference), while the code treats a video that
comes back private as a warning. Write down what you observe in the table of step 3. Do not guess
and do not copy the spec's sentence: only a real upload answers it.

Needs: a YouTube channel you may fill with test videos, connected as in [`PUB-1.md`](PUB-1.md)
(your own Google Cloud project, **left unverified**; note whether the consent screen is in Testing
or in production), the release bundle or `cargo run -p cm-desktop` with a throw-away config folder,
FFmpeg (the bundled one in the bundle). Use the synthetic recording, no screen, camera or
microphone:

```sh
cargo run -p cm-daemon --example make_fixture -- --library <empty dir> --secs 60 --no-pause
```

Open the app on that library (`CAPTUREMANN_LIBRARY=<dir>`), so your real library is never touched.
Run the same steps once more with a 2 GB or longer export for the resume step.

1. **The sheet.** Open the fixture recording → **Publish**. Without an export: "This recording has
   no export yet", an **Export MP4** button, Publish disabled with "Export the recording first".
   Export; the card shows the file name, preset, size and length. The page lists your channel
   (with a switch between channels when several are connected, the default one selected) and, when the
   daemon reports it, a line "N of 100 uploads used today".
2. **Every field.** Fill in: a title of your own (the counter reads "N of 100 characters"); a
   **description of several lines** (type Enter between lines, paste text with blank lines and a
   long line: it wraps to the width, the field grows to a few rows and then scrolls with a visible
   scrollbar, the arrow keys move between lines and the caret stays in view); tags `demo, CaptureMann`;
   a **playlist** (an ID such as `PL…` or the playlist's address); visibility; the kids answer; a
   thumbnail (step 6); **Upload captions (SRT) with the video** (only offered when the recording
   has captions: generate them first with the AI page, and check that the switch is **absent** on a
   recording without captions).
3. **Privacy chosen against privacy applied (the open question).** Publish once per row and read
   the privacy in YouTube Studio (Content → the video → Visibility) **and** on the job result
   ("Uploaded to YouTube" or "Uploaded to YouTube, but kept private" with the warning). Fill the table:

   | Chosen in the sheet | Public confirmation | Applied by YouTube (Studio) | Result text in CaptureMann | Consent screen state |
   | --- | --- | --- | --- | --- |
   | Private | n/a | | | |
   | Unlisted | n/a | | | |
   | Public | switch on | | | |

   Expected by the code: whatever YouTube applies is reported truthfully; a video kept private when
   Unlisted or Public was chosen shows "Uploaded to YouTube, but kept private" and a plain-words
   warning, and CaptureMann **never** changes the video's visibility afterwards (check that Studio
   shows no edit made by the app). Record the facts, including "YouTube applied what was chosen"
   if that is what happens: that answers the spec question for this project state.
4. **Made for kids.** With the question unanswered, Publish is disabled and the line under the
   button says "Answer "Is this video made for kids?" to publish." There is no default; after
   answering Yes, comments are off in Studio for that video.
5. **Public needs one more step.** Choose Public: Publish stays disabled until the switch "Confirm
   that everyone can see this video" is on; changing the visibility withdraws it.
6. **Thumbnail.** (a) Drag the **time slider**: the time label ("0:23 of 1:00") follows the pointer
   and the picture changes to the frame there a moment after you stop (or at once on release); the
   arrow keys move one percent, Shift ten percent, Home and End the ends; the slider takes the focus
   with Tab. The last frame is never asked for (end margin). (b) **Choose image…** with a JPG, then a
   PNG, then a 3 MB file (refused on the thumbnail with the 2 MB message), then **Remove**. (c) Publish
   with a frame thumbnail: on a channel **without** a verified phone YouTube refuses custom
   thumbnails: expected "Thumbnail needs a verified YouTube account" as a **warning**, the video is
   published (AC2). On a verified channel the thumbnail is the picked frame.
7. **Captions.** With the switch on and a recording with captions, publish. Expected: the Confirm dialog
   lists "captions: Uploads the captions as an SRT track"; afterwards Studio → Subtitles shows an
   uploaded track, with the text of the captions as they were at the moment you confirmed. If YouTube
   refuses, the video is still published and a warning says so.
8. **Playlist.** With a playlist ID (and again with its address): the video is in the playlist in
   Studio. With a wrong ID the sheet marks the field ("Not a playlist id") and disables Publish; with a
   well-formed ID that is not yours, the video is published and the warning says the playlist step failed.
9. **Confirm dialog contents.** Press Publish: nothing is sent yet; a separate dialog lists the
   capture, platform, **account (channel)**, title, privacy, made for kids, file name and size, the
   description (clipped), tags, thumbnail file name, captions, "Uploads this video over the internet
   to YouTube, channel "…"", "N of 100 used today", and, for Unlisted or Public, the note that YouTube
   may keep videos from an unverified project private. Decline: no job starts, no upload (check the
   channel). Let it time out (5 minutes): same. Note whether the **playlist** appears in the dialog (it
   is sent with the request; the dialog lists what the daemon prepares).
10. **Progress and leaving the page.** After Confirm, the sheet shows "Uploading to YouTube N%" with a bar;
    leave the page and come back: the upload continues and the result appears.
11. **Resume after a network drop (AC3).** Use the 2 GB export. At about 30 percent turn Wi-Fi off for a
    minute, then on. Expected: the job retries by itself (or shows a failure with **Retry**); the
    final upload does not start from zero (watch the progress or the network meter: roughly the same
    bytes in total as the file size plus the dropped chunk). Also quit the app mid-upload and start it
    again: the job resumes or offers Retry. The finished video plays in Studio in full.
12. **Quota text.** The line on the sheet and in the dialog reads "N of 100 uploads used today"
    (N increases by one per finished upload; the day is Pacific time). If you can reach the limit (use
    a quota-limited project or edit `publish_quota.json` in the config folder to 100 for today), the
    sheet disables Publish with "The upload limit for today is used up; try again after midnight Pacific
    time."; restore the file afterwards.
13. **A privacy-locked result.** If any row of step 3 came back private against the choice, check the
    result block: the title reads "Uploaded to YouTube, but kept private", the link opens (Open and Copy
    link work), a warning in plain words says YouTube kept it private, and the library record is
    unchanged. If nothing was locked in your project, say so in the report; the case stays covered by
    the fake only.
14. **The record.** The recording's `manifest.json` has one `publishes[]` entry per upload, with the URL.

Pass criteria: steps 1, 2, 4 to 10 and 12 pass; the table of step 3 is filled in; a resumed upload
finishes without starting over; nothing is sent before the dialog is confirmed; no video's visibility
is ever changed by the app after the upload.

## Privacy

Uploads the synthetic fixture to YouTube over the internet: use a private test channel and delete
the videos afterwards (Studio → Content → Delete). No camera, microphone or screen is used. The
config folder holds no secret; the keychain holds the sign-in.

## Report

`verification/PUB-2-<date>-<who>.md` from the template in
[`../../verification/README.md`](../../verification/README.md): the table of step 3 verbatim, which
steps ran, the consent-screen state (Testing or in production), and the YouTube behaviour you saw for
each privacy. When the table settles the question, update `specs/03-integrations.md` (the "Unaudited
/ BYO behavior" row) in the same commit.

## Status of the author's runs

(none yet; the first real publish on Oct 23, 2026 is in
`verification/STO-2-PUB-2-gdrive-youtube-2026-10-23-owner.md`, as reported: it worked, and the
privacy chosen and applied were not recorded)
