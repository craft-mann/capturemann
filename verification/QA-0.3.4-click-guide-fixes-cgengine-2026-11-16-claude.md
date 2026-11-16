# QA-0.3.4 Click Guide engine, library and export fixes (cg-engine) report

- **Date:** 2026-11-16
- **Who:** Claude (agent `cg-engine`)
- **Machine:** the maintainer's Mac (Apple Silicon, macOS), no screen, camera or microphone used
- **Build:** worktree of `main` at `0e73b3c2` plus the changes of this report, debug test builds (`cargo test --no-default-features --locked`); the `png` crates and their deflate and hash dependencies are optimised in the dev profile (see Cargo.toml)
- **FFmpeg:** not used
- **Fixture:** none. Scripted clicks (`ScriptedClicks`), the fake screen (`FakeBackend`, a 2880 x 1800 noise-and-flat "screen" for the burst), scripted text readers (a 150 ms reader, a stalled reader, a counting reader)

This is a run with fakes: nothing was seen on screen, no real click, window move, text engine or
confirmation dialog was involved. Nothing here says "verified".

## Steps

| # | Defect | Result | Notes |
| --- | --- | --- | --- |
| 1 | Markdown export overwrites, live links in titles | PASS with fakes | `cg_markdown_export_never_overwrites_and_writes_titles_as_plain_text`, unit tests `markdown_titles_and_notes_are_plain_text` and `a_markdown_export_never_writes_over_what_is_there` |
| 2 | Titles from clicked text: noise and private text | PASS with fakes | `cg_titles_*` (3 tests), `titles::tests` (label rules); setting off reads nothing |
| 3 | A fast burst keeps about 10 % | PASS with fakes (numbers below) | `cg_five_clicks_a_second_are_all_kept`, `cg_a_burst_of_ten_clicks_a_second_keeps_most`, `cg_a_stalled_text_reader_does_not_stop_the_steps` |
| 4 | A moved followed window | PASS with fakes | `cg_a_moved_window_still_gets_the_ring_on_the_clicked_place` (failed before the fix: x 80, y 68 instead of 30, 28) |
| 5 | Cap toast "..", "via MCP" for the CLI | PASS in unit tests, not seen on screen | `toast::tests::agent_toast_says_the_command_line_when_the_client_is_the_cli`; the full stop is in the accessible label of the toast only (`ui/toast_window.rs`) |
| 6 | Confirmation shows the target as `{..}` | PASS with fakes | `cg_the_confirmation_names_the_target_in_words`, `cg_the_ask_first_dialog_names_the_target_in_words`; the dialog itself was not opened |
| 7 | `annotate` right after `import` answers `not_found` | PASS with fakes (failed before the fix) | `annotate_right_after_import_waits_for_the_import_job` |

### Burst numbers (debug test build, 2880 x 1800 frame, text reader 150 ms, other jobs running on the machine)

| | Before | After |
| --- | --- | --- |
| 60 clicks, 100 ms apart | 12 kept (20 %), 0.6 steps/s written | 60 kept, 6.6 steps/s written |
| 40 clicks, 200 ms apart (5 per second) | not run before | 40 kept, 4.2 steps/s |
| PNG of one 2880 x 1800 frame, default encoder | 0.7 to 1.1 s, 2.19 MB | 0.19 s, 2.04 MB (`Balanced` deflate with the `Sub` filter; `Fast` was 7 MB and not faster) |
| step added to the guide (bundle, hash, save) | 55 ms at step 1, 280 ms at step 20 (all earlier steps were rewritten on each click that moved the guide crop) | 30 to 100 ms, not growing with the number of steps |

What made it slow, in order: the PNG encode of the whole frame on the one worker thread; reading the text of the whole frame on the same thread; rewriting the crop of every earlier step on every click that moved the guide crop; and the `guide.json` rewrite. The memory bound `MAX_PENDING_FRAMES` (8 whole frames waiting) and the 500-step cap are unchanged; the existing cap and memory tests (`cg_9_*`, `cg_10_*`) pass.

## Acceptance criteria

| AC | Result | Evidence |
| --- | --- | --- |
| GDE-1.AC1 (titles name the clicked text) | PASS with fakes | `gde_1_ac1_*` unchanged |
| GDE-1 (one crop for the whole guide) | PASS with fakes, now after the stop | `gde_1_every_step_has_the_same_crop_*`: the earlier steps take the final crop when the recording ends, not on every click |
| GDE-3.AC1 (relative links, no absolute path) | PASS with fakes | `gde_3_markdown_export_*` |
| GDE-3.AC2 (a blur is in every export) | PASS with fakes | unchanged test |

## Problems found

- `cargo test -p cm-daemon --test long_shot cap_6_hud_state_pause_resume_and_manifest_source` fails ("paused: no new frame", left 1, right 2) with these changes and also with the dev-profile change taken out. Long Shot code is not touched by this work; I did not run it on a clean checkout, so it may be older. Reported, not fixed.
- CG-13 (the library thumbnail of a display guide is the uncropped frame) is not in this brief and was not changed.

## Not checked

- Nothing on screen: the real click source on any system, a real moved window, the real text engines (the label rules were written against the QA text `ROW 0006) EEE` and general shapes, not tuned to an engine), the real dialog, the toasts.
- Windows and Linux, a second monitor, mixed DPI.
- The Settings page has no switch for `guide.titles_from_screen` (see the report to the orchestrator): the key works when the app writes it.
- A 500-step burst on real hardware (the numbers above are 60 clicks on a debug build with fakes).

## Verdict

PARTIAL (fakes only; items 1 to 4, 6 and 7 are covered by tests that ran, item 5 by a unit test and a label change not looked at on screen).
