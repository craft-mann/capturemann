# QA 0.3.4 Long Shot on real sites, Windows (Edge) and Linux (Firefox X11), lsreal (PARTIAL)

- **Date:** 2026-11-16, Claude (agent lsreal). **Machine:** ThinkPad T490 (Windows 11 at 150 %, 1920x1080, one monitor; Ubuntu 24.04 X11, Firefox snap, 1920x1080).
- **Build:** worktree on main `d8f669dc` plus commits up to `f2ed0814` (cm-capture only; unsigned example binary `long_shot_hand --auto`, release, built on the laptop). Daemon action `capture_screenshot { mode: long, scroll: auto, whole window }` as the user, in a throwaway library. Not the installed app, not the CLI.
- **Method:** each site in a fresh browser profile (Edge `--guest`, Firefox own profile), muted, nothing played, no login; window moved to a fixed size; `CM_LONG_SHOT_TRACE_DIR` and `CM_LONG_SHOT_DEBUG_DIR` set (new, see `docs/development/long-shot-debug.md`). Pictures read at 2 to 4 places per page (downscaled copies), not row by row. Sites: carchargeos.com, Wikipedia (Rust), YouTube @Google/videos, docs.rs/serde, paulgraham.com/greatwork.html.
- **Harness lessons:** (1) an Edge profile that signs in to the Windows account shows a modal "We are now syncing" dialog: every frame dimmed, and in a small window the wheel does nothing (`nothing_scrolled`); `--guest` avoids it. (2) The harness's own console window covered the Edge window and took the wheel (`nothing_scrolled`).

## Windows, Edge guest, final build (`30717279`)

| Page | 1064x782 px window | 1614x1011 px window | What the PNG shows |
| --- | --- | --- | --- |
| carchargeos.com | `no_overlap`, 19 frames, 1030x6182 (header 120, footer 8) | `no_overlap`, 10 frames, 1587x4461 | continuous up to the stop (not re-read after the last change); stops where a frosted header sits over a dark image and the next heading is not drawn yet |
| Wikipedia | `limit_reached`, 53 frames, 1037x16381 | `limit_reached`, 40 frames, 1614x16384 | read in the first run (1632x1097): continuous, side bars once per frame |
| YouTube | `no_overlap`, **2 frames**, 1026x1082 | `no_overlap`, 2 frames | stops at the first sticky tab bar |
| docs.rs/serde | `reached_end`, 12 frames, 994x4046 | `reached_end`, 7 frames, 1614x3274 | not read |
| paulgraham.com | `limit_reached`, 38 frames, 1027x16384 | `limit_reached`, 37 frames, 1579x16384 | read at two places over 3000 px: text continuous across a seam |

Before the fixes (same small window, same laptop): Wikipedia, YouTube, docs.rs, paulgraham.com 1 frame `no_overlap`, carchargeos 8 frames; the wheel moved the page 600 px for a request of 234 px.

## Linux, Firefox X11, final build (`30717279`)

| Page | 1280x900 | 1000x700 | What the PNG shows |
| --- | --- | --- | --- |
| carchargeos.com | `reached_end`, 15 frames, 1276x7339 | `reached_end`, 24 frames, 1000x7428 | not read again |
| Wikipedia | `limit_reached`, 25 frames, 1276x16384 (before: `no_overlap`, 10 frames, 4636) | `limit_reached`, 34 frames, 1000x16384 | main column continuous at 2 places; the contents list and appearance panel repeat once per slice (fixed panels) |
| YouTube | `limit_reached`, 43 frames, 1228x16384 (old binary; not rerun) | `limit_reached`, 59 frames, 996x16384 | not read |
| docs.rs/serde | `reached_end`, 4 frames, 1276x2053 | `reached_end`, 7 frames, 996x2389 | not read |
| paulgraham.com | `limit_reached`, 25 frames, 1280x16384 | `limit_reached`, 41 frames, 996x16384 | not read |

## Causes and fixes (tests in `crates/cm-capture/src/long_shot/`)

| Cause | Fix | Test (failed first?) |
| --- | --- | --- |
| first step overshoots (Edge moved 600 px for 234), first pair had no way back | scroll-back ladder for the first pair | `lsreal_a_first_step_that_overshoots_is_scrolled_back_and_joined` (yes) |
| a scroll of a sparse text page was called "the same frame" (`reached_end` after 12 frames of 16,000 px, silent loss) | a region is believed only if most inked rows are equal unshifted | `lsreal_a_scroll_of_a_sparse_page_is_not_one_position_with_a_region` (yes) |
| one lost wheel step ended a page | one more step before `reached_end` | `lsreal_one_lost_wheel_step_is_not_the_end_of_the_page` (yes) |
| thin pairs (translucent header, cards still drawing) | a step-named shift on 20 inked lines; blank lines do not split stretches | `lsreal_a_short_match_...`, `lsreal_lines_that_differ_with_blank_lines_...` (yes) |
| same, with a steady wheel (300 px notches) | `Stitcher::trust_hint`: shift with 6+ lines of its own, not shared with neighbours, over the whole range | `lsreal_a_steady_wheel_names_...` (new API; wrong distance and no distance refused) |
| Linux Wikipedia: fixed panels after a run trimmed its scrollbar edge | `join_masked_trimmed` (`mask_extra`) | `lsreal_panels_that_stay_put_...` (yes) |
| size-limit note "50 megapixels" for 16383 px | `limit_kind` slack of 64 rows | `ls_4_...` extended (yes) |
| rescue did not teach the wheel distance | gain raised after a rescue | no fake fails without it (seen in real traces) |
| dump written for the ladder's last pair only, cap 12 | pair the step made is dumped; cap 60; frame trace `CM_LONG_SHOT_TRACE_DIR` | none (diagnostics) |
| `cap_6_stop_keeps_the_frames_so_far` failed on main (counts `stop` calls) | asserts the kept rows instead | the test |

Tests: `cargo test -p cm-capture --no-default-features --locked` 274 passed; `-p cm-daemon ... long_shot` passed (at `f7f5ac89`); clippy (1.100.0) clean at `f7f5ac89`; `cap_6_ac3_*` and `ls_*` pass at the final commit (cm-capture).

## Not fixed

- YouTube on Windows (2 frames): after the tab bar sticks, content is 1 to 3 px off non-rigidly; no exact join (78 % of rows differ at the best shift, also with tolerance 24).
- carchargeos.com on Windows after 6182 px / 4461 px: a pair with 5 lines of its own (needs 6). Ends `no_overlap`, frames kept.
- The trusted-distance join rests on 6 lines; it is weak evidence, protected by the steady-wheel condition and the whole-range check. Not run: Windows fixed-sidebars fixture, classic conhost, Wayland, second monitor, the installed app/CLI path. The Edge window height at 150 % shifted what the wheel moves (150 or 300 px per step), so results depend on window size.

## Verdict

PARTIAL
