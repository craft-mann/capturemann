# LINUX-1 — The early Linux build on a real X11 and a real Wayland desktop (manual)

Covers the early Linux build (screenshots, library, editor; recording is not built in yet). Run it twice, once on a
real **X11** session and once on a real **Wayland** session (GNOME 46 or newer, and KDE Plasma 6 if you have it).
A VM is fine for a first pass; say so in the report. Platform rows: `specs/platform-acceptance.md` (CAP-1, CAP-2,
CAP-4, LIB-2, REC-1 to AUD-4, PRM-1). The expected-unavailable list at the end is part of the pass criteria.

Privacy: step 7 puts a fake token in a test keyring or file; use only the fake value. Delete the test library
and `CAPTUREMANN_CONFIG_DIR` folder afterwards.

Setup. Use throw-away folders so your real data is untouched:
`export CAPTUREMANN_CONFIG_DIR=$HOME/cm-linux1/config CAPTUREMANN_LIBRARY=$HOME/cm-linux1/library`.
Write down: distribution and version, desktop and version, `echo $XDG_SESSION_TYPE`, GPU.

## 1. Install the tarball
1. Unpack `capturemann-<version>-linux-<arch>.tar.gz` (check the `.sha256` with `sha256sum -c`). Run
   `./install.sh` from the unpacked folder (default prefix `~/.local`; add `~/.local/bin` to `PATH` if needed).
   **Pass:** the script lists what it installed; `capturemann --version` prints the version; the app appears in the
   launcher grid with its icon.
2. `./install.sh --uninstall`, then install again. **Pass:** uninstall removes only the installed files; your
   library and settings folders are untouched.

## 2. What the system offers (before anything else)
3. Run `capturemann diagnostics`. **Pass:** the System section lists `session X11` or `session Wayland (<desktop>)`,
   the portal line with ScreenCast and Screenshot versions (`v4` or newer expected), `PipeWire yes/no`,
   `Secret Service yes/no`, the VAAPI device line; a "Missing on this system" section appears only if
   something is missing and each line is one plain sentence. No line talks about macOS or Screen Recording grants.
4. To see a gap on purpose: `systemctl --user stop pipewire pipewire-pulse wireplumber` (restore with `start`),
   run `capturemann diagnostics` again. **Pass:** a line says recording needs PipeWire (and "only has PulseAudio"
   when `pulse/native` still exists).

## 3. First launch
5. Start `cm-desktop` (launcher or terminal). **Pass:** the window opens in under 5 s; the library shows an empty
   state; the log `$CAPTUREMANN_CONFIG_DIR/logs/desktop.log` has `desktop <version> started`.
6. Open the permissions page (first run shows it; otherwise Settings -> Permissions). **Pass:** the heading text
   says Linux does not ask per app (no "macOS keeps these under your control"); there are no Screen Recording or
   Input Monitoring cards; a block "What this system is missing" lists the same gaps as step 3, or says nothing
   is missing for screenshots.

## 4. Secrets: keyring present and absent
7. With a keyring running (GNOME Keyring or KWallet): `capturemann diagnostics` shows `Secret Service yes` and
   "secrets in the system credential store". **Pass:** no unlock prompt appears just from running diagnostics or
   opening the permissions page. Then store a fake key in Settings -> AI (any provider field, fake value
   `fake-key-1`). **Pass:** the keyring may ask to unlock (that is its own dialog); afterwards the key persists
   across an app restart.
8. Without a Secret Service (log in to a bare session, or `systemctl --user stop gnome-keyring-daemon.socket
   gnome-keyring-daemon.service` and kill the daemon): **Pass:** the permissions page lists the missing Secret
   Service with the `CAPTUREMANN_SECRETS=file` hint; storing a key without the variable fails with a clear
   message (error code `no_keyring`), not a crash.
9. Restart with `CAPTUREMANN_SECRETS=file`. **Pass:** diagnostics says "secrets in a file (owner-only, not
   encrypted)"; the key is stored; `ls -l` shows the secrets file readable by you only (mode 600).

## 5. Screenshots
10. **X11:** take a display screenshot (or use the tray menu, see step 19; or `capturemann capture
    screenshot --display 1`). **Pass:** a PNG lands in the library at the physical pixel size of the screen.
11. **X11:** region screenshot with the overlay picker; Esc cancels. **Pass:** the saved image shows exactly the
    selected region; Esc saves nothing.
12. **X11:** window screenshot of a terminal. **Pass:** the window's contents, titled in the library.
13. **Wayland:** display screenshot. **Pass:** the desktop's own portal dialog (or an unprompted capture, if
    you chose "allow" before) appears; after accepting, one image per monitor is listed by `capturemann targets`
    (name, position, scale as in the desktop's Displays settings) and `capture screenshot --display N` saves
    exactly that monitor. Declining gives a clear "permission denied" style message and saves nothing.
14. **Wayland:** region screenshot from the launcher (the region shortcut). **Pass:** a dimmed, frozen picture of
    each monitor fills the screen (a fullscreen CaptureMann window per monitor); dragging selects a region, Enter
    saves it, Esc saves nothing; the saved image is the chosen region. With two monitors of different scale, note
    whether the overlay matches each monitor (record it; mixed DPI is untested).
14a. **Wayland:** window screenshot from the launcher, and `capturemann capture screenshot --pick`. **Pass:** the
    desktop's own picker (on GNOME the screenshot chooser with Selection / Screen / Window) appears; choosing
    a window saves that window; Esc saves nothing and the command exits 3. `capturemann capture screenshot --window
    x` answers `not_implemented` and names `--pick`; `capturemann targets` says the desktop does not list windows.
14b. **X11:** open `xclock` and `xterm`; `capturemann targets` lists them **with titles**; `capture screenshot
    --window xclock` saves the window.
15. Open the screenshot in the editor, add an arrow and a blur, copy to clipboard (X11: paste into another app;
    Wayland: paste into another app too). **Pass:** the pasted image is the edited one. The toast says "Copied to
    clipboard" only if the paste works: on a session where no clipboard can be reached (no X11 or XWayland, no
    `wl-copy`) it must say "Copied to library only" or "Couldn't copy" instead. Also from a terminal:
    `capturemann capture screenshot --display <id> --copy`, then `xclip -o -selection clipboard -t image/png | file -`
    (X11) or `wl-paste --type image/png | file -` (Wayland) shows a PNG of the screen's size while the app runs.
    The copy lives as long as the app does; after quitting it, X11 pastes nothing (a clipboard manager may keep it).

