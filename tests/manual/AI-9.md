# AI-9 manual script: summaries and reports with a real local text model

Covers what the scripted tests cannot: the built-in text model writing real
reports. The schema, citation, chunking, undo and local-only rules are covered by `cargo test -p
cm-ai report` and `cargo test -p cm-daemon --test ai_reports`. The built-in text model is the only
provider in 0.2.0 (maintainer decision, Oct 27, 2026).

Privacy: nothing is recorded. Use the fixture or a recording you already have; the transcript text
goes only to `127.0.0.1`. Watch it with a network monitor (Little Snitch, `nettop -p llama-server`) for
step 7.

Setup:

1. In the app: AI page, text model: Built in, with a model downloaded (`AI-7.md`).
   `capturemann ai text-model test` must print `answered`.
3. A recording with a transcript of at least 30 words and, for steps 3 and 4, speaker names
   (`capturemann ai speakers ID`, then `rename`). The M4 fixture has no speech; use a short real
   meeting you recorded with consent, or any `.cmrec` with a transcript.

Steps:

1. **Templates.** `capturemann ai report templates`. Expected: six templates; Interview notes shows
   "Notes only — CaptureMann doesn't score candidates. Using AI to assess candidates is regulated
   in some places."
2. **Meeting minutes (AC1, as far as the model goes).** `capturemann ai report ID --template
   minutes --attendee "Linh" --attendee "Minh" --wait`. Expected: done in a few minutes for a
   30-minute recording; `capturemann ai report get ID` prints Markdown with Summary, Attendees,
   Topics, Decisions, Action items, Open questions and Marked moments; every action item has an
   owner from the attendees or speakers, or "Unassigned"; the dropped count is printed.
3. **Citations (AC2).** In the Markdown each item ends with `[mm:ss]` times. Seek the recording to
   five of them (editor, or `ffplay -ss`): the words at that time support the item.
4. **Names.** Rename a speaker (`ai speakers rename ID 2 Linh`) and regenerate with `--report
   RPT`. Expected: the new name appears in owners; the old "Speaker 2" does not.
5. **Interview notes.** `--template interview --role "Support engineer" --competency
   "Customer empathy"`. Expected: question and answer pairs in order, quotes per competency, the
   notes-only line; no score, rating, ranking or hire wording anywhere (`grep -iE
   'score|rating|rank|recommend|hire'` on the exported Markdown finds only the notes-only line's
   own words, if any).
6. **Language (AC7).** `--lang vi` on an English recording. Expected: section text in Vietnamese,
   quotes in English, times unchanged.
7. **Local only.** During steps 2 to 6 the monitor shows traffic only to loopback. Then set the
   provider to a remote one and run step 2 again. Expected: exit 2, `no_text_model`; nothing sent.
8. **Edit and undo (AC6).** `ai report edit ID RPT --file notes.md`, `ai report get ID RPT`
   (edited), `capturemann undo ID`, `ai report get ID RPT`: the generated text is back.
9. **Failed regenerate (AC6).** Stop the engine (quit `llama-server`) and make it unable to start (for example a copy of the bundle without it), `ai report ID --template minutes --report RPT
   --wait`. Expected: exit 6 (`engine_missing` or `engine_failed`, with the message the AI page shows), and `ai report get ID RPT` still returns the previous report.
10. **Long recording (AC5).** On a 2 to 3 hour recording with a transcript and context 8192:
    the job reports progress, finishes, and the report has citations from the first and last
    tenth of the recording.
11. **Export.** `ai report export ID RPT --format txt --out-dir ~/Desktop` writes a file; running
    it again writes `... (2).txt`, the first is untouched. Delete the files.
12. **Small transcript.** On a recording with 10 spoken words: `Too little speech for a report`.

13. **Real citations, checked one by one (AC2).** On the meeting from step 2, take every
    citation in the report (a report of 30 minutes has 20 to 40). For each, seek to the time and
    read the 20 seconds around it. Mark it **supported** (the words say it), **near** (right topic,
    time off by up to 30 s) or **wrong**. Pass when at least 90% are supported or near and none
    cites a time outside the recording. The count of citations the engine dropped (printed by
    step 2) is a good sign, not a failure: write it down. Also read one Decision and one Action
    item and decide whether the transcript really contains them (no invented owners or dates).
14. **The Report tab.** In the app open the same recording (an audio recording uses the editor's
    Report tab, a screen recording says so in its tab): choose the template, press Generate,
    watch progress, then check that each citation chip jumps the playhead to its time, that Edit
    and Undo behave like steps 8 and 9, and that Export writes the Markdown. With the provider
    None the tab says where to set one up (`no_text_model`) and makes no request. Check English,
    Vietnamese and `CAPTUREMANN_PSEUDO_LOCALE=1`. (The tab is built by another change; if it is not
    in your build, mark this step "not present".)

## Review set

AC1 and the quality bar need a set of recordings read by a person. Gather, with consent, and keep
them outside the repo: **3 meetings** (4 to 6 people, 20 to 40 minutes), **2 interviews**
(two people, 20 to 30 minutes), **1 lecture or talk** (30 to 60 minutes), each in English and the
same kinds in Vietnamese if you can (at least one Vietnamese meeting and the lecture). Each needs a
transcript made with the same model (note it) and, if S7 has passed, speaker names.
For each recording and its suggested template (meeting minutes, interview notes, lecture notes),
score on this card and keep it in `verification/`:

| Check | Scale |
| --- | --- |
| Summary says what the recording was about | 0 wrong, 1 partly, 2 right |
| Decisions and action items are real and complete (list the misses) | 0, 1, 2 |
| Owners are right or "Unassigned" | 0, 1, 2 |
| Citations support the item (step 13) | % supported or near |
| Nothing invented (names, numbers, dates) | count of inventions |
| Reads as a usable set of notes in 5 minutes | 0, 1, 2 |

Write the model, the context size, the time each job took and which Mac. These runs also answer
spike S8 (which text model is the default): do not pick a default here, report the scores.

Pass: steps 1 to 12 behave as written, step 13 meets its bar and step 14 passes or is marked "not
present". Quality review (3 meetings, 2 interviews, 1 lecture, in
English and Vietnamese, scored by hand) goes in `verification/` using its template; say "verified"
only with that report.
