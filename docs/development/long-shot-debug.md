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

## Missing left side of a vertical capture (maintainer, macOS, Nov 15, 2026), and grey sides (QA macOS pass 4, LS-1)

Two Chrome captures (`Screenshot 2026-11-15 13.22` 2120x9930, 9 frames, `reached_end`; `13.25` 2446x4437,
`no_overlap`) had the left 150 to 300 physical px of the window missing and the right edge complete. After the
first fix, QA pass 4 (Chrome 770x860 pt at 2x, a position:fixed left sidebar and a red edge strip) found the full
width kept and frame 1 right, but **every later frame had the window's shadow / desktop grey at the left (about
58 px) and right (about 110 px)** and the red strip lost.

| Finding | Cause found offline | Test |
| --- | --- | --- |
| Left strip of the window missing | `Stitcher::apply_trim` cuts the columns that do not change between the first two frames (`static_strips`, added on Nov 15 for the horizontal toolbar) when the plain join and the edge trim fail. For a vertical capture those columns are a fixed sidebar or a page's margin, which is content, and the cut was never put back | `cap_6_vertical_static_sidebar_is_kept_not_cut` |
| Grey sides in every later frame (LS-1) | The first fix put the first frame's cut columns back with `KeptSides` and **repeated the first frame's last content row below it**. That row is the bottom of the window, where the rounded corner and the shadow are drawn, so the corner pixels were smeared down both sides of the whole image. Not a different capture rectangle in the later frames: frames are the same size and the same window capture (`CGWindowListCreateImage` of the window id, no shadow; width 1540 = 770 x 2 in the normal screenshot too) | `ls_1_fixed_left_sidebar_keeps_every_frames_own_side_pixels`, `ls_1_fixed_right_sidebar_...`, `ls_1_fixed_sidebars_on_both_sides_...` (fail with the old restore: one row repeated, grey corner rows down the sides) |