## 5b. Global shortcuts and desktop shortcuts
15a. **X11:** with the app running, press Ctrl+Alt+Shift+Space. **Pass:** the launcher opens; press again, it closes.
    Ctrl+Alt+Shift+1 starts a region capture. Settings -> Shortcuts shows no "Couldn't register" row, and a
    combination another app owns (try one the desktop uses) shows "Couldn't register - used by another app".
15b. **Wayland on KDE or a GNOME with the GlobalShortcuts portal:** at start the desktop asks to allow the
    shortcuts (or shows them in its keyboard settings). **Pass:** pressing the combination the desktop lists opens the
    launcher; Settings -> Shortcuts says the desktop manages the keys.
15c. **Wayland without the portal (GNOME 46):** **Pass:** a toast "Global shortcuts are not available here" at
    start; every row of Settings -> Shortcuts says "Not available on this desktop" and a note explains the fallback.
    Then in the desktop's keyboard settings add a custom shortcut with the command `cm-desktop --launcher` (and
    one with `cm-desktop --capture region`). **Pass:** the shortcut opens the launcher in the running app (a second
    press closes it) and starts the region capture; no second app window or tray icon appears.
15d. Run `cm-desktop --library` and plain `cm-desktop` while the app is running. **Pass:** both exit at once and the
    running app shows its library window.

## 6. The portal permission dialog and its memory
16. **Wayland:** repeat 13 twice. **Pass:** note whether the second capture asks again (GNOME asks every time for
    screenshots unless the app has a saved permission; KDE may remember). Record the behavior; neither is a failure.

## 7. Must show as unavailable (all sessions)
17. **Pass** when each of these says plainly that it is not available, instead of failing after a Start button:
    - Record in the dock, the Record shortcut and the launcher command: toast "Recording is not available yet"
      (the launcher row says "Not available yet"). No recording sheet, countdown or HUD opens.
    - Microphone and audio recording (the mic button in the dock): the same toast.
    - Tray icon on GNOME without the AppIndicator extension: the toast "Menu bar icon unavailable" explains how to
      turn the extension on; no crash; the launcher window stays reachable.
    - Global hotkeys on GNOME 46 (no GlobalShortcuts portal): not registered; in-app shortcuts work; a toast and
      the Shortcuts page say so (step 15c).
    - "Show in folder" and "Open" of a capture or recording, and a share link: open through `xdg-open`; without
      xdg-utils the error says to install it.
    - Launch at login: with no tray the app started by autostart (`--background`) opens its window instead of
      running unseen.
18. With the app running, `capturemann record start` (CLI) and the `start_recording` MCP tool. **Pass:** each answers
    with a clear error (not a crash, not a half-created bundle in the library). Record what the headless
    `cm-daemon` answers too; its gate is separate from the app's and is not changed by the early-build work.

## 8. Tray icon (StatusNotifierItem)
19. **KDE, or GNOME with "AppIndicator and KStatusNotifierItem Support" enabled (Ubuntu: `gnome-extensions enable
    ubuntu-appindicators@ubuntu.com`, log out and in), or XFCE/Cinnamon/MATE:** start the app. **Pass:** the CaptureMann
    icon appears in the panel; right click (GNOME: click) opens the menu: Capture region / window / display,
    Capture delay (None, 3, 5, 10 s with one check mark), Record\u2026 / Record audio / Pause / Stop (Record\u2026 and
    Record audio say "not available yet" on Linux for now; Pause and Stop are greyed), Open library, Open launcher, Recent
    captures, Settings, Permissions\u2026, Quit CaptureMann. Hover shows the tooltip "CaptureMann".
20. Click "Capture region" in the menu, then "Capture delay > 5 s" and look at the menu again. **Pass:** the region
    picker opens; the check mark moved to 5 s; take a screenshot and open the menu: "Recent captures" lists it, and
    clicking it opens the editor. "Open launcher" opens the launcher; "Quit CaptureMann" quits and the icon goes away.
21. Left click on KDE (Activate). **Pass:** the launcher opens.
22. Change the UI language in Settings. **Pass:** the menu texts change without a restart.
23. Restart the shell's tray host while the app runs (`gnome-extensions disable` then `enable` the extension, or
    `kquitapp6 plasmashell && plasmashell`). **Pass:** the icon returns by itself.

## Result
Store the report in `verification/` (template in its README): list the distribution, desktop, session type, real or
VM, and pass/fail per step. Do not call anything "verified" without it.
