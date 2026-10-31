# L1 report: Linux tray icon (StatusNotifierItem)

- **Date:** 2026-10-31
- **Who:** Claude (agent), for the maintainer
- **Machine:** QEMU VM on an Apple Silicon Mac, Ubuntu 24.04 aarch64, Xvfb `:99` (X11), a **private** session bus from `dbus-run-session` (the GNOME session's own bus was not touched). No real panel.
- **Build:** debug `cm-desktop`, `--no-default-features`, `cargo build -j2`, tree of this commit
- **Stand-in watcher:** `python3` + `dbus-python` script that owns `org.kde.StatusNotifierWatcher`, prints `RegisterStatusNotifierItem` calls and answers `IsStatusNotifierHostRegistered = true`. It is not a real host: nothing drew the icon.

## Decision: hand-written StatusNotifierItem over `zbus`, not `ksni`

| | `ksni` 0.3.7 | hand-written over `zbus` 5.19 |
| --- | --- | --- |
| Licence | **Unlicense**; `deny.toml` allows it only for `whisper-rs`, so a new exception | MIT/Apache (zbus, already in the tree) |
| New crates | `ksni` + `arc-swap`, `crossbeam-queue`, `pastey` and, for the `async-io` runtime GPUI/ashpd need, `async-lock`, `async-executor`, `task-local` (default feature `tokio` would clash with the `ashpd` runtime fix) | none (`Cargo.lock` gains only the edge `cm-desktop -> zbus`) |
| Effort | small | about 400 lines (item, dbusmenu, watcher re-registration) plus a pure menu model with 10 unit tests |
| Control | fixed menu type | the menu is the shared model, tested on macOS too |

Chosen: hand-written. `cargo deny check`: advisories ok, bans ok, licenses ok, sources ok (before and after).

## What was built

- `apps/desktop/src/platform_linux_menu.rs`: pure menu model (same items and order as `platform::build_tray`, Stop first while recording, one check mark among the delays, five recents, stable dbusmenu ids, click -> action id, `IconPixmap` ARGB conversion). Compiled on every OS.
- `apps/desktop/src/platform_linux.rs`: `org.kde.StatusNotifierItem` at `/StatusNotifierItem`, `com.canonical.dbusmenu` at `/MenuBar`, registration with the watcher, re-registration when the watcher reappears, signals `NewIcon`, `NewStatus`, `NewToolTip`, `LayoutUpdated`. Icons: the kit's "on dark" `.ico` set (as Windows) at 22, 32 and 48 px.
- `apps/desktop/src/platform.rs`: the old struct is `NativeTray` (macOS, Windows); `TrayHandle` is that or the Linux one; new `platform::set_menu_handler` replaces `MenuEvent::set_event_handler` in `main.rs`, so a click reaches `Command::TrayMenu(id)` through the same router on every OS.
- No watcher on the bus: `build_tray` fails with a translated message (names the GNOME AppIndicator extension) and the existing "Menu bar icon unavailable" toast shows. Attaching later when no watcher existed at start is not done; a watcher that restarts after a successful start is handled.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 `cargo test -p cm-desktop --lib platform_linux_menu` on macOS | PASS | 10 passed |
| 2 same in the VM (`platform_linux`) | PASS | 10 passed |
| 3 `cargo clippy -p cm-desktop --no-default-features --all-targets -- -D warnings` in the VM | PASS | |
| 4 `cargo fmt`, `python3 scripts/check_target_os.py`, `cargo run -p cm-i18n -- check`, `-- extract` | PASS | no new `target_os` line outside `platform*.rs` |
| 5 App on Xvfb, session bus without a watcher | PASS | app alive after 12 s, no panic, nothing exported; the toast path (not seen on screen) |
| 6 With the stand-in watcher: `RegisterStatusNotifierItem` | PASS | watcher printed `REGISTERED org.kde.StatusNotifierItem-<pid>-1` |
| 7 `busctl --user get-property` on `/StatusNotifierItem` | PASS | Category `ApplicationStatus`, Id `dev.capturemann.app`, Title `CaptureMann`, Status `Active`, Menu `/MenuBar`, ToolTip text `CaptureMann`, IconPixmap 3 sizes (22, 32, 48) |
| 8 `GetLayout` of `/MenuBar` | PASS | 17 root items in the macOS order; Capture delay has 4 check items, Recent captures has "No captures yet" (disabled), Pause and Stop `enabled=false`, 3 separators |
| 9 `Event(102, "clicked")` (Capture delay > 5 s) | PASS | the app's action ran: `GetGroupProperties` then shows `toggle-state` 1 on "5 seconds" and 0 elsewhere; `gdbus monitor` saw `LayoutUpdated (2, 0)` |
| 10 `Event(10)` Open launcher; `Event(8)` Stop while idle | PASS | app alive; Stop is disabled and routes nowhere |
| 11 Kill the watcher, start a new one | PASS | the item registered again (`tray: registered again with the restarted watcher`) |
| 12 `Event(14)` Quit CaptureMann | PASS | the app exited |

## Not done

- Not seen in a real panel (KDE Plasma, GNOME with the AppIndicator extension, XFCE): menu rendering, tooltip, icon size and `Activate` on left click are untested. `tests/manual/LINUX-1.md` steps 19-23 cover them.
- Recording state icons (`Recording`, `Paused`) are wired through `apply_plan` but were not exercised: recording is not available on Linux yet.
- The window opened by "Open launcher" was not looked at (only that the app stayed up); the first `Event(102)` run proves the click-to-action path.
- `Attention` uses `Status = NeedsAttention` with the same icon pixmap; hosts differ in what they do with it.
- Light panels get the dark-panel icon (no theme detection).

## Verdict

PARTIAL: item, menu and click routing work against a stand-in watcher in the VM; a real host was not available.
