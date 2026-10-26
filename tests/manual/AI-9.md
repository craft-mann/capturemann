# AI-9 manual script: summaries and reports with a real local text model

Covers what the scripted tests cannot: a real local model (Ollama or LM Studio) writing real
reports. The schema, citation, chunking, undo and local-only rules are covered by `cargo test -p
cm-ai report` and `cargo test -p cm-daemon --test ai_reports`. Remote and Anthropic providers are
not supported yet (maintainer decision, Oct 26, 2026): this script uses a model on this computer.

Privacy: nothing is recorded. Use the fixture or a recording you already have; the transcript text
goes only to `127.0.0.1`. Watch it with a network monitor (Little Snitch, `nettop -p ollama`) for
step 7.

Setup:

1. Start Ollama (`ollama serve`, a 7B to 9B model pulled, for example `qwen3:8b`) or LM Studio.
2. In the app: AI page, text model: provider Local, the model, context 8192 or more. (Until the AI
   page section exists, put `ai.text.*` in the settings file; `capturemann ai text-model test`
   must print `answered`.)
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
9. **Failed regenerate (AC6).** Stop Ollama, `ai report ID --template minutes --report RPT
   --wait`. Expected: exit 6 (`text_model_unreachable`, "Start Ollama or LM Studio, or choose
   another provider"), and `ai report get ID RPT` still returns the previous report.
10. **Long recording (AC5).** On a 2 to 3 hour recording with a transcript and context 8192:
    the job reports progress, finishes, and the report has citations from the first and last
    tenth of the recording.
11. **Export.** `ai report export ID RPT --format txt --out-dir ~/Desktop` writes a file; running
    it again writes `... (2).txt`, the first is untouched. Delete the files.
12. **Small transcript.** On a recording with 10 spoken words: `Too little speech for a report`.

Pass: steps 1 to 12 behave as written. Quality review (3 meetings, 2 interviews, 1 lecture, in
English and Vietnamese, scored by hand) goes in `verification/` using its template; say "verified"
only with that report.
