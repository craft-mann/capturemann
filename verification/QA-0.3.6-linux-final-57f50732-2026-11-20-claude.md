# QA 0.3.6 final, Linux x86_64 (X11 and Wayland), PARTIAL

- **Date:** 2026-11-20. **Who:** Claude (QA agent), driven from the Mac. **Machine:** ThinkPad T490, Intel UHD 620, Ubuntu 24.04, GNOME 46, one display (1920x1080).
- **Files under test:** the unsigned candidate built on the laptop from commit `57f507327cbdf3921c52ba9c817b7f64aaedd921` (log `~/wt/release-57f-x86_64.log`: no warning lines, `check-package.sh` passed): `capturemann_0.3.6_amd64.deb` sha256 `94d90eb2075c524f47860c68f9cfbd0ca35c2e3e9d210f005c747e520deef22f` (the `.deb` pulled to the Mac has the same hash; installed with `dpkg -i`, tested from `/opt/capturemann/lib/capturemann`), tarball sha256 `16813cc99ab3f69ddca9c58a8784cebc4acd8f4837898b2917984be912a95969` (not installed, only hashed by the build). `main` HEAD was one docs-only commit after `57f50732`.
- **Isolation:** every run used its own `CAPTUREMANN_LIBRARY`, `CAPTUREMANN_CONFIG_DIR`, `CAPTUREMANN_SOCKET`, `CAPTUREMANN_SECRETS=file` (`cm-regress` makes one per case). The laptop user's default library (`~/Videos/CaptureMann`: 44 files, 13 captures, plus `~/.config/CaptureMann/settings.json`) was hashed before and after: **unchanged**.
- **Input and pictures:** X11 with `xdotool` and `import -window root`. Wayland with a scratch virtual absolute mouse and keyboard through `/dev/uinput` (a copy of the idea of `desk-uinput-mouse.py` with digits, letters and chords) and pictures from the desktop portal. The bar and sheets are plain X windows on X11 (visible to `import`). Pictures: `verification/QA-0.3.6-final-linux-shots/` (14 files, `x11-*` and `wl-*`).
- **Session switch:** `scripts/testlab/rec-session.sh` (AccountsService `Session=` then `systemctl restart gdm3`, auto-login brings the desktop back): X11 to Wayland and back. No reboot. The laptop is left on Ubuntu, **X11**, with the 0.3.6 `.deb` installed.
- **Capture exclusion:** Linux has none (the spec says so). Observed: on both sessions the bar closes before the region picker opens and is not in the picker's frozen frame (`x11-after-capture-bar.png` was taken after); a recording started from the bar starts after the bar has closed.

## Regression (`cm-regress`, installed `.deb`, `--commit 57f50732`, 0 failed everywhere)

| Tier and session | Passed | Failed | Skipped | Report |
| --- | --- | --- | --- | --- |
| standard, X11 | 57 | 0 | 7 | `L1-linux-0.3.6-regression-standard-x11-lab-57f50732-deb-2026-11-20-claude.md` |
| full, X11 | 64 | 0 | 16 | `L1-linux-0.3.6-regression-full-x11-lab-57f50732-deb-2026-11-20-claude.md` |
| hardware (`--allow screen` only), X11 | 64 | 0 | 24 | `L1-linux-0.3.6-regression-hardware-x11-lab-57f50732-deb-2026-11-20-claude.md` |
| standard, Wayland | 55 | 0 | 9 | `L1-linux-0.3.6-regression-standard-wayland-lab-57f50732-deb-2026-11-20-claude.md` |
| full, Wayland | 61 | 0 | 19 | `L1-linux-0.3.6-regression-full-wayland-lab-57f50732-deb-2026-11-20-claude.md` |
| `linux.deb_install` (package purged first, programs from a `dpkg -x` copy, `--installer` the same `.deb`) | 1 | 0 | 0 | `L1-linux-0.3.6-regression-full-x11-lab-57f50732-deb-install-2026-11-20-claude.md` |

Each report also has a `.json`. No failure, so nothing to explain as lab or product. Skips are the usual ones (cases "not implemented for linux", no second monitor, no model, no soak). The hardware tier has 2 passes fewer and 8 skips more than the 18 Nov run (66/22) because mic, sound and camera were not allowed this time (`rec.mic`, `rec.system_audio`, `rec.camera`, `rec.soak` skip). The suite has no Quick Capture case; the bar was checked by hand below.

