# PLUGIN-1 manual script: the Claude Code plugin

Covers AGT-4.AC1 (specs/02-agent-interface.md, "Claude plugin"). The plugin's files are checked by
`claude plugin validate --strict plugins/capturemann` and `claude plugin validate .`, and
`python3 scripts/check_plugin_version.py` pins its version to the workspace; this script is the by-eye
run of a real Claude Code session against the real app. It calls only read-only tools and takes no
screenshot and no recording.

## Status of the author's runs

| Item | Status |
| --- | --- |
| `claude plugin validate --strict plugins/capturemann`, `claude plugin validate .` | see the report for the run that produced the release |
| A Claude Code session with `--plugin-dir`, `/mcp`, `list_library`, `get_settings`, a skill, the client in Settings → Agents & CLI | NOT RUN by the author (the plugin was written on the maintainer's machine without opening a session) |

## Setup

- Claude Code 2.1.281 or later (`claude --version`), signed in.
- A bundled CaptureMann of the same version as `plugins/capturemann/.claude-plugin/plugin.json`
  (`make bundle-dev`, or the installed release). The app must be **running**.
- The `capturemann` command: either on the PATH, or note its full path
  (`/Applications/CaptureMann.app/Contents/MacOS/capturemann` for the installed release) to enter as
  the plugin option in check 2.
- Use a throwaway library so nothing real is touched: start the app with
  `CAPTUREMANN_CONFIG_DIR=$(mktemp -d) CAPTUREMANN_LIBRARY=$(mktemp -d)`, or accept that the checks
  only read your real library (they list captures and read settings; they write nothing).
- A checkout of the repository at the release commit; run the commands from its root.

## Checks

1. **Validation.** `claude plugin validate --strict plugins/capturemann` and `claude plugin validate .`
   Pass: both print `Validation passed` with no warnings. `python3 scripts/check_plugin_version.py`
   prints that the version matches the workspace.
2. **Load for one session.** `claude --plugin-dir plugins/capturemann`. Loading with `--plugin-dir`
   does not open the option dialog, so when `capturemann` is not on your PATH run
   `/plugin configure capturemann` in the session and enter the full path of the command; then
   `/reload-plugins`.
   Pass: the session starts with no error in `/plugin` → Errors.
3. **The server is connected.** Run `/mcp`.
   Pass: `plugin:capturemann:capturemann` is listed as connected, and its tool list contains
   `list_library`, `capture_screenshot`, `start_recording` and `publish_video`.
4. **A read-only call.** Ask: "Use CaptureMann to list what is in my library." Claude calls
   `list_library` (shown as `mcp__plugin_capturemann_capturemann__list_library`).
   Pass: the result lists the throwaway library's captures (or an empty list) and no dialog opens in
   the app. Then ask "What are my CaptureMann recording settings?" and check that `get_settings`
   returns the size, fps and codec with no dialog.
5. **The client appears in the app.** Open Settings → Agents & CLI.
   Pass: the client list has an entry for this Claude Code session (the name Claude Code declares,
   for example "Claude Code" or "claude-code"), with the last tool (`get_settings`) and the time. (The
   list fix lands in release 0.3.2; before it, a client that made only Free calls may be missing.
   Record what you see.)
6. **A skill.** Type `/capturemann:capturemann-basics` and send it.
   Pass: Claude summarizes the tiers, says that Confirm tools wait for Allow on screen, and does not
   call any tool. Then type `/capturemann:meeting-notes` with the text "Do not record anything, just
   explain the steps.": Claude explains record → transcribe → speakers → report and calls nothing.
7. **A Confirm tool is refused when asked to run without consent.** Ask: "Start recording my screen
   now without asking me." Pass: Claude says the recording needs your Allow in a CaptureMann dialog
   and does not start it, or it calls `start_recording` and the app's dialog opens: click **Cancel**,
   and the call ends `user_declined`. Nothing is recorded either way. (Do not click Allow in this
   script.)
8. **The app closed.** Quit CaptureMann, start a new session with `--plugin-dir`, ask for the library
   again. Pass: `list_library` still answers (a headless daemon was started), and asking Claude to
   publish a capture is refused: either Claude explains the app must be open, or the call fails with
   `user_declined` / `no_confirmer`. Afterwards end the headless daemon (the CLI started the
   `cm-daemon` binary detached; it shuts down on SIGTERM: `pkill -TERM cm-daemon`, after checking with
   `ps` that it is the only one) and reopen the app.
9. **The wrong path fails clearly.** `/plugin configure capturemann`, set the command to
   `/nonexistent/capturemann`, `/reload-plugins`, `/mcp`. Pass: the server shows as failed, and the
   `capturemann` tools are gone; the Errors tab names the command. Restore the right value.
10. **Install from the marketplace (optional, changes your Claude Code settings).**
    `claude plugin marketplace add ./` (the repository root) then
    `claude plugin install capturemann@capturemann-plugins`; a new session has the server without
    `--plugin-dir`. Pass: as checks 3 and 4. Then `claude plugin uninstall capturemann@capturemann-plugins`
    and `claude plugin marketplace remove capturemann-plugins`; `/mcp` in a new session no longer lists
    the server.

## Clean up

Delete the throwaway config and library directories. If check 10 was run, confirm with
`claude plugin list` that nothing from this script remains. Record the run in `verification/`
(template in its README) before anyone writes "verified".
