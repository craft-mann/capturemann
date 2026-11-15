# QA 0.3.4 Click Guide at 100 and 500 clicks, macOS pass 5 (PARTIAL, stopped by the maintainer)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 5). Dev bundle of main a4f28a07, isolated library under /tmp/cmq5.

| Item | Result | Evidence |
| --- | --- | --- |
| 100-click burst (100 mouse-downs in 11 s from one tool call) | measured | 53 steps (50 with count 2: the OS click count merged pairs into double-clicks), 41 MB bundle; steps kept arriving 3+ s after the last click (38 then 53) |
| 500-click run (520 single clicks 0.45 s apart) | INCOMPLETE and not valid | stopped at 189 steps by the maintainer's request. My click point partly lay over the CaptureMann window and other windows, so step titles took text from other apps' elements (including an account label of another app). That bundle (QA5 guide 500) stays in /tmp/cmq5, not committed. The cap message was never reached |
| Step lag | seen | 68 steps after 3.7 min of clicks at the start, then it caught up (about 1.25 steps per second) |
| Retake, drag-reorder | NOT RUN |

Verdict: PARTIAL.
