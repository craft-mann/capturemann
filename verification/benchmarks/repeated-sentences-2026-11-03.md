# A short sentence repeated again and again (0.2.7, M2d, the real meeting's loops)

- **Date:** 2026-11-03. **Status:** measured; the real meeting (private, 45 minutes, Vietnamese) is read in place and only counts, times and levels are quoted. Not "verified". Apple M4 Pro, Large v3 Turbo.
- **No phrase appears in the code of this fix.** The rule works on folded token sequences, counts, time and audio only; its tests use made-up tokens (`blorp zan.`, `qux.`, `zed nim.`), Chinese (`好的。`) and Vietnamese-looking words. Proof: `git diff main -- crates | grep '^+'` contains none of "cảm", "thank", "ơn", "cam on" (0 matches each), and no word list or language-specific string was added.

## 1. What the real meeting holds (stored transcript of the maintainer's re-run, before the fix)

`loop_probe` (`crates/cm-ai/examples/loop_probe.rs`) finds sentence units (a unit ends at a word with `. ? ! …` or after 6 words; units are compared with case and punctuation folded) repeated 3 or more times in a row, and the same unit 5 or more times within a minute with other lines between.

| Where | Units in a row | Words | Span | Words per second | Level under it (raw mic) | Speaker engine under it |
|---|---|---|---|---|---|---|
| 3:21 | 16 (1 word each) | 16 | 26.6 s | 0.6 | -37 dBFS | 15.2 s |
| 3:53 | 9 (1 word each) | 9 | 14.5 s | 0.6 | -29 dBFS | 9.5 s |
| 13:23 | 7 (5 words each) | 35 | 9.0 s | 3.9 | -37 dBFS | 5.7 s |
| 16:57 | 3 (6 words each) | 18 | 0.9 s | **19.8** | -29 dBFS | 0.9 s |

Four runs, 78 words, 51 s, 31 s of it under speaker-engine speech; **60 more words** in units that come back 5 or more times within a minute with other lines between them. The audio is not silent (-29 to -37 dBFS) and real speech is under it, so a level check alone does not tell a loop from a repeat; the last run is also physically impossible (18 words in 0.9 s). The existing loop checks never see these: `find_runs` needs 8 identical words (the full stops and line splits do not matter to it, but the runs are 3 to 16), and `is_degenerate` needs a quarter of a piece or 40 words.

## 2. The rule

In the piece repair (`Job::repair`), after each piece: words in a sentence run of 3 or more, or in a unit that occurs 5 or more times within 60 s, are suspect. Each stretch of suspects (with 1 s around it) is read again **on its own, with no earlier text**; then:
1. the second reading has no sentence run of 3: it **replaces** the stretch (silence included: nothing is dropped unheard);
2. the second reading repeats the same unit about as often (within one), and the first reading has at most 6 repeats at no more than 7 words per second: **the repeat was said and stays** (a "yeah yeah yeah" read the same way twice survives);
3. otherwise the repeated words are removed and the span is written as a **sound** (`[noise]`, upgraded to `[unclear]` where the speaker engine heard speech) with its time; the neighbouring words stay.

## 3. Results

| Real meeting | Runs of 3+ in a row | Words in them | Words in 5+ in a minute | Sounds |
|---|---|---|---|---|
| stored transcript (before) | 4 | 78 | 60 | 1 (4.9 s) |
| transcribed again with the fix (same Turbo, Vietnamese) | 1 | 18 | 0 | 0 |

The one run left is the 6-word sentence said 3 times at 5 words per second (-41 dBFS, 0.6 s of speaker engine under it): both readings agree on it, so it stays. The new transcript has 7,749 words against 8,212 stored; **the two are not the same run** (the maintainer's used the app's language list and its own timing; the loops did not reappear in this run without the fix either: with the repair switched off the same audio gave 2 runs and 8 dense words), so the drop of 463 words is not all the fix. Words dropped by the fix alone are not separable on a run that does not loop.

**Whisper's own safeguards on the real stretches** (the audio around each of the four stored runs, 80 to 120 s each, read alone with the plain decoder; looped runs of 3+ / words in dense repeats):

| Setting | 3:21 | 3:53 | 13:23 | 16:57 |
|---|---|---|---|---|
| defaults | 0 / 156 | 1 run (24 words) / 0 | 1 run (18) / 0 | 0 / 0 |
| temperature fallback off | 0 / 264 | 1 (24) / 0 | 1 (30) / 36 | 0 / 0 |
| entropy threshold 1.5 | 0 / 156 | 1 (24) / 0 | 1 (18) / 0 | 0 / 0 |
| average log-probability threshold -0.5 | 0 / 156 | 1 (24) / 0 | **0 / 0** | 0 / 0 |
| no-speech threshold 0.3 | 0 / 156 | 1 (24) / 0 | 1 (18) / 0 | 0 / 0 |
| all three stricter | 0 / 156 | 1 (24) / 0 | 0 / 0 | 0 / 0 |

The loops come back when the stretch is read alone (so a re-read of the span can loop again: rule 3 handles it); switching the temperature fallback off makes them worse; only the log-probability threshold removed one of the four and none removed the 156-word dense repeat or the 24-word run. **No setting replaces the structural rule.** One real stretch, four cases, one run each: this says the thresholds are not enough, not which value is best.

**Public and synthetic rows must not change:** Turbo on the FLEURS clips `s_u1..4`, `viu_a/b/c/h` (WER 35.9, 4.5, 2.1, 6.1 / 10.3, 7.4, 13.9, 10.8 %, coverage and calls identical to the earlier table) and the synthetic English `syn4` (6.2 %), `syn6` (6.1 %): the repair found nothing to re-read (0 repairs, 0 words dropped) on any of them. Unit tests: a made-up sentence repeated five times is replaced; a Chinese phrase really said four times survives; repeated words without full stops (Vietnamese-looking) are not touched; a loop that survives the second reading becomes a sound and its words go; a sentence coming back six times in a minute with lines between is read again; the run finder sees repeats across punctuation and case.

## 4. What this does not show
One real meeting, one language; the fix was not run on its stretches in context with the maintainer's exact settings (the loops did not reproduce in my run); the text of the replacement readings was not read by anyone; the 1-hour row was not re-run after this change.
