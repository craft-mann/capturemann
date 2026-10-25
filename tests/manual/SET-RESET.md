# SET-RESET manual script: Settings -> Shortcuts "Reset to defaults" (board R16)

Covers what the unit tests cannot: the button in the page header, the confirmation window, the
real OS hotkeys moving back, and the toast. The decisions (which keys, which differ, what is
written, no conflict after a reset, an invalid stored value) are tested in
`apps/desktop/src/ui/settings/reset.rs` (`cargo test -p cm-desktop --lib reset`). Only the
Shortcuts page has this control (the R12 and R13 boards draw none). No recording is needed.

Use a second instance with its own data if you do not want to touch your shortcuts:
`CAPTUREMANN_CONFIG_DIR=<temp dir> CAPTUREMANN_LIBRARY=<temp dir> make run-bundle`-style launch.

1. **Disabled at defaults.** Open Settings -> Shortcuts with every shortcut at its default. The
   header shows "Reset to defaults" at the right, muted, with no pointer cursor. Clicking it
   does nothing (no window). With VoiceOver it reads "Reset to defaults, unavailable".
2. **Enabled after a change.** Change "Open library" to another combination (Change, press
   it). The button becomes a normal secondary button with the rotate icon.
3. **Ask first.** Press it. A confirmation window opens: title "Reset the keyboard shortcuts to
   their defaults?", a "Will change" row naming Open library, and the buttons Cancel and "Reset
   shortcuts". The primary button is the normal one (not red, no trash icon). Esc and Cancel
   close it and change nothing.
4. **Reset.** Press "Reset shortcuts". Expected: the row shows the default keys again (Option
   Shift L on macOS), the info toast "Keyboard shortcuts reset to their defaults" appears, and
   the button is disabled again. The new combination no longer opens the library from another
   app; the default one does (global hotkey, test from another app).
5. **Persisted.** Quit and start the app again: the page shows the defaults and the button is
   disabled. `capturemann settings get shortcuts.open_library` prints `alt+shift+l`.
6. **Several at once, with a swap.** Set Capture region to the keys of Capture window and
   Capture window to something else, change two more. Press Reset and confirm. All rows show
   their defaults, no "Two actions share a shortcut" line, no "Couldn't register" line, and
   every default hotkey works.
7. **Hand-edited bad value.** Quit; put `"shortcuts.pause_recording": "banana"` in
   `settings.json`; start. The page shows the default for Pause and the button is enabled.
   Reset and confirm: the file holds `alt+shift+p` and the button is disabled.
8. **Other settings untouched.** Before step 4 note the library path (General), the excluded
   apps (Privacy) and the Destinations. After the reset they are identical.
9. **Language.** Switch to Tieng Viet: the button, the window and the toast are in Vietnamese
   and nothing is cut off (the Vietnamese texts are a fuzzy draft awaiting review).

Pass: every expected result holds. Record the run in `verification/` (template in its README).
