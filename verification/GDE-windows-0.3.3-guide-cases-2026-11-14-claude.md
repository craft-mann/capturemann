# GDE report: cm-regress guide cases on Windows (Track D2)

- **Date:** 2026-11-14
- **Who:** Claude (automated, cm-regress), no person has read it
- **Machine:** ThinkPad T490, Intel UHD 620, Windows 11 Pro 10.0.26200, display at 144 DPI (150 %)
- **Build:** branch worktree-agent-a226151c9ec9bcf8e at 15da091d (debug build of cm-daemon, capturemann, cm-mcp, cm-regress, `--no-default-features`, in `C:\wt_d2`); not the installed app
- **FFmpeg:** not found (the cases need none)
- **Command:** `cm-regress --tier hardware --only guide.edit_export,guide.refusals,guide.real_clicks --allow screen --package-dir C:\wt_d2\target\debug`, in the desktop session

## Steps

| Case | Result | Notes |
| --- | --- | --- |
| `guide.edit_export` | PASS | hand-written guide: get, reorder, retitle, undo, redo; Markdown, HTML, PDF export |
| `guide.refusals` | PASS | not_found, refused numbering and format, a start on a display that does not exist leaves no guide |
| `guide.real_clicks` | not run | `guide start` through the CLI answered `user_declined / no_confirmer`: start_guide is a Confirm tool and the headless daemon has no confirmation UI. The case was changed afterwards to end SKIPPED with that reason (not re-run on the laptop). |

## Not done

- No real Click Guide run with the desktop app (bar, clicks on the bar at 150 %, drag reorder, card): the app was not built or launched here. Section 4 of `tests/manual/GDE.md` is still open on Windows, including the exclusion-rectangle check at 125/150 %.

## Verdict

PARTIAL. Two cases passed on one machine; the real-click case and everything on screen were not run.
