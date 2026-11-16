# QA 0.3.4 preview fixes report (PARTIAL)

- **Date:** 2026-11-16
- **Who:** Claude (agent `preview-fix`)
- **Machine:** the maintainer's Mac (Apple Silicon, macOS), headless tests only; no app was run and no screen was captured
- **Build:** worktree on main 4e6e05ac plus the uncommitted/committed change of this branch, debug, `--no-default-features`
- **Scope:** (1) Settings > General > Previews switches hide every entry point live; (2) Vietnamese dock tile label and the XS Preview tag.

## What was checked (automated, tested with fakes)

| Check | Result | Notes |
| --- | --- | --- |
| `nav::extra_tiles`, `rail_layout(flags, counts)`, `guides_page_visible` | PASS | dock tiles, rail buttons, Click Guides page (stays while guides exist) |
| `studio::app_menu_plan(flags)` and `app_menus(flags)` | PASS | no Long Shot / Click Guide items when off, 8 items when on |
| `launcher::commands::build` with `Env.previews` | PASS | no row when off, so search finds none |
| `platform_linux_menu::build_menu` | PASS | no New Click Guide item with `click_guide: false` |
| overlay `--no-click-guide`, `BarMode::listed`, bar layout | PASS | Click Guide button leaves the mode bar |
| `overlay::long_extra_args` | PASS | the desktop passes the flag when the preview is off |
| `PreviewFlags::from_settings` | PASS | default on, restores when set back on, every registry preview has a flag |
| Tile label wraps (`sidebar::tile_label_tests`, real gpui layout and real font on macOS) | PASS | Vietnamese "Hướng dẫn thao tác" is 2 lines, box <= tile; the control with the first layout measures a text item wider than the tile (90 px) |
| `cargo fmt`, clippy 1.100.0 (`cm-desktop`, `cm-overlay`), `cm-i18n check` | PASS | |

## Not checked

- Nothing was looked at on screen: the live menu rebuild (`cx.set_menus`), the native tray item removal and re-insert (`muda` `remove` / `insert` on macOS and Windows), the Linux tray, and the overlay process were not run.
- The XS tag paint order (deferred layer, opaque `surface_2` fill, offsets right -4 / top -8 as in the board) is asserted only as constants; whether it now draws above the neighbour tile and the row above was not seen.
- The Windows ThinkPad and the Linux box were not touched.