## Results by item

PASS = seen working in this run. FAIL = defect (numbered in the next section).

| Item | X11 | Wayland |
| --- | --- | --- |
| Opens with Ctrl+Alt+Shift+C, closes with it again; three quick presses leave it open (toggle) | PASS | n/a (no GlobalShortcuts portal on GNOME 46; the app says so in a toast on every start) |
| Opens from the tray item (dbusmenu `Quick Capture...`, id 16) | PASS | PASS |
| Placement | PASS near the pointer, clamped to the display: pointer (960,540) -> window +529+540; (300,700) -> +14+700; (1800,100) -> +1043+100; (1900,1070) -> +1043+968; the tray item opens it at the pointer too (`x11-bar-opened-at-pointer.png`) | **FAIL N-4** top-left of the screen (about x 117, y 82 for the window), not centred |
| Keyboard goes to the bar on open | PASS: 20 of 20 opens with the focus on a text editor, Firefox or the library, after clicks and Alt+Tab (kind keys 1/2/3 reached the bar every time, Esc closed it, and nothing was typed into the editor); 5 open/Esc/open pairs without a pause | PASS: 8 of 8 (kind keys switched the kind) |
| Keys 1 2 3, R W D L, T, P, O, Tab ring, Enter, Esc | PASS (Tab ring shows the tooltip, `x11-*`); Enter on Audio started an audio recording (see below) | PASS for 1 2 3, O, T, P, Esc, Enter |
| Collapse with O and chevron; `collapsed` persisted; hidden keys do nothing; width 463 vs 867 | PASS: left edge kept (+529), `collapsed: true` written, reopened collapsed, still collapsed after quitting and restarting the app, expanded after the key was removed from `quick_capture.json`; W D T did nothing when collapsed | PASS (toggle, persistence on reopen) |
| Delay menu (T), arrows, Return, Esc goes back | PASS: Down steps Off, 3, 5 (badge follows), Return closes the menu, Esc in the menu leaves the bar open | PASS |
| Picker (P): rows, Down/Enter, Ctrl+2..4, Esc | PASS: five rows plus "Save current as preset..." and "Edit presets..." all visible (window 867x445); Down Down Return picked Docs shot; Ctrl+2/3/4 picked Docs shot / Demo video / Meeting | PASS (six rows visible with the added preset) |
| Ctrl+1 (Bug report) | **FAIL N-1** | **FAIL N-1** |
| Name screen: typing, Save, window shrinks | PASS: "q36 caps 1 w" typed (letters, digits, spaces), kind/target/options unchanged, Return saved it (`presets.json` and Settings list it), window went 867x280 -> 867x100 | PASS (typing "wl q36 name 1" accepted) |
| Name screen: Esc returns to the picker | returns, but **FAIL N-2** (bar deaf afterwards) | returns, but **FAIL N-2** |
| Click elsewhere (library, Firefox): bar stays open | PASS (stays; keys then go to the clicked window, as expected) | PASS (stays, but see N-6) |
| Drag by the background | **FAIL N-3** | PASS: dragged (380,160) -> (680,360), bar followed and stayed open (`wl-bar-dragged.png`) |
| Screenshot from the bar (Region, Copy and Editor off) | PASS: overlay, 320x200 drag, Return, library has the capture; bar gone before the overlay | PASS: the app's own overlay with the frozen frame worked, 19 KB capture saved |
| After-capture bar | PASS: under the area (669x52 at +828+710), C put a 4,926-byte `image/png` on the clipboard and the bar became the toast, Esc closed it; no stale "Capture removed" toast with the bar | PASS for C (toast "Copied to clipboard", `image/png` on the clipboard); **FAIL N-5**: placement |
| Video 5 s (Display, Mic off, Webcam off, Countdown off) | PASS: `record status` showed screen and cursor tracks, `record stop` saved a 5 s take | PASS: screen track, 5 s, no share dialog |
| Audio (Mic) | PASS by accident, then confirmed: Enter on the Audio kind started a recording (tracks: mic, "Audio recording 22.04", 11 s, stopped with `capturemann record stop`); the mic worked without any prompt (level read 0 dBFS in a quiet room); the earlier steps typed `2 3 r w d Return` into a bar I believed focused | not run |
| Preview tag and tooltips (name, state, key, "anywhere" chord) | PASS (hover and Tab ring) | not run |
| Settings unchanged by the bar (QC-1.AC6) | PASS: after Delay 3 s and Countdown 0 in the bar, Settings > Capture & recording still showed Countdown 3 s and "Before a screenshot: No delay" | not run |
| Edit preset sheet (Settings > Capture & recording > Presets > Edit) | PASS: Name shows the preset's name (selected), typing works, Esc closes it. **Note N-7**: Tab does not move to the next field | not run |
| Shortcut rebind conflict (Open Quick Capture > Change > Launcher's chord) | PASS: "Already used by Open launcher", the Launcher did not open | n/a |
| Two rows below 900 points | NOT RUN (1920x1080 only; switching the display was not done) | NOT RUN |

## Defects

Severity: S1 blocks use; S2 a feature does not work as specified here; S3 wrong or incomplete behaviour with a way round; S4 cosmetic or note.

| ID | Sev | Where | Defect and steps |
| --- | --- | --- | --- |
| N-1 | **S2** | X11, Wayland | **Ctrl+1 does not pick the first preset; it raises the library window.** Open the bar, press Ctrl+1: log `activation requested (studio brought to front)`, the preset is not picked, on X11 the bar loses the keyboard (afterwards 1/2/3/Esc do nothing until it is reopened), on Wayland the library is raised over the bar (bar not visible, `wl-ctrl1-library-raised-bar-gone.png`). Ctrl+2, 3, 4 work (`x11-ctrl2-3-4-pick-ctrl1-opens-library.png`). Cause: `apps/desktop/src/ui/studio.rs:88` binds `secondary-1` (Ctrl+1 on Linux and Windows) to `ShowLibrary` with no context, and it wins over the bar's key handler. Windows and macOS reports passed AC17 for Ctrl+1, so the order differs there (macOS uses Cmd for `secondary`). QC-1.AC17 is not met for the first preset on Linux. |
| N-2 | **S2** | X11, Wayland | **After Esc on the name screen ("Save current as preset...") the bar no longer receives the keyboard.** Steps: P, Down to "Save current as preset...", Return, (type), Esc: the picker shows again (window 867x445, the F-3 fix itself works) but Esc, Down, O, Return do nothing. Reproduced 3 times on X11 (also with no text typed) and once on Wayland (`x11-after-esc-from-name-keys-dead.png`, `wl-after-esc-from-name-picker-keys-dead.png`). On Wayland a click on the bar background gave the keys back; on X11 a click did not (clicks on the buttons work). Closing with the shortcut and reopening works. Save with Return works and leaves the keyboard working. Probably the name field's focus is dropped and the bar's own focus handle is not given back (not looked up in code). |
| N-3 | S3 | X11 | **The bar cannot be dragged on X11 (QC-1.AC20).** `xdotool mousedown` on the background (four points: between the groups, the logo, near the action, near the chevron), 8 moves, `mouseup`: the window stays at +529+300 each time. The bar's mouse-down calls `window.start_window_move()` (`apps/desktop/src/ui/quick_window.rs:1267`), which asks the window manager to move the window; the bar is an override-redirect window on X11 (`xwininfo`), which the window manager ignores. Buttons and clicks work. Wayland moves (above). |
| N-4 | S3 | Wayland | **The bar opens at the top-left, not centred on the display** (QC-1: "on Wayland, which gives no position, it opens centred on the display"). From the tray item: window at about (117,82); the same with the dock button earlier (L-2 on Wayland is not fixed). `wl-picker-all-rows.png`. |
| N-5 | S3 | Wayland | **The after-capture bar is not under the area**: for an area at (1000..1320, 600..800) it opened at the top-left (about x 167, y 133), over the library's header (`wl-after-capture-bar-top-left.png`). X11 is correct. |
| N-6 | S4 | Wayland | **The bar is an ordinary window and goes behind the library when you click the library** (a click on a capture card raised the library over the bar, which stayed open, partly hidden). It stays open as specified, but a user may think it has gone. |
| N-7 | S4 | X11 | In the Edit preset sheet Tab does not move the focus to the next field (typing after Tab went into the Name field). |
| N-8 | S4 | note | On the X11 session `xdotool getactivewindow` printed nothing while the bar was open (the focus is set by the app on the override-redirect window, `surfaces.rs:take_keyboard_x11`); keys do reach it (20 of 20). Not a defect, but tools that look for the active window will not see the bar. |

## L-1 to L-11 from the 18 Nov pass (c69ef16e), re-checked where cheap

| ID | Now | Evidence |
| --- | --- | --- |
| L-1 keyboard on X11 | **Fixed** for the bar and the after-capture bar and the Edit sheet; two remaining holes are N-1 and N-2 | 20/20 opens; C on the after bar; typing in the sheet |
| L-2 placement | **X11 fixed**; **Wayland open** (N-4) | table above |
| L-3 picker cut off | **Fixed** (X11 and Wayland) | `x11-picker-all-rows.png`, `wl-picker-all-rows.png` |
| L-4 Edit sheet name empty | **Fixed** | `x11-edit-preset-sheet-typing.png` (sheet scrolling not re-checked) |
| L-5 Long Shot `restored: true` while the page was not restored | NOT RUN | needs a scrolling window and an Allow dialog |
| L-6 stale "Capture removed" toast with the after bar | **Not seen** (2 captures, only the 669x52 bar window opened) | `xwininfo` listing |
| L-7 rebind cannot show "Already used" | **Fixed** | `x11-rebind-conflict.png` |
| L-8 panel visible to a Wayland portal picture | Same as before: the bar is a normal Wayland window; it closes before a capture starts (frozen frame has no bar) | spec says no exclusion on Linux |
| L-9 picking a preset overwrites remembered options | **Fixed**: after picking Docs shot (Window) `quick_capture.json` still said `shot_target: region` | file read right after the pick |
| L-10 pseudo-locale wrapping | NOT RUN | |
| L-11 audio from the bar starts with a countdown | NOT CHECKED (I did not read the status inside the first seconds) | |

## What I did not do

Two-row layout (single 1920x1080 display, no safe way to switch), Window and Display and Long Shot targets from the bar, the Delay countdown pill, after-capture bar keys E, S, T, L, R and the bottom-edge placement, Auto-redact from the bar (it was off), Audio on Wayland, Vietnamese and light theme, the Send feedback page, L-5, L-10, L-11, hotkey-taken, any AMD/NVIDIA, KDE, a second monitor, aarch64, the mic/sound/camera cases of `cm-regress` (not allowed this time).

## Notes for the maintainer

- The test copy of the app on Wayland logged `Tried to initialize the wayland data control protocol clipboard, but failed. Falling back` for each Copy; the copy still worked (image/png on the clipboard).
- In the 4 captures taken with the bar the area picker needed Return after the drag (both sessions); that is the picker's own behaviour, not the bar's.
- `rec-session.sh x11|wayland` printed the old session type on its first run (it checks before GDM has finished restarting); I ran it again and checked with `loginctl`. Four GDM restarts in total.

## Verdict

PARTIAL. `cm-regress` has 0 failures in all six runs. On X11 the 18 Nov keyboard defect is fixed (20/20 opens) and the placement, picker, Edit sheet and rebind fixes hold; the QC-1 flows I ran work on both sessions. Two keyboard defects remain on both sessions (N-1 Ctrl+1, N-2 Esc from the name screen), plus drag on X11 (N-3) and the Wayland placement of the bar and the after-capture bar (N-4, N-5).

## Cleanup

Removed my scratch folder on the laptop (`~/lab/q36f`: private config and library, helpers, pictures, the `dpkg -x` copy), the three portal pictures I made in `~/Pictures` (`Screenshot-24..26.png`), the leftover helper scripts `~/lab/qa57-*.sh` of the earlier agent, my lock, and every test window and process. Kept `~/wt` (build cache, release-out) including `~/wt/qa57-sherpa-cache` and the old `~/wt/qa036-*.log`. `~/lab/capturemann_0.3.6_amd64.deb` (pushed by the install case, same file name as the 18 Nov pass) now holds this candidate. The `.deb` 0.3.6 (57f50732) is installed; the session is X11.
