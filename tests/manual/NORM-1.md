# NORM-1 manual script: check 50 turns of a normalised transcript

Covers what the scripted tests cannot: whether the punctuation, capital letters and paragraph
breaks the built-in text model adds read better than the raw transcript. The word-preserving rule,
the sidecar, the job and the actions are covered by `cargo test -p cm-ai normalize` and
`cargo test -p cm-daemon --test ai_normalize`. This is AI-10; it has no screen in 0.2.7 (the AI
page switch and the Tidy button are 0.2.8), so it runs through the CLI.

Privacy: nothing is recorded. Use a recording you already have, in Vietnamese or English; its text
goes only to `127.0.0.1`. Do not paste its text into an issue or a report: count turns only.

Setup:

1. A text model downloaded and in use (`AI-7.md`; Gemma 4 E2B). `capturemann ai text-model test`
   prints `answered`.
2. A recording with a transcript of at least 200 words, ideally with speaker names
   (`capturemann ai speakers ID`).

Steps:

1. **Run it.** `capturemann ai normalize ID --wait`. Expected: done; the line says how many turns
   were tidied and how many "kept as spoken" (rejected). Note the time: about 1 minute for 15
   minutes of speech on an M-series Mac.
2. **Transcript untouched.** `capturemann ai transcript ID --words` prints the same words as before
   (a copy of the output taken before step 1 matches). Search and captions still work.
3. **Read it.** `capturemann ai normalized ID > /tmp/norm.txt`. Open the text next to
   `capturemann ai transcript ID`.
4. **Score 50 turns.** Take the first 50 turns (a turn is a block separated by a blank line; if
   there are fewer, take all). For each write one of: **better** (punctuation, capitals or breaks
   make it easier to read), **same**, **worse** (a wrong sentence break, a wrong name capital, a
   break in the middle of a thought).
5. **Words.** For 10 of the 50 turns, read the tidied text and the raw text together. Every word,
   every accent and tone mark must be the same. Expected: no difference (the check enforces it). Any
   difference is a bug: note the turn number only.
6. **Kept as spoken.** In `capturemann ai normalized ID --json` look at turns with
   `"normalized": false`. They keep the raw words and carry a `reason`. Expected: under 5 % of turns.
7. **Stale.** Edit one word of the transcript in the app, then run `capturemann ai normalized ID`.
   Expected: the header says the transcript has changed since; run step 1 again and it is gone.
8. **No model.** With the text model removed, `capturemann ai normalize ID` exits 2 with
   `no_text_model`.

Pass: "worse" is under 10 % of the 50 turns, step 5 shows no word difference, step 2 shows no
change. Report the counts (better / same / worse) in `verification/` using the template there.
