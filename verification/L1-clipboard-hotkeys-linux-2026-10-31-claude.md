# L1 clipboard, global hotkeys and system integration on Linux report

- **Date:** 2026-10-31
- **Who:** Claude (agent), nothing by a person; VM only
- **Machine:** QEMU VM on a Mac, Ubuntu 24.04 aarch64, 4 CPUs, llvmpipe. X11: a private Xvfb :98 (1280x800) with openbox. Wayland: the headless GNOME 46 session (`gnome-shell --headless --wayland --no-x11`, 1280x720, so **no XWayland**), xdg-desktop-portal 1.18 without the GlobalShortcuts interface.
- **Build:** branch `worktree-agent-adc41e2c73c473f57`, commits `fix(clipboard)`, `feat(shortcuts)`, `fix(linux)` (see `git log`); release profile, `--no-default-features` (no sherpa); binaries copied to `/tmp/cmclip-bin/cmclip-desktop` (other agents `pkill -x cm-desktop` on this VM)
- **Run with:** `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_SOCKET` in /tmp; readers `xclip -o` and `wl-paste`; `xdotool key` for X11 keys
- **Fixture:** none (the screen is the subject). A fake `org.freedesktop.portal.GlobalShortcuts` service (about 100 lines of Python/Gio on a private dbus-daemon: CreateSession and BindShortcuts through Request objects, an Activated signal, `version` property) stands in for the missing portal. It is a test aid written by the same agent from the portal spec, not a real KDE or GNOME portal.

## Steps

