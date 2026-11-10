# AI-GUARD: lines to leave out (AI-1.AC12, AC13)

Setup: a build with the AI page, a speech model installed, a recording or imported audio file of your own that has a
stretch of silence or room noise (no sound after 22:00: use import, do not record).

1. **Empty by default.** Open the AI page. The "Lines to leave out" list says "No phrases yet. Nothing is left out unless you add one." The switch
   "Leave out lines the audio does not hold" is on.
2. **Add a phrase.** Type a part of a line from a transcript you have, in other capitals and without accents, press Enter or
   "Add a phrase". It appears as a row. Typing the same letters again says "That phrase is already in the list."; typing two letters says
   "Type at least three letters first." Remove the row with its Remove button and add it again.
3. **Re-generate.** Open that capture, Re-generate the transcript. Pass: the lines that contain the phrase are gone from the
   new transcript; the old transcript stays if the run fails. In a terminal `capturemann ai transcript <id>` still works and
   `capturemann settings get ai.ignore_phrases` prints the list; `capturemann settings set ai.ignore_phrases ...` is refused (app-only).
4. **Other captures are not touched.** Open a transcript made earlier: it still has its lines.
5. **The shape switch.** Switch "Leave out lines the audio does not hold" off, Re-generate a recording that had a dropped line:
   the line is back. On again, Re-generate: it goes again. Look at `transcript.json` in the capture's folder: `dropped` lists
   the time ranges (start, end, words, reason); the job result shows `lines_dropped` / `lines_ignored`.
6. **Layout.** Repeat step 2 once with `CAPTUREMANN_PSEUDO_LOCALE=1` at the smallest window width: the rows wrap, nothing is cut.

Record the result in `verification/` (template in its README) with the platform, the build and which steps were run.
