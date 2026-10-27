# UPGRADE-0.2 manual script: a 0.1.0 library opened by 0.2.0

0.2.0 changes the library index (SQLite `user_version` 2 to 3) and adds new bundle content (markers,
speakers, translations, reports, audio recordings). This checks that a library made by 0.1.0 opens,
keeps everything and upgrades without touching the bundles. You work on a copy, never on your real
library.

What has been tried (Oct 27, 2026, tested with the 0.2.0 code on a synthetic 0.1.0 library built from
tag `v0.1.0` with `make_fixture`): the index moved from 2 to 3, the recording was listed, and every
file of the bundle was byte-identical afterwards. **Not covered:** screenshots, transcript-search
rows and a large real library. This script is how to cover them. That run was not stored in
`verification/`; a run of this script should be.

Privacy: the copy holds your real captures. Keep it on your Mac, do not upload it, delete it at the
end. Nothing here sends data anywhere.

Setup:

1. Quit CaptureMann 0.1.0. Find your library (Settings > Library) and copy the whole folder twice:
   `cp -R "<library>" ~/cm-upgrade-test` and `cp -R "<library>" ~/cm-upgrade-ref`. The second is the
   reference for the byte comparison; never open it. Copy the config folder (the CaptureMann folder in
   `~/Library/Application Support`) to `~/cm-upgrade-config`.
2. Before: `sqlite3 ~/cm-upgrade-test/.capturemann/index.sqlite 'pragma user_version'` prints `2`.
   Note the number of recordings and screenshots, two recordings with a transcript, one screenshot,
   one uploaded or published capture, and a word that hits a transcript in search.
3. Fingerprint the reference (everything but the index):
   `cd ~/cm-upgrade-ref && find . -type f -not -path './.capturemann/*' -exec shasum -a 256 {} + | sort -k2 > ~/cm-before.txt`.

Steps:

1. **Open the copy with 0.2.0.** `CAPTUREMANN_LIBRARY=~/cm-upgrade-test
   CAPTUREMANN_CONFIG_DIR=~/cm-upgrade-config
   /Applications/CaptureMann.app/Contents/MacOS/cm-desktop`. Expected: it opens without an error or a
   repair prompt and lists the library.
2. **Counts.** The numbers of recordings and screenshots equal those of setup step 2; the Audio view is
   empty (0.1.0 had no audio recordings) and shows no error.
3. **Details.** Open three recordings and two screenshots: thumbnails, titles, dates, durations, tags,
   the details page (screen recordings and screenshots keep one; only audio recordings have none) and
   the upload and publish states are as in 0.1.0. Play one recording.
4. **Search.** Search the noted word (a title, a tag, a word from a transcript): the same results
   as in 0.1.0. A word that does not exist finds nothing.
5. **Index version.** `sqlite3 ~/cm-upgrade-test/.capturemann/index.sqlite 'pragma user_version'`
   prints `3`. If it prints 2 the app did not migrate; if anything else, stop and keep the copy for
   the maintainer.
6. **Bundles untouched.** Quit the app, fingerprint the copy the same way into `~/cm-after.txt` (run
   inside `~/cm-upgrade-test`) and `diff ~/cm-before.txt ~/cm-after.txt`. Expected: no difference.
   Opening a recording may add cache files; list exactly which, and confirm no master and no
   `manifest.json` that you did not edit changed.
7. **New features on old data.** Transcribe one old screen recording (speech model from the AI page),
   write a report and run Find speakers: both work on a 0.1.0 recording. Undo the report: the
   recording is back as before.
8. **Do not go back.** Do not open the upgraded copy with 0.1.0, and do not open your real library
   with 0.1.0 after 0.2.0 has upgraded it (the older app does not know index version 3).
9. **Clean up.** Delete `~/cm-upgrade-test`, `~/cm-upgrade-ref`, `~/cm-upgrade-config`, and the two
   text files.

Pass: steps 1 to 7 as written, `user_version` 3, no master or unedited `manifest.json` changed, and
nothing lost in counts or search. Write down the library size, the number of captures, the time of the
first open and the macOS version.