| Step | Result | Notes |
| --- | --- | --- |
| 1 Before the fix: `set_image` then read back with `xclip` (X11) | FAIL (found a bug) | `Target image/png not available`: arboard on X11 drops the selection when its `Clipboard` value is dropped (no clipboard manager in the VM). The old code created one per copy, so every copy was lost at once while the toast said "Copied to clipboard" (the earlier L1 report saw arboard's "could not hand the clipboard contents over" warning). |
| 2 After the fix (handle kept for the process life): ignored test `real_clipboard_round_trip`, X11 | PASS | a 5x4 PNG set through `platform_clipboard::set_image`, read back by `xclip -t image/png` with the same pixels |
| 3 X11, CLI path: `capturemann capture screenshot --display 60 --copy` against the running app (twice) | PASS | "copied to the clipboard"; after the CLI exited `xclip -o -t image/png` gave `PNG image data, 1280 x 800`, targets `image/png TARGETS SAVE_TARGETS`; the app owns the selection |
| 4 Wayland (GNOME headless, no XWayland), CLI `--copy` | PASS (truthful failure) | the shot is saved; the answer is `warning: Not copied to the clipboard: cannot reach the clipboard: ... X11 server connection timed out ...; xclip ...` and no "copied" line. `wl-copy` is installed but exits 1 in this VM ("This seat has no keyboard": the headless shell has no keyboard seat), so the helper fallback was also a failure here, reported as such. |
| 5 Wayland positive path (XWayland bridge to Wayland apps; `wlr-data-control` on wlroots/KDE; `wl-copy` with a focus) | NOT TRIED | this VM's shell has no XWayland, no keyboard seat and no data-control; `wl-paste` also returned nothing there. Needs a GNOME with XWayland, KDE and a wlroots compositor. |
| 6 Editor Copy and the post-capture toast | not run in the GUI | both go through the same `Clipboard` hook: `Backend::copy_bytes` maps an error to "Copied to library only" and the editor's Copy to "Couldn't copy" (existing code, `flows.rs` tests); the hook now returns real errors. Not clicked in the VM. |
| 7 X11 global hotkeys: start the app, `xdotool key ctrl+alt+shift+space` | PASS | the launcher window opened (window count 2 -> 3); no "could not register" lines in the log. Ctrl+Alt+Shift+Q (no shortcut) did nothing. |
| 8 Second launch on X11: `cmclip-desktop --launcher` three times | PASS | exit 0 each time, one app instance, launcher opened, closed, opened (toggle); plain `cmclip-desktop` exits 0 and shows the app; `cmclip-desktop --capture display` made the running app start `cm-overlay display` |
| 9 Wayland session on GNOME 46 (no portal): start | PASS | log `no GlobalShortcuts portal: A portal frontend implementing org.freedesktop.portal.GlobalShortcuts was not found`; nothing registered; the toast "Global shortcuts are not available here / Global shortcuts need X11 or a desktop with the GlobalShortcuts portal; use the launcher or the CLI" is on screen (screenshot of the VM desktop, stored by the agent only in the scratchpad, not in the repo) |
| 10 Wayland + fake GlobalShortcuts portal (private bus) | PASS (against the fake) | the fake saw `CreateSession` then `BindShortcuts` with 12 shortcuts and triggers `CTRL+ALT+SHIFT+1`, `...+space` and so on; the app logged the bound list; the fake's Activated for `shortcuts.open_launcher` opened the launcher window |
| 11 Settings -> Shortcuts page text ("Not available on this desktop" rows, the fallback note, "Your desktop manages these shortcuts") | NOT SEEN | the string and layout are in the code and the page builds; clicking the page in the Wayland VM hit the GNOME hot corner instead (the pointer helper), so it was not looked at. |
| 12 Unit tests | PASS | `cargo test --release -p cm-desktop --lib`: 1408 passed, 2 failed (`publish_sheet::input_tests::clicking_a_field_then_typing_fills_it`, `recording_editor::audio_tests::play_sample_plays_the_speakers_longest_turn_and_stops_at_its_end`); neither touches code changed here; **not re-run on the base commit**, so "pre-existing" is not shown |
| 13 `cargo clippy -p cm-desktop --all-targets --no-default-features -- -D warnings` | PASS on Linux and on macOS | macOS run was before the last small edits (the `--background` tray check and one test), Linux run after the hotkey change and before `files.rs` |
| 14 `cargo deny check`, `cargo fmt --all -- --check`, `cm-i18n check`/`unwrapped`, `scripts/check_target_os.py` | PASS | deny: advisories, bans, licenses, sources ok (new crates `wl-clipboard-rs`, `os_pipe`, `petgraph`, `tree_magic_mini`: MIT or Apache-2.0); `target_os` baseline unchanged |
| 15 `xdg-open` for links, Open and "Show in folder" (`recording/files.rs`) | NOT RUN | unit test covers the folder choice; the spawn and the "xdg-open is not installed" message were not exercised |
| 16 Launch at login on Linux | not run | the autostart entry text and path were read and the existing tests cover them; a login was not done |

## What was not tried

KDE (Wayland and X11), a GNOME with the GlobalShortcuts portal (46.x does not have it; newer GNOME and KDE do), a real keyboard (all keys were `xdotool`), a real XWayland-to-Wayland clipboard handover, wlroots, a clipboard manager, quitting the app and pasting afterwards (the X11 copy is gone then by design), `wl-copy` with a focused window, the GUI Copy button and toast, the Shortcuts page on screen, `xdg-open` itself, an OAuth browser open with no `xdg-open`.

## Reviewed, found fine

- Launch at login: the XDG autostart entry (`Exec` quoting follows the Desktop Entry spec, `%` doubled), the `$XDG_CONFIG_HOME` base, the "installed copy only" rule.
- Single instance: an advisory `flock` on `instance.lock` works on Linux; a stale `instance.sock` is replaced because the lock holder is the only listener.
- The desktop app's default library folder uses `dirs::video_dir()` (reads `user-dirs.dirs`, so a localized `~/Vidéos` is honoured) and the editor's Save as uses `dirs::picture_dir()`.
- No system notifications are used anywhere (only in-app toasts), so `notify-rust` / the Notification portal is not needed.

## Open items (not fixed here)

1. `cm_platform::dirs::video_dir()` returns `~/Videos` without reading `xdg-user-dirs`, while the desktop app uses `dirs::video_dir()`: on a system with a localized folder name the headless `cm-daemon` and the app pick different default libraries (not in the allowed files).
2. `cm-auth` opens the OAuth URL with `open::that`: with no `xdg-open`, `gio` and so on it fails with a bare error kind ("entity not found") that does not say what to install.
3. `cm-overlay` copies a picked colour with its own `arboard` handle that is dropped at once: on X11 without a clipboard manager that copy is lost the same way.
4. Keycaps on Linux read "Win" for the Super key (the shortcut model has only Mac and Windows) and the defaults are Ctrl+Alt+Shift+key, which some desktops already use; a Linux platform value would fix both.
5. Wayland global shortcuts: the portal may bind other keys than asked for and the app shows its own chords, not the ones the desktop chose (`trigger_description` is only logged); the Settings "Change" button only changes the preferred key for the next start; ConfigureShortcuts is not wired.
6. The tray is another agent's work; with no tray, "Launch at login" now opens the window, which is a stop-gap.
7. The headless `cm-daemon` has no clipboard (`--copy` answers "no clipboard is available in this daemon", truthful), because an X11 selection cannot outlive a one-shot process.
8. A copy made through `wl-copy` or `xclip` as a fallback survives the app; a copy through arboard does not (X11), by design.
9. AppImage: the autostart entry would point at the temporary mount path; the packaging agent should write the AppImage path.

## Result

PARTIAL: X11 clipboard and global hotkeys work and were checked end to end; the second-launch route works on X11; on GNOME 46 Wayland the app now fails truthfully and tells the user the fallback; the Wayland portal route works against a fake portal only; the Wayland clipboard success path and everything on KDE are untested.
