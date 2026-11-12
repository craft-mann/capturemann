# CaptureMann plugin for Claude

This plugin connects Claude to [CaptureMann](https://www.craftmann.app/capturemann/), the free,
open-source (Apache-2.0) desktop app for screenshots and screen or audio recording with AI that
runs on your own computer. With it, Claude can take and redact screenshots, start a recording
after you approve it on screen, transcribe a meeting and write the minutes, export a video, and
publish it once you confirm.

## What the plugin adds

- **One MCP server**, `capturemann`, which runs the command `capturemann mcp`: the MCP server that
  ships inside the CaptureMann app. It talks to the app over a local socket. The plugin contains no
  server code of its own and downloads nothing.
- **Five skills** that teach Claude how to use the tools well: `capturemann-basics` (tiers,
  confirmations, errors, what to do when the app is not running), `screenshot-and-redact`,
  `record-screen`, `meeting-notes` and `export-and-publish`. Run one by name, for example
  `/capturemann:meeting-notes`, or let Claude pick it.

There are no hooks, agents, background processes or executables in this plugin.

## Requirements

- **CaptureMann 0.3.x installed and running** on the same computer: macOS 13 or later, Windows 11,
  or Ubuntu 24.04 or later and other Linux distributions with glibc 2.39. Download it from the
  [releases page](https://github.com/craft-mann/capturemann/releases) or see the
  [website](https://www.craftmann.app/capturemann/). The plugin does not install the app.
- **The `capturemann` command.** The Linux packages put it in `/usr/bin`; the Windows installer has an
  option to add it to your PATH; Homebrew links it. With the macOS DMG it stays inside the app bundle,
  so either link it once
  (`sudo ln -s /Applications/CaptureMann.app/Contents/MacOS/capturemann /usr/local/bin/capturemann`)
  or set the plugin option **CaptureMann command** to
  `/Applications/CaptureMann.app/Contents/MacOS/capturemann` when Claude Code asks, or later with
  `/plugin configure capturemann`.
- **Claude Code** (terminal, IDE extension or the desktop app's Code tab), or **Cowork** running on
  your computer. The server is a local program, so it does not run in claude.ai in the browser or on
  a phone; there the skills load but the tools are absent.

## Install

From the marketplace in the CaptureMann repository:

```sh
claude plugin marketplace add craft-mann/capturemann
claude plugin install capturemann@capturemann-plugins
```

Or, inside a Claude Code session, `/plugin install capturemann --marketplace craft-mann/capturemann`.
To try it for one session without installing: `claude --plugin-dir plugins/capturemann` from a
checkout of the repository. Then `/mcp` should list `plugin:capturemann:capturemann` as connected,
and a question such as "What is in my CaptureMann library?" calls `list_library`.

## How confirmations work

CaptureMann sorts its tools into tiers. Free tools (reading the library, taking a screenshot,
annotating, editing, transcribing, exporting) run at once and are one undo step each. Confirm tools
(`start_recording`, `publish_video`, `delete_capture`, the 60 fps self-test, model downloads and
removals, moving the library, importing with the originals removed) pause until **you click Allow in a
CaptureMann dialog on your screen**; the dialog shows exactly what will run, and the call fails if
you cancel or do not answer within 5 minutes. In the app, Settings → Agents & CLI lets you set any
tool that changes something to Ask first, let `start_recording` run without asking, and limit or
block each client by name. Publishing and deleting always ask. A recording started by Claude shows
the same border, timer and Stop button as one you start yourself.

## Privacy

- **No telemetry.** Neither the plugin nor CaptureMann collects usage data.
- **Local by default.** Screenshots, recordings, transcripts, speaker labels and reports are made and
  kept on your computer, in your library folder. The speech, speaker and text models run on your
  machine. The only network use of the AI features is a model download you ask for, which is a
  Confirm tool.
- **Nothing leaves the computer unless you connected a destination** (a Google Drive or a YouTube
  account) in the app. The `upload` tool copies to a destination you set up; `publish_video` asks you
  first and never defaults to public. Claude cannot read your tokens or change the privacy of a video
  that is already published; no tool exists for that.
- What Claude itself sends to Anthropic is governed by your Claude plan, not by this plugin: the text
  of a tool result (a transcript, a screenshot image you asked for) becomes part of the conversation.
- Privacy policy: https://www.craftmann.app/capturemann/privacy/

## Limits

- Needs the CaptureMann app installed; the plugin cannot install it or start it for you. When the app
  is closed, every tool answers `daemon_unavailable` until you open the app (the app provides the
  daemon). Confirm tools, such as recording or publishing, wait for your Allow in the app.
- Does not run on claude.ai in the browser, on mobile, or in a cloud Claude Code session (local MCP
  servers do not run there).
- Windows and Linux: window recording, the cursor track and global shortcuts have platform limits
  listed in the [user guide](https://github.com/craft-mann/capturemann/blob/main/docs/user/user-guide.md).

## Support

Issues and questions: https://github.com/craft-mann/capturemann/issues. The tool reference is
`specs/02-agent-interface.md` in the repository. Licence: Apache-2.0 (see `LICENSE` in this folder).
