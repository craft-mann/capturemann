# The Claude plugin

CaptureMann ships a plugin for Claude Code (and Cowork on the desktop) that connects Claude to the
app's MCP server and adds five skills that teach Claude how to use the tools well. It is the same
MCP server you can connect by hand (`capturemann mcp`, see the [user guide](user-guide.md#agents-and-the-cli)),
packaged so that one command installs it and updates follow the app's releases.

## What you get

- The `capturemann` MCP server, started by Claude Code as `capturemann mcp`. It appears in `/mcp` as
  `plugin:capturemann:capturemann`; the tools are the ones in
  [`specs/02-agent-interface.md`](../../specs/02-agent-interface.md).
- Skills: `/capturemann:capturemann-basics` (tiers, confirmations, errors),
  `/capturemann:screenshot-and-redact`, `/capturemann:record-screen`, `/capturemann:meeting-notes`,
  `/capturemann:export-and-publish`. Claude picks them by itself when a request matches; you can also
  run one by name.

The plugin has no hooks, no agents, no background processes and no executables. Its files are in
[`plugins/capturemann/`](../../plugins/capturemann/README.md).

## Requirements

1. **CaptureMann 0.3.x installed and open** on the same computer. Start the app before the session:
   when it is closed, the server starts a headless daemon and anything that needs your confirmation
   (recording, publishing, deleting) fails with `no_confirmer` until the app is open.
2. **The `capturemann` command.** On Linux the packages install it in `/usr/bin`. On Windows tick
   "Add capturemann.exe and cm-mcp.exe to my PATH" in the installer (or re-run it with `/ADDPATH`).
   On macOS, Homebrew links it; with the DMG it stays inside the app, so either link it once:

   ```sh
   sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann
   ```

   or give the plugin the full path: when Claude Code asks for **CaptureMann command** at install,
   type `/Applications/CaptureMann.app/Contents/MacOS/capturemann`. Change it later with
   `/plugin configure capturemann`.
3. Claude Code 2.1.281 or later (older versions print a warning about the directory fields in the
   plugin's manifest; the plugin was validated with 2.1.311).

## Install

```sh
claude plugin marketplace add craft-mann/capturemann
claude plugin install capturemann@capturemann-plugins
```

Inside a session the same two steps are `/plugin install capturemann --marketplace craft-mann/capturemann`.
Then run `/mcp`: `plugin:capturemann:capturemann` should be connected. Ask Claude "What is in my
CaptureMann library?" to make the first call (`list_library`, read-only).

Once the plugin is in Anthropic's directory you can also add it from **Customize → Plugins** on
claude.ai; it then reaches your Claude Code sessions as a synced plugin. The directory listing is
planned for 0.3.2 and is not live yet.

Update: `claude plugin update capturemann@capturemann-plugins`. The plugin's version follows the
app's version; install the matching app release first. Remove: `claude plugin uninstall capturemann@capturemann-plugins`.

## What Claude can and cannot do

- **At once (Free):** read the library and settings, take screenshots, annotate and redact, edit a
  recording (trim, cut, zoom, layout), transcribe, label speakers, write reports, export, copy to a
  destination you connected.
- **After you click Allow on screen (Confirm):** start a recording or the 60 fps self-test, publish a
  video, delete a capture, download or remove a model, move the library, import files with the
  originals removed. The dialog shows exactly what will run and times out after 5 minutes.
- **Never:** read your tokens or keys, change the privacy of a video that is already published, hide
  the recording indicator. No tool exists for these.

Settings → Agents & CLI lists the plugin as a client (the name Claude Code declares), shows the last
request, and lets you set it to Read-only or Blocked, set any tool that changes something to Ask
first, or let `start_recording` run without asking. The activity log there keeps every confirmed and
declined request for 30 days.

## Privacy

The plugin adds no network traffic and no telemetry. Captures, transcripts and reports stay in your
library folder; the AI models run on your computer. Nothing leaves the computer unless you connected
a Google Drive or YouTube account in the app and asked Claude to upload or publish, and publishing
never defaults to public. What Claude sends to Anthropic as part of a conversation (the text of a
tool result you asked for) is governed by your Claude plan. Policy: https://www.craftmann.app/capturemann/privacy/.

## Limits

- Local MCP servers do not run on claude.ai in the browser, on mobile, or in cloud Claude Code
  sessions. The skills load there, the tools do not.
- Cowork runs the server only in a session on your own computer and does not ask for the
  **CaptureMann command** value, so there `capturemann` must be on your PATH.
- One CaptureMann at a time: the plugin talks to the app that serves the default socket. A second
  instance on another socket (`CAPTUREMANN_SOCKET`) is not reachable from the plugin.

## If it does not work

| Symptom | Cause and fix |
| --- | --- |
| `/mcp` shows the server failed or no `capturemann` tools exist | `capturemann` is not on the PATH or the option points at a wrong path. Fix the **CaptureMann command** option (`/plugin configure capturemann`) and restart the session. |
| A tool answers `daemon_unavailable` | The app could not be reached or started. Open CaptureMann. |
| A recording, publish or delete fails with `user_declined` and `no_confirmer` | The app is closed (headless daemon). Open CaptureMann and ask again. |
| `user_declined` with `client_read_only` or `client_blocked` | You limited this client in Settings → Agents & CLI. |
| `permission_missing` | Grant Screen Recording, Camera or Microphone in the OS and in CaptureMann's Permissions page. |

Report problems at https://github.com/craft-mann/capturemann/issues. The manual check script for the
plugin is [`tests/manual/PLUGIN-1.md`](../../tests/manual/PLUGIN-1.md).