Now: for a vertical capture the static columns are never cut. They only **mask the comparison** (they match at any
shift, so they cannot say where the join is); the join is found on the columns between them as before, and every
row of the image is composed from the **whole frame** that supplies it (`FullSides` in `stitch.rs`). A fixed
sidebar appears once per slice, like anything that scrolls: honest, and no row is invented. `KeptSides` is gone.
The rounded corners lie in the masked columns, which `corner_depth` cannot see, so `side_corner_depth` compares the
outermost 96 columns of each cut side, at the shifted rows, to know how many rows at the end of the previous frame
the next frame must replace (a fixed element at the edge only makes that stretch longer: safe). A horizontal capture
still drops its static rows (a browser's toolbar), and an edge trim alone (scrollbar, border) still cuts.

Not reproduced on a real screen (the pass-4 pictures were read, no frames were dumped; `CM_LONG_SHOT_DEBUG_DIR`
writes refused pairs only). The cause above is from the code and the synthetic frames; a real run on the sidebar
page is the proof still to be made.

## Window moved or resized during a run (QA macOS pass 4, LS-2) and the limit wording (LS-4)

| Finding | Cause and fix | Test |
| --- | --- | --- |
| LS-2: moving the window mid-run ended with "Stopped where the page jumped" | the frames no longer joined (another size, or the window elsewhere) and every failed join was `no_overlap`. Now, when a join fails, the run asks whether the window changed: the frame is another size than the first (`Stitcher::size_differs`), or the source says its window is not at the rectangle it had at the first frame (`FrameSource::target_changed`; the daemon's `CapturerFrames` lists the windows only then, never on every grab). Auto and Hand end `window_changed`, keeping the frames; the retries (first pair, scroll back) are skipped. Toast: "The window was moved or resized. Everything up to there is kept: {n} frames, {size}." [Open, Copy] | `ls_2_a_resized_window_ends_as_window_changed_and_keeps_the_frames`, `ls_2_a_moved_window_ends_as_window_changed_...`, `ls_2_a_page_that_jumps_is_still_no_overlap`; `s18_the_end_toasts_...` (desktop) |
| LS-4: "the longest image CaptureMann can edit" for a user `max_px` and for the 50 MP rule | the end wording knew only the 16,384 px limit. `long_shot.limit_px` is sent; the daemon's warning and the toast name the limit met: "It reached the size limit of {limit} px." (user `max_px`), "...of 50 megapixels." (image shorter than the limit), and the 16,384 wording only for that limit (`limit_kind`) | `ls_4_the_limit_note_names_the_limit_that_was_met`; `s18_the_end_toasts_...` |

A window moved without a resize is seen only where the target is a window and the listing gives its bounds
(`area_rect`); a move that the capture follows (the frames stay whole) and that joins is not an error and does not
end the run.

## macOS QA pass 6: Auto scroll ended after 2 to 8 frames on every page (LS-9, Nov 15, 2026)

The pass-6 dumps (`/tmp/cmq6`) no longer exist, so none of the three reasons below could be re-read from the
frames; they are read from the code and reproduced with synthetic frames. No real screen was used, so none of
these is called verified: a real run of the five fixtures (side, rside, both, plain, ctl) with
`CM_LONG_SHOT_DEBUG_DIR` set is the proof still to be made.

| Reason in the report | Cause in the code | Fix | Test |
| --- | --- | --- | --- |
| (a) the last frame and the refused frame are pixel-identical, yet `no_overlap` | **Confirmed in `run.rs`**: after a pair that did not join, the run scrolls back by half a step and looks again; a frame equal to the last one taken (the scroll back landed exactly on it, or the wheel did nothing) was turned into `NoOverlap` on purpose ("back where the last frame was taken is not the end"), and the run ended after two such tries. The refused frame was then identical to the last one. Why the first forward step overshot cannot be seen without the dump | When the scroll back lands on the last frame, its position is known again: the step is sent again **halved**, up to four times (each failed forward try is scrolled back by its own distance), and the distance a join teaches counts from that position. Only a ladder that finds nothing ends `no_overlap` | `ls_9_a_scroll_back_onto_the_last_frame_is_tried_again_with_a_smaller_step` (a wheel that moves 3 times what is asked from the second scroll on; fails with `SMALLER_STEPS = 0`) |
| (b) no shift matches (closest 1448, 20 to 54 of 60 lines differ) | Not confirmable. A closest shift of 1448 lines on a 1720-line frame means an overlap of about 270 lines at best: the wheel moved the page by about a frame, more than the learned step, and the half-step scroll back could not reach an overlap. The same ladder as (a) is the answer where the scroll back lands on a known frame; a frame taken during a smooth scroll is covered by the settle change below | the ladder above; `settle_frame` | as above, `ls_9_a_frame_that_settles_late_is_waited_for` |
| (c) a fixed band "changed" (header 204): the overlay scrollbar over the fixed 14 px edge strip | **Confirmed in `stitch.rs`**: the macOS overlay scrollbar shows only while the page scrolls. If the first pair has no thumb, the edge trim chosen is none (`trim = (0, 0)`), and a later pair with the thumb over the outermost columns differs in every row it covers, in the content (no shift matches) and in the fixed bands ("a fixed band changed") alike. The plain control (`ctl`) is the same without a band: any thumb that was not in the first pair refuses the join | When a pair does not join and the trim is none (or only static side columns), the pair is tried again with the outermost columns (16 on the left, 32 on the right) left out of the comparison, and the image from then on is composed from **whole frames** (`FullSides`, as for a fixed sidebar), so no content is lost and the thumb only shows where it was. The mask also covers the bands' equality | `ls_9_a_scrollbar_thumb_that_shows_after_the_first_pair_does_not_end_the_run` (header 0 and 60, thumb 12 and 26 px; fails without the retry) |
| machine under load (load 5 to 7) | `wait_settled` gives up after 150 ms and the unsettled frame was compared anyway | Auto scroll waits again (three more times, 150 ms apart) when a frame is still changing at the limit (`settle_frame`); Hand scroll is unchanged | `ls_9_a_frame_that_settles_late_is_waited_for` |

What could not be confirmed: which of (a), (b) or (c) hit the plain control page; whether 32 px covers every overlay
scrollbar of macOS at 2x (the thumb with its margin was 14 to 28 px in the report); whether a step halved four times
is enough when a wheel moves the page by more than ten frames.

