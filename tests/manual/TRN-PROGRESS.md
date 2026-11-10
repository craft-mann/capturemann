# TRN-PROGRESS: transcript progress for a recording and an audio capture (0.3.1 section 4)

What it checks: while a transcript of a recording or an audio capture made in the app is being written, the page and the
library card show the same progress as an imported file (stage, percent, a bar, Cancel); the progress is still there when
the page is closed and opened again; a failure shows its reason with Retry. The code was tested with fakes only; nobody has
looked at the screen yet. Run it on macOS, Windows 11 and Ubuntu (one pass each).

## Setup

- A speech model installed (AI page). A library with no important data (use a test library: `CAPTUREMANN_LIBRARY`,
  `CAPTUREMANN_CONFIG_DIR` pointing at empty folders).
- Settings -> AI: "Transcribe after each recording" on for step 3, off for the others.
- A recording of 3 to 5 minutes with the microphone (long enough to watch the bar), and an audio capture of the same length.
  The sound cases need a quiet hour (not after 22:00): record by speaking, play nothing.
- Also set `CAPTUREMANN_PSEUDO_LOCALE=1` once (step 8) and once in Vietnamese (step 9).

## Steps and pass criteria

1. **Recording, started by hand.** Open the recording's details page (library, click the card). Transcript section: press
   Transcribe.
   Pass: the button is replaced at once by one line with the icon, "Transcribing", the percent at the right (mono) and a
   4 px bar under it, with a Cancel button beside it. The percent rises; the bar fills in the accent colour; the same look as
   the strip on an imported file's card, but drawn on the page (not dark). When the stage changes to "Checking the
   transcript" the words change and the percent carries on. At the end the progress disappears and the stored transcript
   (summary, Open captions, Download, Re-generate) shows, and a "Transcript ready" toast appears.
2. **Library card.** While step 1 runs, go back to the library.
   Pass: the card of that recording (a video recording, not an import) has the same strip at the bottom of its thumbnail
   (stage, percent, bar) and no "Imported" badge. It updates by itself and disappears when the transcript is ready.
3. **Automatic run after a recording.** Turn "Transcribe after each recording" on, record 3 minutes with the microphone,
   stop, open the recording's page at once.
   Pass: the page shows the progress as in step 1 without pressing anything (also when the job was still waiting: the
   bar is full and dim, no percent, until the first percent arrives). Its library card shows the strip too.
4. **Audio capture.** Record 3 minutes of audio (Audio recording), stop, open it (the audio editor), Transcript tab.
   Pass: with the automatic run on, or after pressing Transcribe, the tab shows the sentence "The transcript is being
   written on this computer..." and under it the same progress line with Cancel. The library card of the audio capture
   shows the strip over its waveform. The Captions and Report tabs, which also offer Transcribe when there is no
   transcript, show the same progress line instead of the button.
5. **Close and reopen.** During a run, go to the library, then open the recording again; then quit the page by opening
   another recording and coming back; then (once) close the app window from the tray menu path you normally use and
   open it again only if the app keeps running in the tray.
   Pass: each time the page opens, the progress is there with the current percent (not 0 and not missing); the app was
   never told by an event while the page was closed.
6. **Cancel.** Press Cancel during a run.
   Pass: the progress line disappears within a second, the Transcribe button (or, with a transcript, Re-generate) is
   back, no error text and no "Transcript ready" toast. Press Transcribe again: it starts from the beginning. In another
   terminal, `capturemann jobs get <id>` (or `get_job`) shows the first job as `cancelled`.
7. **Error and Retry.** Make a run fail: remove the speech model's file while the job waits (or record 5 seconds of
   silence if the build reports "the audio is silent"), then press Transcribe.
   Pass: the progress line is replaced by the reason in the warning colour and a Retry button. Retry starts a new run
   (the error goes away at once and the progress shows). With a stored transcript, a failed Re-generate shows the reason
   and Retry opens the Re-generate panel (it states what a new run replaces); it does not run at once.
8. **The same job on the CLI and MCP.** During a run: `capturemann jobs list --type transcribe` and
   `capturemann jobs get <id>` (or the MCP tools `list_jobs`, `get_job`).
   Pass: state `running`, `progress` between 0 and 1 rising, `message` "Transcribing N%" (later "Checking the transcript
   N%"). `capturemann jobs cancel <id>` from the terminal ends the run in the app too (the page and the card return to
   idle after the next refresh; reopening the page is enough).
9. **Layout.** With `CAPTUREMANN_PSEUDO_LOCALE=1` and with the language set to Vietnamese, repeat step 1 with the window at
   its smallest width.
   Pass: the stage text shrinks instead of overflowing, the percent and Cancel stay visible, nothing is cut at the card's
   border (the line is at most 420 px wide).
10. **No regression of the import.** Import a video file with "Transcribe" ticked (Cmd/Ctrl+I).
    Pass: the import card's strip and, after the import step, the transcription's strip look as before (dark strip with
    the "Imported" badge).

11. **The first part appears while the rest runs (AI-1.AC10).** Use a recording or audio capture of 25 minutes or more
    (import a long file you own; do not record sound after 22:00). Press Transcribe and open the page (the recording's
    details page, and in a second pass the audio editor's Transcript tab).
    Pass: below the progress strip a banner says "Transcribing: the first N minutes are ready. The rest follows." after
    the first piece (about 5 minutes of audio, a minute or two of waiting); under it the first part reads as rows (a time
    over the text), no edit, download or Re-generate control is offered; every few minutes N and the rows grow without the
    list jumping back to the top (scroll down a little, wait for the next piece: the position stays). In a terminal,
    `capturemann ai transcript <id>` prints "PARTIAL: the first N min of M min are ready" and `capturemann ai captions get
    <id>` says the recording is still being transcribed (exit 6). Look in the capture's folder: `transcript.partial.json`
    exists. Press Cancel: the rows and the banner go and `transcript.partial.json` is deleted. Start again and let it
    finish: the page switches to the final transcript and the file is gone. Start again, start a recording to hold the
    job, and see the first part stay; stop the recording and see the run go on.
    Layout: repeat once with `CAPTUREMANN_PSEUDO_LOCALE=1` at the smallest window width: the banner wraps, nothing is cut.

12. **The words line and row clicks follow the first part (AI-1.AC11).** During step 11, in the audio editor with the first part ready:
    play inside the first minutes, and drag the playhead there.
    Pass: the line above the transport shows the current words, the word being said marked, as it does for a finished
    transcript; past the ready part (move the playhead beyond N minutes) it reads "The words for this part are still being
    made."; the Transcript tab no longer says the transcript "appears here when it's done"; clicking a row moves the
    playhead to its time. When the run finishes the line follows the final transcript; after a Cancel the old hint returns.

Record the result in `verification/` (template in its README) with the platform, the build and which steps were run.
