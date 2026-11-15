# Long Shot: sending a failing pair (opt-in debug dump)

When Long Shot says "Stopped where the page jumped" (or its Hand scroll HUD says "Scroll back up a little") and you think it should not have, the engine can
write the two frames it could not join. It is **off by default**, writes only to a folder you name, and nothing
is ever uploaded.

```text
CM_LONG_SHOT_DEBUG_DIR=/tmp/ls-debug open -a CaptureMann      # or export it before cm-desktop / cm-daemon
```

For every frame the stitcher refuses (Auto scroll: the one that ended the run; Hand scroll: each settled frame
that did not join the last one taken), at most 12 per process, the folder gets:

| File | Content |
| --- | --- |
| `refused-NN-last.png` | the last frame that was accepted |
| `refused-NN-refused.png` | the frame that did not join it |
| `refused-NN.txt` | frame sizes, the run kind, and why `find_join` refused (fixed band changed, no shift matches with the closest shift and how many lines differ, several shifts match equally, frame of another size), first with exact lines and then with a few differing pixels allowed |

The pictures hold what was on your screen: look at them before sending them to anyone.

Real run without the app, through the daemon action as the app runs it (Hand scroll, a window or its area):

```text
CM_LONG_SHOT_DEBUG_DIR=/tmp/ls-debug cargo run --release -p cm-daemon --example long_shot_hand -- --window "<title part>" --secs 20 --out /tmp/ls.png [--area]
```

You scroll (or another program does); the example stops after `--secs` the way the HUD's Stop button does. Offline
check of saved frames: `cargo run -p cm-capture --example long_shot_stitch -- <dir of frames> out.png`.

## Offline reproduction of the 0.3.4 QA findings (Nov 15, 2026)

The cases below are reproduced with synthetic frames in `crates/cm-capture/src/long_shot/tests.rs` (no screen, no
input); a dump from the real window is still the better proof, so keep sending them.

| Finding (Windows QA, Nov 15) | Cause found offline | Test |
| --- | --- | --- |
| Horizontal Long Shot of a whole browser window: one frame, `no_overlap` | the tab strip and toolbar are rows that run across the scroll axis and do not move; no column of a frame matches the shifted column of the next. Now cut as static strips (`static_strips`) before the join is looked for | `cap_6_horizontal_whole_window_with_a_static_toolbar_joins` |
| Slow continuous Hand scroll: one frame | not reproduced as such (the engine joins small shifts). The frame is now taken at a third of the area or when the content holds still; a first frame that was not what stayed on screen is rebased | `cap_6_hand_scroll_with_continuous_small_steps_...`, `cap_6_hand_scroll_takes_a_frame_whenever_the_content_holds_still`, `cap_6_hand_scroll_rebases_on_a_first_frame_that_joins_nothing` |
| Windows Terminal, 200 numbered lines, stopped at line 86 | not reproduced with the real frames. Reproduced with text lines that differ in a few pixels each plus a cell that changes every frame: the tolerant look called every shift equal (ambiguous, or "no change" and a false end). Now the shift with the fewest differing pixels per line is chosen when clearly the best, and "no change" needs that no shift explains the difference better | `cap_6_terminal_lines_that_differ_in_a_few_pixels_do_not_stop_the_join` |
| First Auto run ended after one frame | not reproduced; a first frame taken while the screen still changes refuses every join. First frame taken again after 250 ms, first pair retried twice | `cap_6_auto_first_frame_taken_while_the_screen_still_changes`, `cap_6_auto_first_pair_is_retried_before_no_overlap`, `cap_6_auto_scroller_that_ignores_the_first_step` |
| Esc does not stop a run (a CLI run has no HUD) | Esc was not a global key during a run | `cap_6_esc_stops_a_run_that_has_no_request_id_and_keeps_the_frames`, `cap_6_long_shot_state_stop_ends_the_run_and_keeps_the_image` (daemon); `esc_follows_the_daemons_long_shot_events` (desktop) |

## macOS QA pass 3 findings (Nov 15, 2026)

Synthetic-frame tests only (`ls_*` in `crates/cm-capture/src/long_shot/tests.rs`, `point_tests` in
`crates/cm-daemon/src/long_shot_ops.rs`).

| Finding | Cause found offline | Test |
| --- | --- | --- |
| LS-seam: grey window-corner notches at the outer edges of every seam | the bottom lines of each frame hold the window's rounded corners and shadow, deeper than the 8 uncompared lines (`GUARD`) the next frame replaces. The lines at the end of the previous frame that differ from the same content in the next frame (up to 96, until three equal lines in a row; `corner_depth`) are now replaced too. The first frame's top and the last frame's bottom are never asked about | `ls_seam_window_corners_leave_no_notch_at_the_seams` |
| LS-flaky (a): `no_overlap` on a page of identical paragraphs | several shifts match; the expected step was the asked step, but a wheel moves more or less than asked, so no candidate was near it. The expected shift is now the step times the distance learned from earlier joins, and when that still names none within a twentieth of the content, the one candidate alone within a quarter of the step is taken. Two candidates in that window, or none, still refuse (never a guess). A last short scroll at the end of such a page has no clear shift and still refuses | `ls_flaky_repeated_paragraphs_join_by_the_expected_step`, `ls_flaky_repeated_paragraphs_still_refused_when_the_step_names_two` |
| LS-flaky (b): `nothing_scrolled` for CLI and agent runs while the app's own window covers the target's centre | the pointer was put on the centre and the wheel went to our window. The daemon now lists the app's own windows (`Capturer::own_window_rects`) and orders the points centre, then the quarter points, with the covered ones last. When the first step moves nothing the run asks the scroller for the next point (`Scroller::next_point`) and sends the step again | `ls_flaky_a_point_under_our_own_window_is_not_first`, `ls_flaky_the_scroller_moves_to_the_next_point_when_asked`, `ls_flaky_first_step_is_retried_at_another_point` |
| LS-flaky (c): `nothing_scrolled` for a fresh Chrome window | not reproduced. 0.3.4 already sent one more step after 450 ms of looking. Now three more steps (the first after 200 ms at the same point, the others at the next points) before the run says nothing scrolled | `ls_flaky_fresh_window_that_ignores_two_steps` |

