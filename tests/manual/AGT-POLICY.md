# AGT-POLICY manual script: per-tool permissions (Allow / Ask first)

Covers AGT-2.AC7 to AC14 (maintainer decision OD-24, Oct 25, 2026) where automation cannot see the real
window. The daemon rules, the setting, the dialog content and the Agents page model are tested with
fakes (`cargo test -p cm-daemon --test tool_policy`, `cargo test -p cm-desktop --lib agt_2`); this
script is the by-eye run of the real app.

## Status of the author's runs

| Item | Status |
| --- | --- |
| Rules, setting, effective tier, generic dialog data, `start_recording` Allow path (fakes) | automated, passing |
| The Agents page controls, the generic dialog window, the Allow-start confirm, the toast | NOT RUN by eye |

## Setup

- A bundled app (`make bundle`, run by the user; never over a copy that is running) or `cargo run
  -p cm-desktop`. Use a throwaway config and library so nothing real is touched:
  `CAPTUREMANN_CONFIG_DIR=$(mktemp -d) CAPTUREMANN_LIBRARY=$(mktemp -d) CAPTUREMANN_SOCKET=/tmp/cm-policy.sock`.
  Point the CLI at the same socket (`export CAPTUREMANN_SOCKET=/tmp/cm-policy.sock`).
- Check 6 needs the recording permissions (Screen Recording). It records your screen for a few
  seconds and deletes the take: only run it when you are ready for that.
- Open Settings → Agents & CLI. Keep the settings file at `$CAPTUREMANN_CONFIG_DIR/settings.json`.

## Checks

1. **The list.** Every tool is listed once, with its name in mono under its title.
   Pass: tools that change something (Take screenshots, Annotate captures, Change settings ...) show
   an Allow / Ask first control with Allow selected; reading tools (List the library, Read settings
   ...) show an Allow pill and a lock, and hovering the lock says "Reading never needs asking.";
   Publish a video and Delete a capture show Ask first and a lock ("Publishing and deleting always
   ask first."); Start recording shows Ask first with a control (not locked). No row says "Changed"
   and there is no "Reset to defaults" button.
2. **Tighten.** Set Take screenshots (`capture_screenshot`) to Ask first.
   Pass: the row shows a "Changed" marker and a "Reset to defaults" button appears. Quit and reopen
   the app: the row is still Ask first.
3. **An agent is asked.** In a terminal run `capturemann capture screenshot --display 1`.
   Pass: a confirmation window opens with the agent chip ("Requested by Capturemann Cli" or the
   client name), the title "Allow this action?", a sentence naming the tool ("... is asking to run
   "Take screenshots". You set this tool to Ask first ..."), a Tool row and a Target row, Cancel
   focused, and a primary "Allow". Nothing is captured while it is open; the command waits.
   Press Allow after 1 s: the command prints the capture id and exits 0.
4. **Cancel.** Run the command again and click Cancel.
   Pass: the command fails and `echo $?` prints `3`; with `--json` the error has code
   `user_declined` and `data.reason` `ask_first`; no capture was added to the library. Doing nothing
   for 5 minutes gives `confirmation_timeout` (also exit 3); you can shorten the wait in a test
   build, not by hand.
5. **The app is never asked.** Take a screenshot from the app (the capture dock or the shortcut).
   Pass: it is captured with no dialog, with `capture_screenshot` still set to Ask first.
6. **Loosen start_recording.** Set Start recording to Allow.
   Pass: before anything is written a window asks "Let AI agents start recordings without asking?"
   and explains that the screen, camera and microphone follow your Recording settings, that the
   indicator and tray icon still show and that you can stop from the toast. Cancel leaves the row at
   Ask first. Confirm: the row says Allow with a "Changed" marker and an amber note ("AI agents can
   start recordings without asking ..."). Now run `capturemann record start --display 1 --countdown 0`.
   Pass: no confirmation window opens; the recording border, timer and Stop button, the tray
   indicator and the agent toast "Started by ..." with its Stop button all appear; Stop from the
   toast ends it. Under "Recent agent requests" the entry reads "allowed by your setting".
   Delete the take afterwards.
7. **Publish and delete cannot be loosened.** In the page: the Publish a video and Delete a
   capture rows have no Allow / Ask first control, only the lock. Over the socket, an agent cannot
   write the setting: `capturemann settings set agents.tool_policy '{"publish_video":"allow"}'`
   fails with `user_declined` / `data.reason` `app_only_setting` (exit 3).
8. **A hand-edited file cannot loosen them either.** Quit the app. Edit `settings.json` and add
   `"agents.tool_policy": {"publish_video": "allow", "delete_capture": "allow", "annotate": "ask"}`.
   Start the app. Pass: Publish a video and Delete a capture still show Ask first and locked; the
   Annotate captures row shows Ask first with "Changed"; an agent call to `publish_video` still opens
   the dialog. (Use a throwaway library; cancel the dialog.)
9. **A bad value is ignored, not fatal.** Put `"agents.tool_policy": "allow"` (a string) in the file
   and start the app. Pass: it starts, every tool is at its default, nothing is "Changed".
10. **Reset.** Tighten two tools, press Reset to defaults: no dialog, all rows back to their defaults,
    the button disappears. Tighten four tools (or three plus Start recording set to Allow) and press
    Reset to defaults: the app asks "Reset tool permissions?" with "4 tools are set differently ..."
    first; Cancel changes nothing, Reset to defaults clears them.
11. **A change applies to the next call.** Tighten Take screenshots, start `capturemann capture
    screenshot --display 1` (the dialog opens), and without answering reset the policy in the page.
    Pass: the open dialog stays; a second `capture screenshot` runs at once with no dialog; answering
    the first dialog still completes the first command.
12. **Per-client policy first.** Set a client to Read-only and run its `capture screenshot`:
    `user_declined` / `client_read_only` with no dialog, whatever the row says.
13. **Language.** Switch Settings → General → Language to Tiếng Việt: the page, the "Changed" marker,
    the lock tooltips, both Settings dialogs and the generic dialog are in Vietnamese and nothing is
    clipped (check at the narrowest window width, 720 px).
14. **Accessibility.** With VoiceOver on: each row reads "title, tool: tier" (plus "locked" and the
    reason, or "Changed from the default"); the control is a radio group named "Permission for
    <title>"; Tab reaches every control; the dialogs open with Cancel focused.

## Clean up

Delete the throwaway config and library directories and `/tmp/cm-policy.sock`. Record the run in
`verification/` (template in its README) before anyone writes "verified".
