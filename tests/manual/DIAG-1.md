# DIAG-1 — Log file and Copy diagnostics (manual)

Needs the dev app (`make bundle-dev`, `~/Applications/CaptureMann-dev.app`). Use a throw-away config
folder if you do not want to touch your real logs: `CAPTUREMANN_CONFIG_DIR=/tmp/cm-diag open ...`.
Privacy: this script types a fake token into nothing that is stored; use only the fake value below.

1. Start the app. Open `<config folder>/logs/desktop.log`. **Pass:** it has a line `desktop <version> started`.
2. Settings → About. **Pass:** a Diagnostics card is there and no diagnostics text is shown yet.
3. Press **Show diagnostics**. **Pass:** a box shows the sections App, System, Permissions, Models and engines,
   Settings, Library, Last recording, Warnings and errors; the chip and macOS version match About This Mac.
4. Read the whole box. **Pass:** no token, no recording title, no transcript text; paths start with `~`.
5. Press **Copy**, paste into a text editor. **Pass:** the pasted text is the same as the box; the button
   showed "Copied" for a moment.
6. Press **Reveal log folder**. **Pass:** Finder opens the `logs` folder.
7. Redaction check: in Terminal run `capturemann settings set ai.languages "vi,en"` then press **Refresh**.
   **Pass:** `ai.languages = "vi,en"` appears under Settings. Then confirm no line of the log file contains
   your user name: `grep -c "$USER" <config folder>/logs/*.log` (expect 0 for paths; the word may appear in other text).
8. Unclean exit: `pkill -9 cm-desktop`, start the app again. **Pass:** `desktop.log` has one warning
   "the previous run of desktop did not end normally"; quit normally and start again: no second warning.
9. Rejected recording settings (issue 16 case, only if you can set a platform-rejected size): record 5 s, then
   Show diagnostics. **Pass:** the Last recording section shows the codec and size really used.
10. Offline: turn Wi-Fi off, press **Show diagnostics**. **Pass:** the same text appears.

Store the run in `verification/` (template in its README) before calling DIAG-1 verified.
