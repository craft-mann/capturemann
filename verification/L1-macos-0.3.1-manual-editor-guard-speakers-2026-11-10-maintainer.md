# macOS 0.3.1 manual checks: instant editor, outro guard, speaker abort fix, partial playback, Vietnamese strings

- **Date:** 2026-11-10. **Status: PARTIAL** (reported by the maintainer, one Mac, one run each; written down by Claude from the maintainer's message, no measurements of its own).
- **Build:** dev bundle `CaptureMann-dev.app` made from `27541c7d` (`main`, same as `public/main`), signed "CaptureMann Local Dev", installed in `~/Applications` after the previous copy was quit.
- **Computer:** Apple M4 Pro, macOS 26.6.2.
- **Privacy:** the recordings are private; nothing of their content is written here.

| Check | Script | Result (as reported) |
|---|---|---|
| Editor opens at once, sound prepared in the background ("Preparing the sound... N %"), on the 4 h 13 min import | `tests/manual/VOC-2.md`, VOC-2.AC6 to AC10 | **Approved** |
| Outro guard: the invented "subscribe cho kenh" type lines are left out of the regenerated Nov 9 Vietnamese transcript | `tests/manual/AI-GUARD.md`, AI-1.AC12 to AC14 | **Verified** |
| Speakers on the long file (automatic count), no "unexpected quit" | speaker abort fix `dc62692e` | **OK** (one run: "did not recur", not "fixed") |
| Partial transcript shown in playback | `tests/manual/TRN-PROGRESS.md`, AI-1.AC11 | **OK** |
| Longer Vietnamese strings in the export popover and the reviewed wording | `docs/development/release-0.3.1-vi-review-confirmed.md` | **OK** |

## Not covered

- The number of lines the guard dropped, the opening time of the editor in seconds and the speaker-job memory were not recorded in this run (the `capturemann diagnostics` "Last runs" line would give the first).
- The default of `ai.drop_unsupported_lines` is still the maintainer's decision.
- One run each, on one Mac; Windows and Linux were not part of it.
