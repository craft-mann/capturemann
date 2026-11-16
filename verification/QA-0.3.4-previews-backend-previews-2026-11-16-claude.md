# QA-0.3.4 previews-backend (design S24) report

- **Date:** 2026-11-16
- **Who:** Claude (agent `previews`)
- **Machine:** the maintainer's Mac (macOS, Apple Silicon); no window opened, no screenshot of the live screen
- **Build:** worktree branch off `0e73b3c2`, debug, `--no-default-features`, `cargo test` per crate
- **FFmpeg:** not used
- **Fixture:** fake screen (`FakeBackend`), scripted click source, in-memory and file settings stores

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| Registry `cm_core::features` (ids, keys, tools, default on, stable = never disabled) | PASS (tests with fakes) | 4 unit tests |
| Settings keys `previews.*`: default on, app-only (agent refused with `app_only_setting`, app and file store accept) | PASS (tests with fakes) | `engines.rs`, `backend.rs`, `tests/preview_switch.rs` |
| `feature_disabled` for `start_guide` and `capture_screenshot` mode long: user and agent, before any dialog; get/export/stop of existing work still work; re-enable works; the other preview unaffected | PASS (tests with fakes) | `crates/cm-daemon/tests/preview_switch.rs` (5 tests) |
| Error code: exit 4, JSON-RPC -32014, payload round trip, CLI exit table | PASS | cm-core, cm-cli |
| MCP listing: `_meta.stage`, "(preview)", `x-stage` on `capture_screenshot.mode`; 89 tools unchanged | PASS | `cm-mcp` unit + `tests/stdio.rs` |
| CLI help `[preview]` on `guide`, its subcommands and the Long Shot flags; registry names exist | PASS | `cm-cli/tests/cli.rs` |
| Settings > General > Previews: texts (English, Vietnamese), feedback URL (feature name and version only), width tests incl. pseudo-locale, layout_guard, font coverage, i18n check | PASS (tests only) | desktop lib 1906 passed |
| The Settings page as drawn on screen (rows, pill, switch, buttons, light and dark theme, narrow window) | NOT RUN | no live screen allowed; needs a look by a person |
| Feedback button opens the browser with the issue form | NOT RUN | `cx.open_url` only built; URL text tested |

## Acceptance criteria

Not applicable: S24 has no AC ids; behaviour follows `docs/development/design-s24-preview-label.md`.

## Problems found

- Pre-existing, not from this change (fails the same with the gate removed): `cm-daemon` test `long_shot::cap_6_hud_state_pause_resume_and_manifest_source` ("paused: no new frame").
- `guide::pdf::tests::ten_vietnamese_steps_are_written_fast_in_both_sizes` failed once in a parallel run and passes alone (timing in debug).
- Deviation to confirm: `set_guide` and `export_guide` stay allowed when Click Guide is off (they work on existing guides; the brief's "edit/export of NEW work" is read as "starting new work").

## Verdict

PARTIAL (all automated checks pass; the drawn page and the browser hand-off were not seen).