None of these has a report on a real screen after the fix, so none is called verified.

## Windows QA pass 3 findings (Nov 15, 2026)

Synthetic-frame tests only (`cap_6_*` in `crates/cm-capture/src/long_shot/tests.rs`, tests in `crates/cm-daemon`, `crates/cm-cli`, `crates/cm-core`); no real screen was used, so none of these is called verified.

| Finding | Cause found offline | Test |
| --- | --- | --- |
| 1 (medium): Windows Terminal, about 200 near-identical lines, `no_overlap` at line 111 | not reproduced with the real frames. Reproduced with lines that differ only in their number, a blinking cursor in the text and in a static status strip, and a wheel that moves whole notches (one short now and then, once two too many). Four faults: (a) when the wheel overshoots and leaves a handful of lines of overlap, several shifts match over 4 to 7 lines each and the tolerant look called the pair "no change", a false `reached_end`; now it says no overlap when several shifts explain the change clearly better than no shift does; (b) with no true overlap at all, the shift that fits least badly (a line matched with another that differs in a digit) was believed and taught a wrong distance; a best shift far short of the expected one is refused unless it matches exactly over 30 or more lines; (c) several shifts matching equally were chosen by the expected step even over a few lines: now needs 12 compared lines, and a long exact overlap beats short ones that match as well; (d) the learned distance followed one noisy join: now the first join sets it and later ones move it halfway. Plus the adaptive retry: a pair that does not join (not the first) scrolls back by half a step, takes the frame again and joins without an expected shift, twice, before the run ends `no_overlap` (`Scroller::scroll_back`, built for macOS, Windows and X11; a scroller without it, or Wayland, ends as before). The static status strip at the bottom was already cut as a fixed band (footer) by the first pair | `cap_6_terminal_near_identical_lines_with_a_status_strip_and_blinking_cursors` (five scroll configurations; with the retries turned off it ends `no_overlap` at 748 px) |
| 2 (low): Auto does not raise the target window; `nothing_scrolled` with a note that does not name the cause | before the first step the daemon asks the system to bring a window target to the front (`LongShotSystem::raise_window`; Windows: restore if minimized, attach to the front window's input queue, `SetForegroundWindow`). **Windows only**: macOS (`NSRunningApplication`, needs the owner pid, which the target does not carry here) and X11 (`_NET_ACTIVE_WINDOW`) have no helper in `cm-capture` yet, and Wayland has no window ids. The note is now "Nothing scrolled: the area may be covered by another window or the page is not scrollable" (English and Vietnamese catalogue) | `cap_6_auto_scroll_raises_the_target_window_first` (daemon); the Windows code compiles for `x86_64-pc-windows-msvc` and has not run |
| 3 (low): HUD Stop stores `no_overlap` in 2 of 3 runs | the loop looked at Stop only at the top of the next round; a Stop pressed during a scroll, with the HUD closing in the frame, ended the run with the join result. Every ending that is not a clean join (`no_overlap`, `reached_end`, `nothing_scrolled`) now asks Stop again first and ends `stopped_by_user`; the image is kept | `cap_6_stop_while_the_join_fails_ends_as_stopped_by_user` (fails as `no_overlap` without the fix) |
| 4 (low): `capturemann library list` with a 341-character library path and no app: 15 s, then `daemon_unavailable` | the CLI started a daemon that exited at once on the long path and waited for it. The path check (`cm_core::library_path::check_root_path_len`, moved from `cm-library`, which re-exports it; the message now names the path) runs in the CLI's launcher before anything is started (explicit folder, `CAPTUREMANN_LIBRARY`, then `library.path` in `settings.json`) and answers `invalid_input` at once; `cm-daemon` checks too and prints the error to stderr | `a_library_path_past_the_limit_is_invalid_input_at_once` (`cm-cli`), `a_path_past_the_limit_is_invalid_input_and_names_path_and_limit` (`cm-core`) |
| 5 (unresolved): Chrome horizontal Long Shot with a region ended after one frame in one run | not reproduced. The region geometry is right: the offset is applied once (the crop inside the display, the scroll points inside the global area) and the scale once (a logical rect scaled once, a physical rect never), checked on a display that is not at (0, 0) at scale 2. The one-frame end looks like the whole-window case (a toolbar or a tab strip inside the region, cut now as static strips) or like the first-frame warm-up; a dump from the real run (`CM_LONG_SHOT_DEBUG_DIR`) would tell | `ls_region_offset_and_scale_are_applied_once` (`cm-daemon`) |

Known limit of the join: when a wheel overshoots so far that fewer than about 12 lines overlap, and the lines are alike, the pair cannot be told from a wrong match; the retry scrolls back by half a step, which usually brings a clear overlap, but two notches too many on a 250-line area still gave a wrong join in two of the 36 synthetic cases tried.
