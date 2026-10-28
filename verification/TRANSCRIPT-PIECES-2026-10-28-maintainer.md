# TRANSCRIPT-PIECES report (piece-by-piece transcription, loop check, resume)

- **Date:** 2026-10-28
- **Who:** maintainer (results as reported in the session), numbers from the stored files by Claude
- **Machine:** MacBook Pro, Apple M4 Pro, 48 GB, macOS 26.x
- **Build:** dev bundle `CaptureMann-dev.app`, commits `955cba4` (piece runner, checkpoints) and `16d6b6b` (the last run, after a quit and reinstall)
- **FFmpeg:** the bundled LGPL build
- **Recording:** one audio recording, 88 minutes, Vietnamese, one microphone (`rec_06GR16YE5SHVEMRC`), model Large v3 (`large`), language auto

No script in `tests/manual/` covers this yet; this report is the record of one real run. One recording is one data point: no word error rate was measured and no reference text exists.

## What went wrong before (same recording)

- Large v3, one run over the whole file (build before `ae06db6`): the phrase "Đã, trả lại cho mình đâu rồi?" appears 2,480 times from 5:35 to the end, about 96% of 18,121 words.
- Turbo, one run: 14,590 words, 28.6% of words under 0.5 confidence, 6 short repeated runs (worst 6).

## Steps

| Step | Result | Notes |
|---|---|---|
| Re-generate with Large v3 (pieces, loop check, glossary per piece) | PASS (as reported) | Job `job_6eda3e1542e5615d`, `done`, 16,727 words, language `vi`, last word at 87.9 min |
| The loop at about 5:35 is gone | PASS (as reported, and in the file) | 0 occurrences of the old phrase |
| Any piece still looping after splitting | not detected (as reported) | Two short repeated runs in the whole file, worst 5 (a group of 3 words said 5 times); no collapse was needed that anyone saw |
| Glossary terms late in the recording | "quite good" (as reported) | Not measured; no list of terms was counted |
| Quality against the earlier Turbo run | "much better" (as reported) | Low-confidence words 18.8% against 28.6%, mean confidence 0.763 against 0.692 (different models: not strictly comparable) |
| Steady word rate over the recording | PASS (in the file) | 1,554 / 2,005 / 1,942 / 1,982 / 2,196 / 1,860 / 1,526 / 2,112 / 1,550 words per 10 minutes |
| Held by a new recording, resumes instead of restarting | PASS (observed in the job table, not a planned run) | The first run restarted from 0% at 15:12 (before checkpoints). On `955cba4` the daemon saved a 56 KB, later 875 KB, checkpoint; after the quit and reinstall the last run started at 15:52:36 and finished at 15:56:33 (4 minutes for the last part, which fits resuming at about 85%; this is an inference, the progress it started from was not read) |
| Progress visible on the Transcript tab for a job the daemon picked up | not confirmed | Fixed in `16d6b6b`; the maintainer did not report seeing it on screen |

## Problems found

- Whisper loops: found and fixed in this change set (above).
- The run was interrupted three times, so there is no time for an uninterrupted run. From the job table: it started 14:59:52 and reached 7x% by 15:12, when a new recording held the queue and the build had no checkpoint, so it restarted from 0%. It reached 75% by 15:39, then went back to the queue when the app was quit and the new build installed; that run restarted from 0% as well (9% at 15:42), because the interrupted run was by a build that saved nothing. The run on the build with checkpoints reached 67% at 15:49; after the quit and reinstall the final run started 15:52:36 and finished 15:56:33.
- The glossary and the 5 minute piece length were not tuned; nothing in this run shows they need to be.

## Verdict

PASS as reported for one recording on Large v3. Not covered: screen recordings (same code path, not run), Turbo and the smaller models with pieces, Intel, recordings between 10 and 20 minutes, and any measured accuracy.
