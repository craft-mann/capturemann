---
name: capturemann-basics
description: "How CaptureMann's MCP tools work: Free and Confirm tiers, request_id, client policy, errors, and what to do when the app is not running or not installed. Read before the first CaptureMann tool call."
---

# CaptureMann basics

CaptureMann is a desktop app for screenshots and screen or audio recording with local AI. This plugin connects to the MCP server inside the app on this computer (`capturemann mcp`). Nothing runs in the cloud; the app sends nothing anywhere unless the user connected a destination (Google Drive, YouTube) in the app.

## Tools

Tools appear as `mcp__plugin_capturemann_capturemann__<tool>`. Each tool has the same name as a CLI subcommand (`capture_screenshot` is `capturemann capture screenshot`). Use the tool list and each tool's schema; do not guess parameters. Long work (`transcribe`, `export_video`, `upload`, `publish_video`, `diarize`, `generate_report`, `import_media`) returns a `job_id` at once: follow it with `get_job` or `list_jobs`, cancel with `cancel_job`.

## Tiers

- **Free** tools run at once: reads (`list_library`, `get_capture`, `get_settings`, `list_targets`, `list_devices`, `get_job`, ...), `capture_screenshot`, `annotate`, the edit tools, `transcribe`, `export_video`, `upload`, `import_media`.
- **Confirm** tools pause until the user clicks Allow in a CaptureMann dialog on their screen, for up to 5 minutes: `start_recording`, `publish_video`, `delete_capture`, `run_fps_selftest`, `move_library`, `download_model`, `remove_model`, `download_text_model`, `remove_text_model`, and `import_media` with `remove_originals: true`. Tell the user a dialog is waiting before you call one. The dialog shows exactly what will run; you cannot answer it.
- The user can set any tool that changes something to **Ask first** in Settings → Agents & CLI; a call then opens a generic dialog. `start_recording` alone can be set to Allow.
- Reading tokens or keys, changing the privacy of a published video, hiding the recording indicator: no tool exists. Do not try to work around that.

## Idempotency and undo

Every mutating tool takes an optional `request_id`. Give one for uploads, publishes and anything you might retry: a repeated id returns the first result instead of acting twice. One call is one undo step (`undo` / `redo` on the capture).

## Settings

`get_settings` reads; `set_settings` writes recording settings (size, fps, codec, camera, audio). App-only keys (`library.*`, `privacy.*`, `shortcuts.*`, `permissions.*`, `agents.*`, `storage.*`, `export.*`, `mcp.*`, `auth.*`, `app.*`) are refused with `user_declined` / `data.reason: "app_only_setting"`; do not retry them.

## Client policy

Settings → Agents & CLI lists this client by name. Read-only refuses every tool that changes something (`user_declined`, `data.reason: "client_read_only"`); Blocked refuses all calls (`client_blocked`). Tell the user to check that page; never try another client name.

## Errors

Results and errors carry a shared code: `invalid_input`, `not_found`, `user_declined`, `confirmation_timeout`, `permission_missing` (`data.permission`: `screen`, `camera`, `microphone`; the user grants it in the OS and in CaptureMann's Permissions page), `busy`, `job_failed`, `not_implemented`, `disk_full`, `daemon_unavailable`. CLI exit codes: 0 ok, 2 bad input or not found, 3 declined or timed out, 4 permission missing, 5 daemon unreachable, 6 job failed or busy. Report the message to the user; do not loop on `user_declined`.

## When the tools are missing or fail

- **No `capturemann` tools in this session:** the MCP server did not start. CaptureMann is not installed, or the `capturemann` program is not on the PATH. Ask the user to install CaptureMann (https://www.craftmann.app/capturemann/) and, on macOS, to set the plugin option "CaptureMann command" to `/Applications/CaptureMann.app/Contents/MacOS/capturemann` (`/plugin configure capturemann`), then restart the session. Do not install anything yourself.
- **The app is not open:** the MCP server does not start a daemon of its own, so every tool answers `daemon_unavailable`. Ask the user to open the CaptureMann app and retry.
- **`daemon_unavailable`:** the daemon could not be reached; ask the user to open the app. (If a daemon runs without the app, Free tools work and a Confirm tool fails with `user_declined` and `data.reason: "no_confirmer"`.)
