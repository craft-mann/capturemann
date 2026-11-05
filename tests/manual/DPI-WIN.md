# DPI-WIN: Windows with more than one monitor and mixed scaling

Plan item: 0.2.8 item 7 "second monitor with mixed scaling". The lab laptop has one display, so **nothing in
this file was run on hardware**. What exists is a code review (below) and unit tests with fake monitor layouts.
Everything under "Run when a second monitor exists" is open.

Conventions on Windows (set by the code, checked in review):

* The process is per-monitor-v2 DPI aware, so every coordinate the capture, cursor and overlay code sees is
  **physical px of the virtual screen** (monitors can sit at negative x or y). `DisplayInfo.bounds`,
  `WindowInfo.bounds`, the cursor hook and WGC sizes are all in that space; the cursor target scale is 1.0.
* A region is **display-relative** (physical px from the display's own top-left, or logical px converted with
  that display's own scale: `resolve_region`). It lives on one display.
* GPUI (HUD, countdown, brackets, bubble) works in logical px per display: a display's origin is
  `physical origin / its own scale` (gpui-pre-windows `display.rs`), the same rule as `geometry::area_points`.
  These point rectangles of two displays can overlap on a mixed-DPI desktop.

## What was reviewed

`crates/cm-capture`: `sys/win_display.rs`, `sys/win_window.rs`, `sys/win_names.rs`, `sys/win_wgc.rs`,
`record/windows.rs`, `record/wgc_logic.rs`, `record/plan.rs`, `region.rs`, `xcap_backend.rs` and the xcap 0.9.8
Windows sources it calls (`impl_monitor.rs`, `impl_window.rs`, `capture.rs`); `crates/cm-daemon/src/recording/sys/windows.rs`;
`crates/cm-cursor` (`windows.rs`, `target.rs`); `apps/overlay` (`app.rs`, `targets.rs`); `apps/desktop`
(`platform_window.rs`, `ui/recording/mod.rs`, `recording/geometry.rs`, `window_picker/session.rs`, `main.rs`,
`app.rc`, `build.rs`); gpui-pre-windows 0.3.9 and winit 0.30.13 for DPI handling; `packaging/windows`.

## Findings

Kind: **bug** (fixed, with a test), **decision** (written down, behavior unchanged), **fine** (checked, no change),
**open** (cannot be settled without a second monitor).

| # | Kind | Where | Finding |
|---|------|-------|---------|
| 1 | bug, fixed | `sys/win_window.rs` `frame_bounds`, `sys/win_names.rs` `visible_xywh` | The picker listed the DWM frame rectangle, but `capture` saved `crop_box(outer, frame)`, which falls back to the whole window rectangle when the frame is not inside it (1 px at a fractional scale, a window between two monitors). Listing and saved picture could differ by 1 px, the old "1426x720 vs 1426x721" class. Now both use `visible_xywh`. Table test `the_listed_rectangle_is_the_saved_rectangle_on_every_layout` (100 % + 150 % right, 200 % + 100 % left at -1920, a monitor above, frame outside rectangle, window across the seam). `crop_box` moved to `win_names.rs` so its tests run on every OS (they only ran on Windows before). |
| 2 | bug, fixed | `apps/desktop/src/main.rs`, `xcap_backend.rs` `check_permission` | `cm-desktop.exe` has no DPI manifest (`app.rc` is an icon only) and GPUI does not set awareness; the lazy `ensure_dpi_aware()` ran only on the first window listing. The in-process daemon could therefore read monitors/scales before awareness was set (xcap's scale falls back to `GetDeviceCaps`, positions may be virtualized) and switch the process to aware after windows existed. `win_window.rs` even says "the desktop app is already aware", which nothing in the repo guarantees. Now `cm_capture::ensure_dpi_aware()` is the first call in the desktop `main`, and every `XcapBackend` call goes through `check_permission`, which sets it first. No-op on other systems. The overlay needs nothing: winit sets per-monitor-v2 when its event loop starts, and the overlay does not link `cm-capture`. |
| 3 | bug, fixed | `apps/desktop/src/ui/recording/mod.rs` `indicator_area`, `recording/geometry.rs` `pick_display_for` | The HUD, countdown disc and brackets chose the area's display by the center of the area in "points". Points are `physical / own scale`, so a 100 % display (points 0..1920) and a 150 % display to its right (points 1280..2987) overlap; a region in the left part of the second display has its center inside both, and the first display in the list won, so the HUD/countdown opened on the wrong display. Now the engine's `indicator.area.display_id` decides (GPUI's `DisplayId` is the same `HMONITOR` value as xcap's id), the center test is the fallback. Test `the_areas_display_is_found_by_id_on_mixed_dpi_layouts` asserts the old center test names the wrong display on 2 of the 4 layouts (100 % + 150 %, 100 % + 200 %) and the id is right on all. |
| 4 | fine | `region.rs` `resolve_region`, `record/plan.rs` | Regions are display-relative; logical rects use the scale of their own display; edges are rounded, not origin and size separately; a region leaving its display (so also one spanning two monitors) is `invalid_input`; an unknown display is `not_found`. Negative display origins never enter (they are only added later for the cursor). Tests in `cm-daemon` `recording/sys/windows.rs`: `a_logical_region_uses_the_scale_of_its_own_display`, `a_region_on_no_monitor_or_on_two_is_refused`. |
| 5 | fine | `cm-daemon/.../sys/windows.rs` `cursor_target` | Origin = display origin + crop origin (physical), scale 1.0, window = its listed rectangle. Table test `region_cursor_targets_on_mixed_dpi_layouts` over four layouts (including a display at -1920,-1080) and `a_window_on_a_negative_monitor_maps_from_its_own_corner`. |
| 6 | fine | `record/wgc_logic.rs` `clamp_crop`, `sys/win_wgc.rs` | The crop is physical px relative to the frame and clamped to the frame, so a mode change that shrinks the display cannot read outside it. The frame pool is recreated on the first frame whose `ContentSize` differs (`win_wgc.rs:263-269`), the staging texture follows (`staging_for`). A DPI-scale change alone (150 % to 200 % in Settings) does not change the physical size, so WGC keeps the size; the cursor target stays valid. |
| 7 | fine | `overlay/targets.rs` `match_monitors`, `display_at` | Monitors are matched to displays by physical size and position, both in the virtual screen; a left/up monitor at negative coordinates, a different OS order and an unplugged monitor are handled. Test `displays_pair_with_monitors_on_a_mixed_dpi_desktop`. Overlay windows are placed with winit's physical `monitor.position()`/`size()` and borderless fullscreen on that monitor. |
| 8 | fine | `xcap_backend.rs` | On Windows `to_physical` passes xcap values through (xcap reports physical px: `DEVMODE` position and size, `GetDpiForMonitor` effective DPI), a window's `scale` is the scale of `MonitorFromWindow(NEAREST)` (the monitor with the largest overlap). `ensure_primary` covers a missing primary flag. |
| 9 | decision | `record/windows.rs`, `cm-cursor/src/target.rs` | A **window** recording is fixed to the window's rectangle at start. Moving the window to a monitor with another scale makes Windows resize it (WM_DPICHANGED); WGC follows (pool recreate) and the encoder scales the new frame to the planned size by stretching (no letterbox), so a resized window is distorted and the cursor target (rectangle at start) no longer matches. Same limit as macOS ("window moves are not tracked in v0.1"). Not changed. |
| 10 | decision, fixed (Nov 5, 2026) | `cm-cursor/src/windows.rs`, `cm-cursor/src/dpi.rs` | The cursor image density was `GetDpiForSystem()/96`, not the DPI of the monitor being shown. Maintainer decision: use the MONITOR's DPI, with no change at 100 %. Now the sprite is extracted per (cursor handle, DPI of the monitor under the pointer: `MonitorFromPoint` + `GetDpiForMonitor(MDT_EFFECTIVE_DPI)`), so a handle shown on another monitor gets a new image. **Detail the decision did not spell out:** the bitmap from `GetCursorInfo` is loaded at the SYSTEM DPI and Windows draws it at the MONITOR's DPI; the renderer draws `size x display_scale(1.0) / image.scale` master px per bitmap px, so the stored `scale` is `system_dpi / monitor_dpi` (pure fn `dpi::sprite_density`), not the monitor scale itself (storing 2.0 for a 200 % monitor would shrink the cursor further). Examples: system 100 %, monitor 200 %: 0.5 (32 px bitmap drawn 64 px); monitor = system: 1.0; 100 % everywhere: 1.0 as before. Side effect: a single monitor at 150 % was 1.5 (cursor drawn at 32 px) and is now 1.0 (drawn at the bitmap's 48 px). The process is already per-monitor-v2 aware before cursor calls (`cm_capture::ensure_dpi_aware` is the first call in `cm-daemon` and `cm-desktop` mains). |
| 11 | decision | `apps/overlay/src/targets.rs:67` | A window is hit-tested only on the display in its `display_id` (rule R1-2 for macOS, where global spaces overlap). On Windows the global space is consistent, so a window across two monitors could be picked on both; today the half on the other monitor is not highlightable there. Kept. |
| 12 | decision | `platform_window.rs` (Windows `imp`), `launcher_window.rs:152` | On Windows `frame`, `set_frame`, `display_frame`, `visible_frame_at` and `pointer` are stubs (`None`/no-op). So the launcher and the start sheet always open on the primary display (not where the pointer is), the HUD and bubble cannot be dragged/snapped, and nothing remembers their position. This is a multi-monitor gap, not a DPI bug; the HUD/countdown/brackets still open on the right display after fix 3. |
| 13 | open | `xcap` `impl_monitor.rs` | xcap reads monitor position/size with `EnumDisplaySettingsW` (`dmPosition`, `dmPelsWidth`). It is assumed these are physical and unvirtualized once the process is per-monitor-v2 aware (fix 2 makes that so before the first read). Needs one run on a mixed-DPI machine; the single-monitor laptop at 150 % showed correct sizes in the daemon. |
| 14 | open | `sys/win_display.rs` | Friendly names come from `QueryDisplayConfig` by GDI device name; not exercised with two monitors (a clone/duplicate topology has two paths for one source). |

Not found to be a problem: using the primary monitor's scale for another monitor (every scale read is per display
or per window), rounding of region edges, negative origins in the cursor mapping, WGC frame size changes (pool
recreate), `is_primary` flag test (`dwFlags == MONITORINFOF_PRIMARY`; the only flag Windows sets there).

## Tests added (all fakes, run on every OS)

* `cm-capture` `sys::win_names::tests::the_listed_rectangle_is_the_saved_rectangle_on_every_layout` (+ the moved `crop_box` tests)
* `cm-daemon` `recording::sys::windows::tests`: `region_cursor_targets_on_mixed_dpi_layouts`,
  `a_logical_region_uses_the_scale_of_its_own_display`, `a_region_on_no_monitor_or_on_two_is_refused`,
  `a_window_on_a_negative_monitor_maps_from_its_own_corner`
* `cm-overlay` `targets::tests::displays_pair_with_monitors_on_a_mixed_dpi_desktop`
* `cm-desktop` `recording::geometry::tests::the_areas_display_is_found_by_id_on_mixed_dpi_layouts`

## Run when a second monitor exists (NOT run on hardware)

Setup: Settings > System > Display. Make a layout per row below, note each monitor's scale, and restart
CaptureMann after every layout change. Use the `list_targets` tool or its CLI subcommand (or the Start sheet) to read the bounds.
Recording steps record a few seconds of the screen; delete the output. Ask before real screen runs.

| Layout | |
|--------|--|
| A | Laptop panel 100 % at 0,0 + external 150 % to the right |
| B | Primary 200 % + a 100 % monitor to the LEFT (negative x) |
| C | An external monitor ABOVE the primary (negative y), mixed scales |

For each layout:

1. List the targets: every display has the physical size Windows Settings shows, its own scale, and
   the origins match the arrangement (B has x < 0, C has y < 0). Pass: no display has the primary's scale unless
   it really has it. If a size is off, finding 13 is real.
2. Screenshot a display on each monitor. Pass: the PNG size equals the listed physical size.
3. Region screenshot on the second monitor (the picker, then drag), and `capture region` with a logical
   and a physical rect. Pass: the PNG is the area the overlay showed, edge to edge, size = the rect times that
   monitor's scale (rounded per edge). A region dragged over the seam must be refused or clipped to one display,
   never a picture of the wrong monitor.
4. Window screenshot with the window on each monitor. Pass: the picker's size equals the PNG size to the pixel
   (finding 1); no blank border, no cut edge.
5. Move a window from the 100 % to the 150 % monitor and screenshot again. Pass: still exact.
6. Record 10 s of a display on the second monitor, mouse moving in circles and clicking in the corners.
   Pass: the cursor in the editor preview sits where the real pointer was, also at the corners; no jump when
   the pointer leaves for the other monitor (it goes "outside", not to an edge).
7. Record a region on the second monitor, then a window on it. Same pass criteria; the video size equals the
   region/window size (not the monitor's).
8. Start a recording of the second monitor from the Start sheet. Pass: the countdown disc, the area brackets and
   the HUD appear on THAT monitor (finding 3), for a region in the left part of the second monitor too.
9. While recording a window, drag it to the other monitor. Record what happens (finding 9: the picture may be
   stretched; the cursor may be off). This is a decision, not a pass/fail; note the result in the report.
10. While recording a display, change that monitor's scale in Settings. Pass: the recording continues and
    stops cleanly, the file plays, the cursor overlay is still right. Also unplug the recorded monitor: the
    recording ends with a message, the file up to then is kept.
11. Cursor sprite (finding 10): record the 200 % monitor with the system scale at 100 %; note whether the cursor
    looks half the size it had on screen.
12. Overlay (region/window/display capture hotkey): one overlay per monitor, each covering its monitor exactly
    (no gap, no overlap, taskbar covered), crosshair and magnifier follow the pointer from one monitor to the
    other, Esc closes all.
13. The app's log folder: the overlay trace line `mapping` shows
    `covers_display_1_to_1: true` for each monitor.

Report the run in `verification/` (template in its README). Until then the status is "tested with fakes" for
the items fixed above and "not run on hardware" for everything in this section.

## Cursor sprite size on hardware (finding 10; NOT run)

Record a few seconds (ask first; delete the output) with the pointer moved over a text field and the link hand, then open the
recording and check `cursors/index.json` and the exported video:

1. Single monitor at 100 %: `scale` of every image is 1.0, the drawn cursor looks as before.
2. Single monitor at 150 % or 200 %: `scale` is 1.0, and the drawn cursor is as large as the real cursor in a screen
   recording made by another tool (compare the pointer height in px). Before this change it was 32 px.
3. System scale 100 % (the primary) and a 200 % second monitor, recording the second: images captured there have
   `scale` 0.5 and the PNG size is whatever Windows returns (32 px expected; **if it is already 64 px the bitmap is
   at the monitor DPI and the premise is wrong: report it, do not guess**). The drawn cursor is about twice the size of
   the one drawn for the 100 % monitor and matches the real pointer on that monitor.
4. Move the pointer between the two monitors during a recording of one: the index holds two images for the same
   shape with different `scale` (only the first is used while the pointer is on the recorded monitor).
5. A cursor-size change in Settings > Accessibility changes the bitmap size; the drawn size follows.

## Recording windows on a second monitor (Nov 5, 2026 fix; NOT run on hardware)

Report: `verification/L1-windows-0.2.8-round2-2026-11-05-claude.md` item 6d (panel 1920x1080 at 150 % +
Samsung 3840x2160 at 150 % to the right; recording display 131073 put the countdown on the panel, the
brackets window at (-11,-5) and the HUD at (1366,1960) straddling the seam).

**Root cause (found by reading the code and GPUI 0.3.9's Windows backend; not reproduced on hardware).**
Three things combined:

1. `ui::recording::panel_options` made the window origin relative to the display (`global.relative_to(display)`),
   which is what GPUI's macOS backend wants. GPUI's Windows backend takes `WindowBounds` as **absolute** points
   (physical px / the display's own scale) and only keeps them if their center lies on the named monitor,
   otherwise it falls back to the display's default bounds. The numbers of the report follow exactly:
   the HUD's local origin on the external was (910, 1307) points = (1366, 1960) px at 150 %, and its center
   x was 1920, the seam. The brackets window had local origin (0,0) = the panel's origin. The same mistake sat
   in the launcher (origin (0,0) on whichever display) and the confirm dialog (`library_ops`).
2. The engine's `indicator.area.display_id` (131073) is not GPUI's display id (the `HMONITOR` handle), so the
   `pick_display_for` by id of c990dd1 could never match on Windows and fell back to the point-centre test,
   which is ambiguous when point rectangles overlap (mixed DPI).
3. `display_frame`, `visible_frame_at`, `frame`, `set_frame`, `move_to`, `mouse_location` and `left_button_down`
   were stubs, so the HUD was placed in the whole display (not the work area) and could not be snapped, and the
   launcher and start sheet never followed the pointer (finding 12).

**What is correct now (contract, `recording/monitors.rs`).** The recorded monitor is found from the recorded
pixels in physical px (never from the id or from points). The countdown disc is centered on the recorded area,
kept on that monitor. The area window covers exactly that monitor's frame (physical (1920,0) 3840x2160 on the
lab layout). The HUD sits at the bottom centre of that monitor's work area (above the taskbar) and its snap
and remembered position use the same work area. The launcher, the Record sheet and the dialogs open on the
monitor under the pointer (`GetCursorPos` + physical containment), primary as the fallback. Windows opened on
a display get absolute points; macOS keeps display-relative ones.

**Tested with fakes (any OS).** `recording::monitors::tests`: the lab layout (external and panel), 100 % external
right of a 150 % panel, a 150 % monitor whose points overlap a 100 % one, 100 % left of a 200 % primary
(negative x), a monitor above the primary (negative y); each checks the countdown, the area window, the
brackets and the HUD in physical px. The Win32 reading code (`platform_win_monitors.rs`) was type-checked for
`x86_64-pc-windows-msvc` in isolation; `cargo check --target x86_64-pc-windows-msvc -p cm-desktop` fails on
the Mac in the SQLite C build (no MSVC headers), so the wiring in `platform_window.rs` is checked by the
Windows CI build only.

**Run on the laptop (two monitors, Windows):**

1. Install the package built from this commit. Layout A (panel primary + Samsung to the right, both 150 %).
2. `capturemann record start --display <Samsung id> --countdown 5`: the disc is on the Samsung, centered; during
   the recording a window of exactly 3840x2160 at (1920,0) (read the HWND client rect, not the outer rect) and
   the HUD fully on the Samsung at its bottom centre; nothing on the panel except the bubble if the camera is on.
3. Same for the panel (`--display 65608`): the HUD is above the taskbar, centered on the panel.
4. A region and a window recording on the Samsung (brackets around the region, HUD on the Samsung).
5. Press the launcher hotkey and "Record..." with the pointer on the Samsung, then on the panel: both open on the
   monitor with the pointer.
6. Drag the HUD near an edge of the Samsung and release: it snaps to the margin, and the next recording of that
   display opens there.
7. Repeat with layouts B and C (mixed DPI) and a monitor left of / above the panel.

**Not checked / risks.** (a) The area window is not click-through on Windows (`Tweaks::click_through` is a
macOS-only tweak), so it may swallow clicks on the recorded monitor: look for this in step 2. (b) Dragging the
HUD and the bubble relies on GPUI starting a native window move on Windows; `frame` and `move_to` now work but
the drag start was not looked at. (c) The Record sheet's Display tiles and the delay pill send GPUI's display
id (the `HMONITOR`) as the engine's `display_id`: on Windows that is another number, so "Display" capture from
the pill or a tile may name the wrong display; this needs a map from the monitor to the engine's id (not done).
(d) The `frame` rect is the client area, which is what GPUI calls its bounds; the 3862x2171 outer rect of the
report includes the invisible resize frame, so compare against the client rect.
