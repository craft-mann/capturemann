# QA 0.3.4 recording with a publish record, macOS pass 5 (PARTIAL, mostly NOT RUN)

- **Date:** 2026-11-15. **Who:** Claude (QA agent, macOS pass 5). Dev bundle of main a4f28a07, isolated library under /tmp/cmq5.

- Fixture built: a synthetic recording (make_fixture, 5 s, no camera) with a hand-edited manifest holding one fake YouTube record (shape from crates/cm-library/src/rec.rs: platform, account_id, account, remote_id, url, title, privacy, privacy_requested, status, warnings, at), copied into the isolated library. No network.
- NOT RUN (paused by the maintainer before I looked at it in the app): Recordings Published and Needs attention chips with the record, card pill/meta/status line, card menu, details Published card and header pill, Vietnamese. Pass 4 results for the chips stand.

Verdict: PARTIAL (not run).
