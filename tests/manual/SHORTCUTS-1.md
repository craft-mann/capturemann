# SHORTCUTS-1: change, conflict, reset (CAP-2.AC2, AC3)

Status: not run. Needs a real keyboard. Run on macOS, Windows 11, and Linux (X11 session; Wayland shows the desktop-managed note instead of Change buttons). Report in `verification/` (template in its README). Use a dev build or a second library (`CAPTUREMANN_CONFIG_DIR`), not your daily copy.

Privacy: no recording, no camera, no microphone. Pressing a shortcut that starts a capture is a failure of this script (step 3).

## Steps and pass criteria

1. Settings -> Shortcuts. Each row shows its keys and a Change button. Pass: 12 rows (the recording group may be hidden where recording is off).
2. Change "Open library" to a free combination with a letter (macOS Option+Shift+K; Windows/Linux Ctrl+Alt+Shift+K). Pass: the keys on the row change at once, nothing else changes; press the new combination from another app: the library opens; the old one (…+L) does nothing.
3. Choose Change on "Capture window" and press the combination of "Capture region" (…+1). Pass: the line "Already used by Capture region" shows, the shortcut is not changed, and NO capture overlay starts (before the fix the press started the capture and no message showed).
4. Choose Change, press a combination with a shifted digit (…+Shift+4). Pass: it is accepted and shown as 4 (before the fix: "That key can't be used"). Press it outside the app: it works.
5. Choose Change, press a bare key (K) -> "Hold a modifier key too". Press Esc -> recording ends. Pass: after Esc every shortcut still works (press …+L / the one from step 2 outside the app).
6. Choose Change, then click another page in the sidebar and come back, or switch to another app and back. Pass: all shortcuts work from other apps in the meantime (recording does not leave them released).
7. Pick a combination another app owns (for example one set in the OS keyboard settings). Pass: "Couldn't register — used by another app" and the old shortcut keeps working.
8. Quit and restart the app. Pass: the changed shortcut is still the new one and works.
9. Swap two shortcuts through Change (A to a free key, B to A's old keys, A to B's old keys), then "Reset to defaults" and confirm. Pass: both defaults work from another app, no row says "Couldn't register".
10. `capturemann` CLI: `capturemann settings get shortcuts.open_library` shows the new value after step 2.

Record the OS, version, keyboard layout (non-US layouts may refuse Shift+digit: report what the page said) and the result per step.
