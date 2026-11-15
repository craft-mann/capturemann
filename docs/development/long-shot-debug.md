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

## Missing left side of a vertical capture (maintainer, macOS, Nov 15, 2026)

Two Chrome captures (`Screenshot 2026-11-15 13.22` 2120x9930, 9 frames, `reached_end`; `13.25` 2446x4437,
`no_overlap`) had the left 150 to 300 physical px of the window missing and the right edge complete.

| Finding | Cause found offline | Test |
| --- | --- | --- |
| Left strip of the window missing (an address bar that starts mid-URL, a dashboard without its sidebar) | not the picker or the capture target: a Window target is captured whole by `Capturer::capture` and the overlay's area only places the HUD. `Stitcher::apply_trim` cuts columns that do not change between the first two frames (`static_strips`, added on Nov 15 for the horizontal toolbar) when the plain join and the edge trim (at most 48 px left, 64 px right) fail. For a vertical capture those columns are a fixed sidebar or a page's margin, which is content, and the cut was never put back. The first frame's pixels of the cut columns are now kept (`KeptSides`) and added at both sides at the finish; below the first frame's last content row they repeat its last row. A horizontal capture still drops its static rows (a browser's toolbar) | `cap_6_vertical_static_sidebar_is_kept_not_cut` |

Not reproduced on a real screen (the two folders were read only; the frames were not dumped, `CM_LONG_SHOT_DEBUG_DIR`
writes refused pairs only). A frame is cut only when the first pair did not join as it was, so a page that still
shows the cut should be sent with its debug dump.
