# QA 0.3.4 B1 delete a capture with a paused upload on Windows (NOT RUN)

- **Date:** 2026-11-15
- **Who:** Claude (agent)
- **Result:** not run on the laptop.
- **Reason:** the case needs an upload that is paused, which needs a configured Google Drive destination (an OAuth sign-in through a browser with a person's account). The isolated lab profile has none and no account was available to an unattended run. The overnight run did not fake a destination.
- **What exists instead:** unit and daemon tests of the fix are in the repository (`cargo test -p cm-daemon` upload and delete tests; see `docs/project/status.md`); no stored report from a real Drive on any system.
- **How to run it:** connect a Drive account in Settings, start an upload of a recording, pause it from Jobs, then delete the capture from the library; expect the delete to go through (the job is cancelled), not "blocked by a paused upload".

## Verdict

NOT RUN
