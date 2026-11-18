# CaptureMann MCP reference

CaptureMann ships a Model Context Protocol (MCP) server, `cm-mcp`, that gives an AI agent the same actions as the app and the CLI. This page lists all 90 tools (the 89 of version 0.3.4 and `list_presets`, added for 0.3.6), in the order the server lists them within each area. It was written from the running server (`tools/list`) and the source of the 0.3.4 build, not from memory; the machine-readable version is [tools.json](tools.json). How both were made and how to regenerate them: [README.md](README.md).

Wording: nothing here is "verified" in the sense of the repository's `verification/` reports. The examples marked as run were run against a headless daemon with an empty library in a throw-away folder; the rest are calls shown for their shape only.

## What the server is

- A thin bridge. Every tool call becomes one action on the CaptureMann daemon over its local socket (JSON-RPC, one request per line). The tool name is the daemon method and the CLI subcommand has the same behaviour (`capture_screenshot` is `capturemann capture screenshot`). The server holds no engine code.
- The server is named `capturemann` (version 0.3.4) and offers tools, resources, prompts and logging (events as notifications).
- Local only. No network traffic and no telemetry of its own; the tools act on this computer's library. Nothing is uploaded or published unless a destination was connected in the app and the user approves it.

## Starting the server

| Mode | Command | Notes |
| --- | --- | --- |
| stdio (default) | `cm-mcp` or `capturemann mcp` | MCP over stdin and stdout. Logs go to stderr only. |
| Local HTTP | `cm-mcp --http [--port N]` or `capturemann mcp --http [--port N]` | Streamable HTTP on `127.0.0.1` only, path `/mcp`, random free port unless `--port`. Every request needs `Authorization: Bearer <token>`; a missing or wrong token is HTTP 401. The token is `CAPTUREMANN_MCP_TOKEN` when set, else a fresh random one. The address and the token are printed once to stderr. At most 64 simultaneous connections. |
| Other socket | add `--socket <path>` (or set `CAPTUREMANN_SOCKET`) | Where the daemon listens. Default: the platform endpoint (macOS `~/Library/Application Support/CaptureMann/daemon.sock`, Linux `$XDG_RUNTIME_DIR/capturemann/daemon.sock`, Windows the pipe `\\.\pipe\capturemann`). |

`capturemann mcp` runs the `cm-mcp` binary found in `CAPTUREMANN_MCP`, then next to the `capturemann` executable, then on `PATH`, and forwards `--http`, `--port` and a non-default `--socket`. Running the HTTP mode on a start-up run looks like this (the token here was set through `CAPTUREMANN_MCP_TOKEN` for the demonstration):

```text
$ CAPTUREMANN_MCP_TOKEN=demo-token cm-mcp --http
cm-mcp listening on http://127.0.0.1:52221/mcp
bearer token: demo-token
```

A request without the token is answered `401 missing or invalid bearer token`; the same `initialize` request with `Authorization: Bearer demo-token` returns the server info (run).

## Adding it to an MCP client

Settings > Agents & CLI in the app shows a "Client config" card with a snippet to copy (the launcher's "Copy MCP server config" gives the same text). The shape is the `mcpServers` JSON that Claude Desktop, Cursor and similar clients read; `command` is the real path of the `capturemann` program found next to the app (or `capturemann` when it was not found) and `--socket` is the socket the app is serving:

```json
{
  "mcpServers": {
    "capturemann": {
      "command": "/Applications/CaptureMann.app/Contents/MacOS/capturemann",
      "args": ["mcp", "--socket", "/Users/you/Library/Application Support/CaptureMann/daemon.sock"]
    }
  }
}
```

The path and socket above are the macOS defaults, shown as an illustration: copy the snippet from your own app, because it carries your real paths. When the app serves no socket the snippet has no `--socket` and reads `"args": ["mcp"]`. The snippet never contains the HTTP bearer token. Where each client keeps its config file is that client's documentation and was not checked here.

- **Claude Desktop, Cursor and other clients that read `mcpServers`:** paste the snippet into the client's MCP config and restart the client.
- **Claude Code and Cowork:** the repository carries a plugin (`plugins/capturemann`, marketplace at the repository root) that registers the same server as `{"command": "<your capturemann path>", "args": ["mcp"]}` and adds six skills. Install: `claude plugin marketplace add craft-mann/capturemann`, then `claude plugin install capturemann@capturemann-plugins` (from `plugins/capturemann/README.md`). The server is a local program, so it does not run in claude.ai in the browser.
- **A client that speaks HTTP:** start `capturemann mcp --http`, then point the client at the printed `http://127.0.0.1:<port>/mcp` URL with the bearer token.

The `capturemann` command is inside the app bundle on macOS (`/Applications/CaptureMann.app/Contents/MacOS/capturemann`) and is not on `PATH` unless linked; see [cli.md](cli.md#where-the-command-lives).

## The app or daemon must be running

The MCP server never starts a daemon. It opens the daemon connection when the client has initialised (waiting up to 5 seconds) and again on the first tool call. If nothing listens, a tool answers an error result with code `daemon_unavailable` and the message `cannot connect to the CaptureMann daemon: ...`. Start the CaptureMann app (which serves the socket), or run `cm-daemon` yourself, and call again; the server reconnects by itself after a daemon restart. (Run: with no daemon, `list_library` returned `isError: true` and `structuredContent.code` `daemon_unavailable`.)

The CLI differs: `capturemann` starts a headless `cm-daemon` when none is running (see [cli.md](cli.md)). The macOS app bundle does not contain `cm-daemon`, so on macOS the app must be running for both.

## Permission tiers and per-tool policy

Each tool is in one tier. The server writes the tier into every tool description.

| Tier | Behaviour | Tools |
| --- | --- | --- |
| **Free** | Runs at once. A tool that changes something is one undo step and is logged as a revision by `agent:<client name>`. | 79 tools |
| **Confirm** | Waits for the user to approve in an on-screen dialog that shows exactly what will run. The call waits up to 5 minutes, then fails with `user_declined` (the user cancelled) or `confirmation_timeout`. | `delete_capture`, `move_library`, `start_recording`, `run_fps_selftest`, `download_model`, `remove_model`, `download_text_model`, `remove_text_model`, `start_guide`, `publish_video` |
| **Never** | Not exposed: reading or exporting tokens and API keys, changing the privacy of an existing remote post, turning off the recording indicator. No tool exists; a call to one of those reserved names is `method not found`. | none |

**Parameter-dependent tiers.** `import_media` is Free, but a call with `remove_originals: true` (and not `dry_run: true`) is Confirm. `capture_screenshot` is Free, but `mode: "long"` with `scroll: "auto"` (Long Shot, Preview) is Confirm because the screen moves, and `scroll: "hand"` is refused for agents with `invalid_input`.

**What the user can change (Settings > Agents & CLI, per tool, decision OD-24).** The list shows each tool as Allow or Ask first. In `tools.json` this is the `policy` field:

| `policy` | Meaning | Tools |
| --- | --- | --- |
| `locked_allow` | Read-only tools: always allowed, locked ("Reading never needs asking"). | 29 |
| `default_allow` | Free tools that change something: Allow by default; the user can set Ask first, which makes an agent's call wait for a generic on-screen confirmation (up to 8 key and value lines of the parameters, secrets never shown). | 50 |
| `default_ask` | `start_recording` only: Ask first by default; the user can set it to Allow (the recording border, timer and Stop button and the "Started by <agent>" toast still appear). | 1 |
| `locked_ask` | Always asks and cannot be changed: `delete_capture`, `move_library`, `run_fps_selftest`, `download_model`, `remove_model`, `download_text_model`, `remove_text_model`, `start_guide`, `publish_video`. | 9 |

The setting is stored in the app-only key `agents.tool_policy`, a JSON object `{"<tool>": "allow" | "ask"}` that lists only the tools whose value differs from the default. An agent cannot change it (`set_settings` refuses `agents.*` and the other app-only keys with `user_declined`, `data.reason` `app_only_setting`), and a hand-edited file can tighten a tool but never loosen anything except `start_recording`. A call that is asked first and cancelled fails with `user_declined` and `data.reason` `ask_first`.

**Per-client policy.** Settings > Agents & CLI also lists every client by the name it declares (`clientInfo.name`, sent in `session.hello`, at most 64 bytes) and lets the user set it to Allowed (default), Read-only (refuses every call that changes something and every Confirm tool) or Blocked (refuses everything). A refusal is `user_declined` with `data.reason` `client_read_only` or `client_blocked`. The name is self-declared, so this limits well-behaved agents; it is not a security boundary against a hostile local process. The confirmation gate, the app-only settings list and the Never tier hold regardless of names.

**The headless case.** A daemon with no app window attached has nobody to ask. A Confirm call then fails at once with `user_declined`, `data.reason` `no_confirmer` (run: `delete_capture` against a headless daemon).

## Idempotency

Every tool that changes something accepts an optional `request_id` (a string of 1 to 256 characters). Repeating a call with the same `request_id` within 24 hours returns the first result instead of acting twice; this matters most for `upload` and `publish_video`. The CLI sets it with the global `--request-id`. Read-only tools take none.

## Results and errors

A successful call returns `structuredContent` (the daemon's result object) and the same JSON as a text block. An image (`image.data`, base64) is moved out of the structured result into an MCP image content block; the structured result keeps `image.mime_type` and `image.content_index` (always 1).

A failed call is a tool result with `isError: true`, a text block `<code>: <message>` and `structuredContent` = `{ "code", "message", "data" }`. Read `code`; it is one of the shared codes (they also give the CLI exit codes):

| `code` | CLI exit | JSON-RPC code | Meaning |
| --- | --- | --- | --- |
| `invalid_input` | 2 | -32602 | Bad or missing parameter; the message names the field. May carry `data.reason`. |
| `not_found` | 2 | -32001 | Capture, target, device, job or account id does not exist. |
| `user_declined` | 3 | -32002 | The user refused a confirmation, or a policy refused the call (`data.reason`: `ask_first`, `client_read_only`, `client_blocked`, `app_only_setting`, `no_confirmer`, `cancelled`). |
| `confirmation_timeout` | 3 | -32003 | No answer within 5 minutes. |
| `permission_missing` | 4 | -32004 | An OS permission is absent; `data.permission` is `screen`, `camera`, `microphone`, `accessibility`, `input_monitoring` or `files`. |
| `device_busy` | 5 | -32005 | Camera or microphone in exclusive use by another app. |
| `disk_full` | 5 | -32006 | Below the free-space floor. |
| `network` | 5 | -32007 | Transient network failure (retryable). |
| `platform_rejected` | 5 | -32008 | The platform API refused; `data.platform_code`. |
| `auth_expired` | 5 | -32009 | Token revoked or expired; reconnect the account in the app. |
| `job_failed` | 6 | -32010 | A background job failed; `data.job_id`, `data.reason`. |
| `busy` | 6 | -32011 | A conflicting operation is running, for example a second recording. |
| `internal` | 1 | -32012 | Unexpected internal, I/O or database failure. |
| `daemon_unavailable` | 5 | -32013 | The server cannot reach the daemon (produced by the MCP server and the CLI, not by the daemon). |
| `not_implemented` | 1 | -32099 | The action is known but this build or platform has no engine for it. |

The JSON-RPC level has its own errors, outside the shared table. In MCP they appear as JSON-RPC `error` objects rather than tool results:

| JSON-RPC code | When |
| --- | --- |
| -32700 | Parse error (the daemon socket: the line is not JSON). |
| -32600 | Invalid request (not a JSON-RPC 2.0 object, line too long). |
| -32601 | Method not found: an unknown method, or a Never-tier name, on the daemon socket. |
| -32602 | Invalid params. From the MCP server: `unknown tool` (run: `{"code":-32602,"message":"unknown tool `no_such_tool`"}`), an unknown prompt or a missing prompt argument, or a malformed resource URI. |
| -32603 | Internal error (a worker panicked, a lock was poisoned). |
| -32001 to -32013 and -32099 | The shared codes above, when a resource read fails: `resource_not_found` for `not_found`, otherwise an invalid-request error that keeps `{ code, message, data }` as its data. |

Every tool can also answer: `invalid_input` (schema or value problems, checked by the daemon), `daemon_unavailable`, `user_declined` from a client policy, and `internal`. The per-tool lines below list the codes named in each tool's description plus the tier and id-derived ones; they are not an exhaustive guarantee.

Run examples (headless daemon, empty library plus two imported pictures):

```text
get_settings {"keys":["recording.fps"]}          -> {"settings": {"recording.fps": 30}}
get_capture {}                                    -> isError, {"code": "invalid_input", "message": "capture_id: is required", "data": null}
get_capture {"capture_id":"missing_id"}           -> isError, {"code": "not_found", "message": "no capture `missing_id`", "data": null}
delete_capture {"capture_id":"shot_..."}          -> isError, {"code": "user_declined", "message": "No confirmation UI is attached to this daemon", "data": {"reason": "no_confirmer"}}
set_settings {"values":{"privacy.x":1}}          -> isError, {"code": "user_declined", "message": "`privacy.x` can only be changed in the CaptureMann app (Settings)", "data": {"reason": "app_only_setting"}}
list_library {"kind":"movie"}                    -> isError, {"code": "invalid_input", "message": "kind: must be `screenshot`, `recording`, `audio` or `guide`", "data": null}
```

## Events

MCP has no event channel, so the server subscribes to the daemon's `events.subscribe` when a session starts and forwards each event as a `notifications/message` with `logger` `capturemann.events` and `data` = `{ seq, event, at, data }`. The server declares the `logging` capability and honours `logging/setLevel` (default `info`, which forwards everything; `error` for `job.failed`, `warning` for `recording.warning` and `events.dropped`). The per-client policy applies: a blocked client gets none. A client that reads slowly loses events and gets one `events.dropped { dropped }`.

Topics forwarded: `capture.*`, `job.*`, `export.*`, `confirmation.*`, `recording.starting`, `recording.started`, `recording.paused`, `recording.resumed`, `recording.stopped`, `recording.discarded`, `recording.warning`, `recording.recovered`, `recording.selftest`, `camera.*`, `model.changed`, `transcript.started`, `transcript.ready`, `transcript.partial`, `translation.ready`, `speakers.started`, `speakers.ready`, `report.ready`, `report.failed`. Left out on purpose because they would flood a model's context: `recording.stats` (1 Hz), `recording.health` (5 s) and `recording.countdown` (per second); read the same numbers with `get_recording_status`. Long work returns a `job_id` at once; follow it with `get_job` or the `job.*` events.

## Resources and prompts

Read-only resources (`resources/list`, `resources/templates/list`, `resources/read`; every read is one Free daemon action, at most 1 MB, `resources/subscribe` is not offered):

| URI | Content |
| --- | --- |
| `capturemann://library` | The 50 newest captures: id, title, kind, duration, created. Search and paging: the list_library tool. |
| `capturemann://jobs` | Active and recent background jobs (export, transcribe, upload, publish) with state and progress. |
| `capturemann://platforms/youtube/rules` | Current limits and required fields for publishing to YouTube. |
| `capturemann://capture/{id}` | Metadata, tracks, uploads, annotation ops and revisions of one capture (no image: use get_capture for that). |
| `capturemann://capture/{id}/transcript` | Word-level transcript with times and the caption lines of a recording; not found until it is transcribed. |
| `capturemann://capture/{id}/reports` | The AI-9 reports of a recording: the list (`reports`) and the newest one in full (`latest`, JSON and Markdown). |
| `capturemann://platforms/{name}/rules` | Current limits and required fields of a publishing platform (youtube). |

Prompts (`prompts/list`, `prompts/get`): text templates that name the real tools: `record-tutorial`, `publish-everywhere`, `bug-report-screenshot`, `lesson-to-drive`, `caption-cleanup`, `make-short-clip`. They guide the steps; nothing is recorded or published without the on-screen confirmation.

## Tool index

The server lists 90 tools. Tier: F = Free, C = Confirm. Policy as in the table above (`locked_allow`, `default_allow`, `default_ask`, `locked_ask`).

| Tool | Area | Tier | Policy | Read-only | CLI | Preview |
| --- | --- | --- | --- | --- | --- | --- |
| [`list_targets`](#list_targets) | Discovery and diagnostics | F | `locked_allow` | yes | `capturemann targets` |  |
| [`list_devices`](#list_devices) | Discovery and diagnostics | F | `locked_allow` | yes | `capturemann devices` |  |
| [`get_diagnostics`](#get_diagnostics) | Discovery and diagnostics | F | `locked_allow` | yes | `capturemann diagnostics` |  |
| [`get_settings`](#get_settings) | Settings | F | `locked_allow` | yes | `capturemann settings get` |  |
| [`set_settings`](#set_settings) | Settings | F | `default_allow` | no | `capturemann settings set` |  |
| [`capture_screenshot`](#capture_screenshot) | Screenshots | F | `default_allow` | no | `capturemann capture screenshot` | yes |
| [`get_capture`](#get_capture) | Screenshots | F | `locked_allow` | yes | `capturemann capture get` |  |
| [`recognize_text`](#recognize_text) | Screenshots | F | `locked_allow` | yes | `capturemann text` |  |
| [`annotate`](#annotate) | Annotate and undo | F | `default_allow` | no | `capturemann annotate` |  |
| [`suggest_redactions`](#suggest_redactions) | Annotate and undo | F | `locked_allow` | yes | `capturemann annotate --suggest-redactions` |  |
| [`suggest_blueprint`](#suggest_blueprint) | Annotate and undo | F | `locked_allow` | yes | `capturemann annotate --suggest-blueprint` |  |
| [`undo`](#undo) | Annotate and undo | F | `default_allow` | no | `capturemann undo` |  |
| [`redo`](#redo) | Annotate and undo | F | `default_allow` | no | `capturemann redo` |  |
| [`create_board`](#create_board) | Boards and bulk export | F | `default_allow` | no | `capturemann board` |  |
| [`export_images`](#export_images) | Boards and bulk export | F | `default_allow` | no | `capturemann export-images` |  |
| [`list_library`](#list_library) | Library and import | F | `locked_allow` | yes | `capturemann library list` |  |
| [`rename_capture`](#rename_capture) | Library and import | F | `default_allow` | no | `capturemann library rename` |  |
| [`delete_capture`](#delete_capture) | Library and import | C | `locked_ask` | no | `capturemann library rm` |  |
| [`move_library`](#move_library) | Library and import | C | `locked_ask` | no | `capturemann library move` |  |
| [`import_media`](#import_media) | Library and import | F | `default_allow` | no | `capturemann import` |  |
| [`start_recording`](#start_recording) | Recording | C | `default_ask` | no | `capturemann record start` |  |
| [`pause_recording`](#pause_recording) | Recording | F | `default_allow` | no | `capturemann record pause` |  |
| [`resume_recording`](#resume_recording) | Recording | F | `default_allow` | no | `capturemann record resume` |  |
| [`set_mic_muted`](#set_mic_muted) | Recording | F | `default_allow` | no | `capturemann record mic` |  |
| [`add_marker`](#add_marker) | Recording | F | `default_allow` | no | `capturemann record marker` |  |
| [`stop_recording`](#stop_recording) | Recording | F | `default_allow` | no | `capturemann record stop` |  |
| [`get_recording_status`](#get_recording_status) | Recording | F | `locked_allow` | yes | `capturemann record status` |  |
| [`run_fps_selftest`](#run_fps_selftest) | Recording | C | `locked_ask` | no | `capturemann record selftest` |  |
| [`get_cursor_track`](#get_cursor_track) | Recording | F | `locked_allow` | yes | `capturemann cursor` |  |
| [`trim`](#trim) | Editing a recording | F | `default_allow` | no | `capturemann edit trim` |  |
| [`cut_segments`](#cut_segments) | Editing a recording | F | `default_allow` | no | `capturemann edit cut` |  |
| [`restore_segments`](#restore_segments) | Editing a recording | F | `default_allow` | no | `capturemann edit restore` |  |
| [`split`](#split) | Editing a recording | F | `default_allow` | no | `capturemann edit split` |  |
| [`set_speed`](#set_speed) | Editing a recording | F | `default_allow` | no | `capturemann edit speed` |  |
| [`set_background`](#set_background) | Editing a recording | F | `default_allow` | no | `capturemann edit background` |  |
| [`set_audio_mix`](#set_audio_mix) | Editing a recording | F | `default_allow` | no | `capturemann edit audio` |  |
| [`set_camera_layout`](#set_camera_layout) | Editing a recording | F | `default_allow` | no | `capturemann edit camera` |  |
| [`set_zooms`](#set_zooms) | Editing a recording | F | `default_allow` | no | `capturemann edit zoom` |  |
| [`set_cursor_style`](#set_cursor_style) | Editing a recording | F | `default_allow` | no | `capturemann edit cursor` |  |
| [`set_reframe`](#set_reframe) | Editing a recording | F | `default_allow` | no | `capturemann edit reframe` |  |
| [`export_video`](#export_video) | Export | F | `default_allow` | no | `capturemann export` |  |
| [`export_audio`](#export_audio) | Export | F | `default_allow` | no | `capturemann export --audio` |  |
| [`estimate_export`](#estimate_export) | Export | F | `locked_allow` | yes | `capturemann export --estimate` |  |
| [`get_job`](#get_job) | Jobs | F | `locked_allow` | yes | `capturemann jobs get` |  |
| [`list_jobs`](#list_jobs) | Jobs | F | `locked_allow` | yes | `capturemann jobs list` |  |
| [`cancel_job`](#cancel_job) | Jobs | F | `default_allow` | no | `capturemann jobs cancel` |  |
| [`retry_job`](#retry_job) | Jobs | F | `default_allow` | no | `capturemann jobs retry` |  |
| [`list_models`](#list_models) | Speech and text models | F | `locked_allow` | yes | `capturemann ai models` |  |
| [`download_model`](#download_model) | Speech and text models | C | `locked_ask` | no | `capturemann ai models download` |  |
| [`import_model`](#import_model) | Speech and text models | F | `default_allow` | no | `capturemann ai models import` |  |
| [`select_model`](#select_model) | Speech and text models | F | `default_allow` | no | `capturemann ai models use` |  |
| [`remove_model`](#remove_model) | Speech and text models | C | `locked_ask` | no | `capturemann ai models rm` |  |
| [`list_text_models`](#list_text_models) | Speech and text models | F | `locked_allow` | yes | `capturemann ai text-models` |  |
| [`download_text_model`](#download_text_model) | Speech and text models | C | `locked_ask` | no | `capturemann ai text-models download` |  |
| [`select_text_model`](#select_text_model) | Speech and text models | F | `default_allow` | no | `capturemann ai text-models use` |  |
| [`remove_text_model`](#remove_text_model) | Speech and text models | C | `locked_ask` | no | `capturemann ai text-models rm` |  |
| [`get_text_model`](#get_text_model) | Speech and text models | F | `locked_allow` | yes | `capturemann ai text-model` |  |
| [`test_text_model`](#test_text_model) | Speech and text models | F | `locked_allow` | yes | `capturemann ai text-model test` |  |
| [`transcribe`](#transcribe) | Transcript and captions | F | `default_allow` | no | `capturemann ai transcribe` |  |
| [`get_transcript`](#get_transcript) | Transcript and captions | F | `locked_allow` | yes | `capturemann ai transcript` |  |
| [`get_captions`](#get_captions) | Transcript and captions | F | `locked_allow` | yes | `capturemann ai captions get` |  |
| [`set_captions`](#set_captions) | Transcript and captions | F | `default_allow` | no | `capturemann ai captions set` |  |
| [`replace_in_captions`](#replace_in_captions) | Transcript and captions | F | `default_allow` | no | `capturemann ai captions replace` |  |
| [`set_caption_options`](#set_caption_options) | Transcript and captions | F | `default_allow` | no | `capturemann ai captions options` |  |
| [`export_captions`](#export_captions) | Transcript and captions | F | `default_allow` | no | `capturemann export --captions <srt|vtt|txt|md>` |  |
| [`translate_captions`](#translate_captions) | Transcript and captions | F | `default_allow` | no | `capturemann ai translate` |  |
| [`normalize_transcript`](#normalize_transcript) | Transcript and captions | F | `default_allow` | no | `capturemann ai normalize` |  |
| [`get_normalized_transcript`](#get_normalized_transcript) | Transcript and captions | F | `locked_allow` | yes | `capturemann ai normalized` |  |
| [`diarize`](#diarize) | Speakers | F | `default_allow` | no | `capturemann ai speakers` |  |
| [`set_speakers`](#set_speakers) | Speakers | F | `default_allow` | no | `capturemann ai speakers rename` |  |
| [`suggest_zooms`](#suggest_zooms) | Suggestions | F | `locked_allow` | yes | `capturemann ai suggest zooms` |  |
| [`suggest_cuts`](#suggest_cuts) | Suggestions | F | `locked_allow` | yes | `capturemann ai suggest cuts` |  |
| [`list_report_templates`](#list_report_templates) | Reports | F | `locked_allow` | yes | `capturemann ai report templates` |  |
| [`generate_report`](#generate_report) | Reports | F | `default_allow` | no | `capturemann ai report` |  |
| [`get_report`](#get_report) | Reports | F | `locked_allow` | yes | `capturemann ai report get` |  |
| [`export_report`](#export_report) | Reports | F | `default_allow` | no | `capturemann ai report export` |  |
| [`delete_report`](#delete_report) | Reports | F | `default_allow` | no | `capturemann ai report delete` |  |
| [`edit_report`](#edit_report) | Reports | F | `default_allow` | no | `capturemann ai report edit` |  |
| [`check_report_item`](#check_report_item) | Reports | F | `default_allow` | no | `capturemann ai report check` |  |
| [`start_guide`](#start_guide) | Click Guide (Preview) | C | `locked_ask` | no | `capturemann guide start` | yes |
| [`stop_guide`](#stop_guide) | Click Guide (Preview) | F | `default_allow` | no | `capturemann guide stop` | yes |
| [`get_guide`](#get_guide) | Click Guide (Preview) | F | `locked_allow` | yes | `capturemann guide get` | yes |
| [`set_guide`](#set_guide) | Click Guide (Preview) | F | `default_allow` | no | `capturemann guide set` | yes |
| [`export_guide`](#export_guide) | Click Guide (Preview) | F | `default_allow` | no | `capturemann guide export` | yes |
| [`list_presets`](#list_presets) | Quick Capture presets (Preview) | F | `locked_allow` | yes | `capturemann presets list` | yes |
| [`list_destinations`](#list_destinations) | Destinations and publishing | F | `locked_allow` | yes | `capturemann destinations` |  |
| [`upload`](#upload) | Destinations and publishing | F | `default_allow` | no | `capturemann upload` |  |
| [`list_publish_accounts`](#list_publish_accounts) | Destinations and publishing | F | `locked_allow` | yes | `capturemann publish accounts` |  |
| [`get_platform_rules`](#get_platform_rules) | Destinations and publishing | F | `locked_allow` | yes | `capturemann platform-rules` |  |
| [`publish_video`](#publish_video) | Destinations and publishing | C | `locked_ask` | no | `capturemann publish` |  |

## Tools by area

Notes for the entries: ids in the examples are illustrative; the descriptions are shortened (the full text and the complete schemas, including output schemas, are in [tools.json](tools.json)). "Preview" marks Click Guide, Long Shot and Quick Capture, which are Preview features: they may still change, but they are always available. A recording id looks like `rec_...`, a screenshot id like `shot_...`.

### Discovery and diagnostics

#### `list_targets`

List the displays and windows that can be captured (ids, titles, apps, bounds in physical px, scale).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann targets`
- **Spec:** CAP-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `windows` | boolean | no | true | Include the window list. false returns displays only and an empty `windows` array, without enumerating windows. |

Returns: `displays`, `windows`, `capabilities`.

Errors:  (plus the common errors above).

```json
{"name": "list_targets", "arguments": {"windows": false}}
```

#### `list_devices`

List the cameras, microphones and system-audio support, with the permission state of the camera, microphone and screen recording.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann devices`
- **Spec:** CAM-4, AUD-1, AUD-2

No input.

Returns: `cameras`, `cameras_error`, `camera_permission`, `mics`, `microphone_permission`, `system_audio`, `screen_recording_permission`, `cursor`.

Errors:  (plus the common errors above).

```json
{"name": "list_devices", "arguments": {}}
```

#### `get_diagnostics`

Read the diagnostics report of this CaptureMann: the text a user pastes into an issue.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann diagnostics`
- **Spec:** DIAG-1

No input.

Returns: `text`, `sections`.

Errors:  (plus the common errors above).

```json
{"name": "get_diagnostics", "arguments": {}}
```

### Settings

#### `get_settings`

Read effective settings (dotted keys such as `recording.fps`).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann settings get`
- **Spec:** settings

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `keys` | array of string | no |  | Keys to read; omit for all. |

Returns: `settings`.

Errors:  (plus the common errors above).

```json
{"name": "get_settings", "arguments": {"keys": ["recording.fps"]}}
```

#### `set_settings`

Change settings by dotted key; returns the effective settings.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann settings set`
- **Spec:** settings

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `values` | object | yes |  | Map of dotted key to new value. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `settings`.

Errors: `user_declined` (plus the common errors above).

```json
{"name": "set_settings", "arguments": {"values": {"recording.fps": 30}}}
```

### Screenshots

#### `capture_screenshot`

Take a screenshot of a display, a window, or a region of a display and store it in the library.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first. **Confirm when** mode is "long" and scroll is "auto" (the default): the screen moves; scroll "hand" is refused for agents.
- **Read-only:** no
- **CLI:** `capturemann capture screenshot`
- **Preview parameter:** `mode` (Long Shot) is a Preview feature inside this tool.
- **Spec:** CAP-1, CAP-4, CAP-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `target` | object {display_id} or object {window_id} or object {display_id, rect} or object {pick} | yes |  |  |
| `delay_s` | number | no |  | Seconds to wait first. (min 0, max 30) |
| `mode` | "long" | no |  | (preview) Long Shot (CAP-6): a scrolling capture of the target as one tall image. The target is a window, a display or a region. |
| `direction` | vertical \| horizontal | no | "vertical" | Long Shot only. |
| `max_px` | integer | no |  | Long Shot only: stop at this length. (min 1, max 16384) |
| `scroll` | auto \| hand | no | "auto" | Long Shot only. |
| `copy_to_clipboard` | boolean | no | false |  |
| `title` | string | no |  | Library title; defaults to `Screenshot <date> <time>`. |
| `preset` | string | no |  | (preview) Quick Capture preset: the name or id of a screenshot preset from `list_presets`. It fills `delay_s`, `copy_to_clipboard` and `mode: "long"` (for a Long Shot preset) when the call leaves them out; an argument in the same call wins. A preset holds no area, so `target` is still required. An unknown name fails with `invalid_input` naming it. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `path`, `kind`, `width`, `height`, `copied_to_clipboard`, `warnings`, `image`, `source`, `long_shot`.

Errors: `user_declined`, `confirmation_timeout`, `not_implemented`, `permission_missing` (plus the common errors above).

```json
{"name": "capture_screenshot", "arguments": {"target": {"display_id": 1}, "copy_to_clipboard": false}}
```

#### `get_capture`

Read a capture: metadata, tracks, uploads, current annotation ops, revisions, and the current image (annotations rendered).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann capture get`
- **Spec:** ANN-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `revision` | integer | no |  | Only the current revision is supported. (min 0) |
| `include_image` | boolean | no | true |  |

Returns: `capture_id`, `title`, `kind`, `created`, `path`, `tracks`, `uploads`, `ops`, `revision`, `revisions`, `can_undo`, `can_redo`, `warnings`, `image`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_capture", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH", "include_image": false}}
```

#### `recognize_text`

Read the text in a screenshot with on-device text recognition (nothing is sent anywhere, nothing is stored) and return it as plain text in reading order: a blank line between paragraphs and between columns.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann text`
- **Spec:** ANN-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `region` | array of number | no |  | [x, y, w, h] in pixels of the original image. Default: the whole image. (min items 4, max items 4) |
| `copy_to_clipboard` | boolean | no | false |  |
| `format` | text \| lines | no | "text" | `lines` adds the `lines` array. |

Returns: `capture_id`, `recognizer`, `region`, `text`, `word_count`, `truncated`, `copied_to_clipboard`, `warnings`, `lines`.

Errors: `not_implemented`, `not_found` (plus the common errors above).

```json
{"name": "recognize_text", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH", "format": "lines"}}
```

### Annotate and undo

#### `annotate`

Add annotation ops (arrow, line, rect, ellipse (these four take `style`: solid, dashed or dotted), text, highlight, step, callout, spotlight, stamp, magnify, retype, move_area, blueprint, blur, pixelate, crop) to a screenshot.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann annotate`
- **Spec:** ANN-1, ANN-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `ops` | array of object {from, to, color, width, op, id} or object {from, to, color, width, op, id} or object {rect, color, width, fill, radius, op, id} or object {rect, color, width, op, id} or object {at, text, size, color, outline, bg, op, ...} or object {rect, color, opacity, op, id} or object {at, n, size, color, style, start, text, ...} or object {rect, tail, text, size, color, bg, radius, ...} or object {shapes, dim, mode, strength, op, id} or object {at, kind, size, color, text, op, id} or object {rect, strength, op, id} or object {rect, block, op, id} or object {rect, op, id} or object {source, at, zoom, shape, border, border_width, connector, ...} or object {rect, fill, text, size, color, weight, align, ...} or object {source, to, fill, op, id} or object {boxes, style, color, radius, colors, fills, op, ...} | yes |  | The ops to add. Empty is allowed only with `replace: true`, which clears every annotation (revert to the original). |
| `replace` | boolean | no | false | Clear the existing ops first (still one undo step). |
| `copy_to_clipboard` | boolean | no | false | Copy the rendered image (annotations and redactions applied, never the original) to the clipboard once the revision is saved. |
| `return_image` | boolean | no | true | Return the rendered image. Set false when you do not need it (the result then has `image: null`). |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `by`, `copied_to_clipboard`, `warnings`, `auto_text`, `image`.

Errors: `invalid_input`, `busy`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "annotate", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH", "ops": [{"op": "rect", "rect": [40, 40, 200, 120], "color": "#E5484D", "width": 4}, {"op": "text", "at": [60, 200], "text": "Step 1", "size": 28}], "return_image": false}}
```

#### `suggest_redactions`

Find emails, API keys and other secrets, payment cards, SSNs and other US identifiers, and (with lower confidence) phone numbers and IP addresses in a screenshot with on-device text recognition and propose blur rectangles.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann annotate --suggest-redactions`
- **Spec:** ANN-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `kinds` | array of email \| token \| private_key \| secret_assignment \| url_secret \| card \| card_extra \| ssn \| itin \| ein \| aba \| iban \| npi \| dea \| mbi \| passport \| vin \| imei \| ip_address \| mac_address \| phone | no |  | Only look for these kinds. Default: every kind except `npi`, which is off by default because its false-alarm rate is high (name it to turn it on). |

Returns: `capture_id`, `recognizer`, `image_size`, `rects`, `applied`.

Errors: `not_implemented`, `not_found` (plus the common errors above).

```json
{"name": "suggest_redactions", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH", "kinds": ["email", "token"]}}
```

#### `suggest_blueprint`

Find the text lines of a screenshot with on-device text recognition and return them as boxes for a `blueprint` op (nothing is stored, nothing leaves the computer).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann annotate --suggest-blueprint`
- **Spec:** ANN-11

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id of a screenshot. (min length 1) |

Returns: `capture_id`, `recognizer`, `image_size`, `boxes`.

Errors: `not_implemented`, `not_found` (plus the common errors above).

```json
{"name": "suggest_blueprint", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH"}}
```

#### `undo`

Undo the last revision of a capture (one step reverts a whole earlier call).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann undo`
- **Spec:** ANN-3, AGT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `changed`, `can_undo`, `can_redo`, `image`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "undo", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH"}}
```

#### `redo`

Redo the last undone revision of a capture.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann redo`
- **Spec:** ANN-3, AGT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `changed`, `can_undo`, `can_redo`, `image`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "redo", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH"}}
```

### Boards and bulk export

#### `create_board`

Put 2 to 12 screenshots into one new picture (a board).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann board`
- **Spec:** BRD-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_ids` | array of string | yes |  | The screenshots, in order. (min items 2, max items 12) |
| `layout` | pair \| grid \| strip \| free or object {kind, arrangement, columns, slots, canvas} | yes |  | A layout name (`pair`, `grid`, `strip`, `free`) or an object. |
| `options` | object {gap, margin, background, radius, shadow, captions, title, ...} | no |  |  |
| `title` | string | no |  | The new capture's title (default: the board title, else `Board`). |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `title`, `path`, `kind`, `layout`, `width`, `height`, `sources`, `warnings`, `image`.

Errors: `invalid_input`, `not_found`, `user_declined` (plus the common errors above).

```json
{"name": "create_board", "arguments": {"capture_ids": ["shot_06GYEX0HFHRZHYWH", "shot_06GYEX0JGWS1Y0BN"], "layout": "pair", "options": {"before_after": true}}}
```

#### `export_images`

Resize, convert, watermark and rename many screenshots at once (Bulk export).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann export-images`
- **Spec:** BLK-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_ids` | array of string | yes |  |  (min items 1, max items 500) |
| `dest` | string | yes |  | Absolute path of the folder to write to. |
| `size` | object {width, height, longest, percent, allow_larger} | no |  | Exactly one of width, height, longest or percent. |
| `format` | png \| jpeg \| webp | no | "png" |  |
| `quality` | integer | no | 90 | JPEG. (min 60, max 100) |
| `watermark` | object {text, image_path, position, opacity, size} | no |  | Exactly one of text or image_path. |
| `blueprint` | boolean | no | false | Replace every text line of each picture with a bar in the exported files (the captures are not changed). |
| `name_pattern` | string | no | "{name}" |  (max length 200) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `kind`, `state`, `dest`, `count`.

Errors: `not_found`, `not_implemented`, `user_declined` (plus the common errors above).

```json
{"name": "export_images", "arguments": {"capture_ids": ["shot_06GYEX0HFHRZHYWH"], "dest": "/Users/me/Pictures/site", "size": {"longest": 1600}, "format": "jpeg", "quality": 85}}
```

### Library and import

#### `list_library`

List or search captures in the library, newest first, with paging.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann library list`
- **Spec:** LIB-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `query` | string | no |  | Full-text search. |
| `kind` | screenshot \| recording \| audio \| guide | no |  | `audio` lists audio-only recordings (VOC-1); `recording` lists screen recordings; `guide` lists Click Guides (GDE-1). |
| `status` | uploaded \| not_uploaded \| published \| not_published | no |  | Only captures with (or without) an upload record (`uploaded`: sent to a storage destination) or a publish record (`published`: on YouTube and the like). Combines with `kind` and `query`; paging stays correct. |
| `limit` | integer | no | 50 |  (min 1, max 200) |
| `cursor` | string | no |  | `next_cursor` of the previous page. |

Returns: `captures`, `next_cursor`.

Errors:  (plus the common errors above).

```json
{"name": "list_library", "arguments": {"kind": "recording", "limit": 20}}
```

#### `rename_capture`

Rename a capture: sets its title and renames its library folder to match.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann library rename`
- **Spec:** LIB-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `title` | string | yes |  | The new title. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `title`, `path`, `kind`.

Errors: `busy`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "rename_capture", "arguments": {"capture_id": "shot_06GYEX0HFHRZHYWH", "title": "Settings page, before"}}
```

#### `delete_capture`

Delete a capture: its folder is moved to the Trash (the user can restore it) and it leaves the library.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no (destructive: removes data)
- **CLI:** `capturemann library rm`
- **Spec:** LIB-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `also_delete_remote` | boolean | no | false | Also permanently delete the remote copies recorded in the capture (Google Drive, local destinations, YouTube), listed in the confirmation dialog. A failed remote step keeps the capture. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `title`, `kind`, `size_bytes`, `moved_to_trash`, `deleted_remote`, `remote`.

Errors: `network`, `platform_rejected`, `auth_expired`, `busy`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "delete_capture", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "also_delete_remote": false}}
```

#### `move_library`

Change the folder the library lives in (for example to an external drive).

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann library move`
- **Spec:** LIB-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `path` | string | yes |  | Absolute path of the new library folder (created if it does not exist). (min length 1) |
| `mode` | move \| fresh \| use_existing | no | "move" | `move` copies the captures, `fresh` starts empty, `use_existing` switches to a library already in the folder. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `mode`, `from`, `to`, `bytes`, `bundles`, `restart_required`, `warnings`.

Errors: `invalid_input`, `not_found`, `disk_full`, `busy`, `user_declined`, `confirmation_timeout` (plus the common errors above).

```json
{"name": "move_library", "arguments": {"path": "/Volumes/Big/CaptureMann", "mode": "move"}}
```

#### `import_media`

Bring video, audio and picture files that CaptureMann did not record into the library as captures (a phone clip, a voice memo, a downloaded interview, a phone screenshot), so that the editor, transcript, speakers, reports, export,...

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first. **Confirm when** remove_originals is true and dry_run is not true.
- **Read-only:** no
- **CLI:** `capturemann import`
- **Spec:** IMP-1 to IMP-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `paths` | array of string | yes |  | Absolute paths of the files to import (files only, no folders). (min items 1, max items 200) |
| `titles` | array of string | no |  | One title per path, in order; an empty string keeps the default title: the file's own title tag (ID3 for an MP3), else the file name without its extension. |
| `remove_originals` | boolean | no | false | Move each original to the Trash after its copy is verified. Raises the call to Confirm tier: the user is asked on screen. |
| `dry_run` | boolean | no | false | Only check the files and return their status; import nothing. |
| `after` | object {transcribe, diarize, report_template} | no |  |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `captures`, `needed_bytes`, `files`.

Errors: `invalid_input`, `user_declined`, `confirmation_timeout`, `permission_missing`, `disk_full` (plus the common errors above).

```json
{"name": "import_media", "arguments": {"paths": ["/Users/me/Downloads/interview.m4a"], "after": {"transcribe": true}}}
```

### Recording

#### `start_recording`

Start a screen recording of a display, a window or a region (physical px relative to the display, like capture_screenshot), optionally with the webcam, the microphone and system audio.

- **Tier:** Confirm by default: asks on screen; the user can set it to Allow in Settings > Agents & CLI (the only tool that can be loosened).
- **Read-only:** no
- **CLI:** `capturemann record start`
- **Spec:** REC-1-4, CAM-1, AUD-1, AUD-2, AGT-2, VOC-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `mode` | screen \| audio | no | "screen" | `audio`: microphone and/or system audio only (see above). |
| `preset` | string | no |  | Either a Quick Capture preset (a Preview feature): the name or id of a video or audio preset from `list_presets`. It fills `mic`, `system_audio`, `camera`, `cursor`, `fps`, `countdown_s` (and `mode: "audio"` for an audio preset) when the call leaves them out; an argument in the same call wins; `target` is still required for a screen recording; the confirmation shows the plan the preset resolves to; an unknown name fails with `invalid_input` naming it. Or, with `mode: "audio"`, the use case: `room`, `online`, `lecture`, `product` or `note`. For the use case: `room` (interview, 1:1, meeting around a laptop), `lecture` and `note` record the microphone; `online` (call) and `product` (demo call) add system audio. Every source can still be set. |
| `people` | integer | no |  | Only with `mode: "audio"`: how many people speak, when known. Stored with the recording; the automatic speaker step (setting `ai.auto_speakers`, on by default) uses it as the speaker count. (min 1, max 8) |
| `target` | object {display_id} or object {window_id} or object {display_id, rect} | no |  | Required for a screen recording; refused with `mode: "audio"`. |
| `size` | source \| 2160p \| 1440p \| 1080p \| 720p | no |  | Output size; default `source` (native, no cap) or the `recording.size` setting. |
| `fps` | 30 \| 60 | no |  | Default 30 or the `recording.fps` setting. 60 is accepted only after a passing `run_fps_selftest` for this display, size and codec (valid 30 days)... |
| `codec` | auto \| hevc \| h264 | no |  | `auto`: HEVC above 1920 px wide, else H.264. |
| `quality` | high \| balanced \| small | no |  |  |
| `camera` | boolean or string or object {on, device_id, resolution, mirror, shape, size, position} | no |  | Record the webcam: false (default), true (the camera set in Settings > Webcam & audio (`camera.device`) when it is connected, else the system default camera), a device id, or an object. |
| `mic` | boolean or string or object {on, device_id, denoise} | no |  | Record the microphone: false (default), true or `default` (the microphone set in Settings > Webcam & audio (`audio.mic_device`) when it is connected, else the system default input), a device id from list_devices, or an object. |
| `system_audio` | boolean | no | false |  |
| `cursor` | boolean | no | true | Record cursor telemetry (positions and clicks, never keystrokes). The pointer stays in the video unless `cursor_separate` is true. |
| `cursor_separate` | boolean | no | false | Record the cursor separately (EDT-4): the pointer is left out of the screen video and drawn from the telemetry in the editor and the export, so it can be made bigger, smoother and given click effects. Needs `cursor` on... |
| `max_duration_s` | number | no |  | Stop automatically after this much recorded time (pauses do not count). (max 86400, above 0) |
| `countdown_s` | integer | no |  | Countdown before recording; default the `recording.countdown_s` setting (3). stop_recording during it cancels the start. (min 0, max 10) |
| `countdown` | integer | no |  | Alias of `countdown_s`. (min 0, max 10) |
| `title` | string | no |  | Library title; defaults to `Recording <date> <time>`. |
| `exclude` | object {bundle_ids, window_ids} | no |  |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`target`) or (`mode`).

Returns: `recording_id`, `capture_id`, `title`, `state`, `live_at_ms`, `started_at`, `by`, `countdown_s`, `mode`, `tracks`, `warnings`, `indicator`, `max_duration_s`, `restart_of`.

Errors: `invalid_input`, `permission_missing`, `busy`, `user_declined`, `confirmation_timeout` (plus the common errors above).

```json
{"name": "start_recording", "arguments": {"mode": "screen", "target": {"display_id": 1}, "fps": 30, "mic": true, "countdown_s": 3}}
```

#### `pause_recording`

Pause the running recording: every track stops writing and the timer stops; resume continues in the same files with no gap.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann record pause`
- **Spec:** REC-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `recording_id` | string | no |  | From start_recording; optional while only one recording exists. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `recording_id`, `state`, `changed`, `recorded_ms`, `cancelled`.

Errors: `user_declined` (plus the common errors above).

```json
{"name": "pause_recording", "arguments": {}}
```

#### `resume_recording`

Resume a paused recording after the resume countdown (default 1 s, the `recording.resume_countdown_s` setting).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann record resume`
- **Spec:** REC-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `recording_id` | string | no |  | From start_recording; optional while only one recording exists. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `recording_id`, `state`, `changed`, `paused_ms`.

Errors: `user_declined` (plus the common errors above).

```json
{"name": "resume_recording", "arguments": {}}
```

#### `set_mic_muted`

Mute or unmute the microphone of the running recording (also while paused).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann record mic`
- **Spec:** AUD-6

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `muted` | boolean | yes |  | true mutes, false unmutes. |
| `recording_id` | string | no |  | From start_recording; optional while only one recording exists. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `recording_id`, `muted`, `changed`, `at_ms`.

Errors: `user_declined` (plus the common errors above).

```json
{"name": "set_mic_muted", "arguments": {"muted": true}}
```

#### `add_marker`

Add a marker (a moment worth coming back to, with an optional label) to the running recording at the current recorded time, or to a finished recording at `at`.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann record marker`
- **Spec:** VOC-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `recording_id` | string | no |  | From start_recording; optional while only one recording exists. Not with `capture_id`. (min length 1) |
| `capture_id` | string | no |  | A finished recording; needs `at`. (min length 1) |
| `at` | number | no |  | Seconds of recorded time (pauses removed). With `capture_id`: where to add the marker to a finished recording. Without it, and with `label`: the running recording's marker set at that time gets the label (nothing is added). (min 0) |
| `label` | string | no |  | Optional short label. (max length 200) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `recording_id`, `capture_id`, `marker`, `count`, `revision`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "add_marker", "arguments": {"label": "Decision"}}
```

#### `stop_recording`

Stop and save the recording (also valid while paused).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann record stop`
- **Spec:** REC-1, REC-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `recording_id` | string | no |  | From start_recording; optional while only one recording exists. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `recording_id`, `capture_id`, `title`, `path`, `reason`, `duration_s`, `duration_ms`, `paused_ms`, `tracks`, `warnings`, `already_stopped`, `cancelled`.

Errors: `user_declined` (plus the common errors above).

```json
{"name": "stop_recording", "arguments": {}}
```

#### `get_recording_status`

The state of the recording: idle, countdown, recording, paused, resuming or stopping, with elapsed recorded time, per-track state and counters, late-frame percentage, microphone level (dBFS), minutes of disk left and warnings.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann record status`
- **Spec:** REC-3, REC-8, AUD-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `recording_id` | string | no |  |  (min length 1) |

Returns: `state`, `recording_id`, `capture_id`, `countdown_remaining`, `recorded_ms`, `elapsed_ms`, `paused_ms`, `pauses`, `tracks`, `late_frame_pct`, `mic_level_dbfs`, `disk_left_minutes`, `warnings`, `indicator`, ....

Errors:  (plus the common errors above).

```json
{"name": "get_recording_status", "arguments": {}}
```

#### `run_fps_selftest`

Test whether this machine records 60 fps without late frames.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann record selftest`
- **Spec:** REC-3, AGT-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `target` | object {display_id} or object {window_id} or object {display_id, rect} | no |  | What to test; default the primary display. |
| `size` | source \| 2160p \| 1440p \| 1080p \| 720p | no |  | Output size to test; default the `recording.size` setting, else `source`. |
| `codec` | auto \| hevc \| h264 | no |  | Codec to test; default the `recording.codec` setting, else `auto`. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `selftest_id`, `state`, `passed`, `fps`, `late_pct`, `dropped`, `dropped_pct`, `frames`, `duration_s`, `size`, `size_label`, `codec`, `display_id`, `at`, ....

Errors: `permission_missing`, `busy`, `user_declined`, `confirmation_timeout` (plus the common errors above).

```json
{"name": "run_fps_selftest", "arguments": {"target": {"display_id": 1}, "size": "1440p"}}
```

#### `get_cursor_track`

Read the cursor track of a screen recording: pointer moves, button down and up (with modifier flags, never a key or a character), scroll direction and cursor-image changes.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann cursor`
- **Spec:** REC-7

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `from` | number or string | no |  | Start of the range, inclusive: seconds or "t:<ticks>" (session time). Default: the start. |
| `to` | number or string | no |  | End of the range, exclusive. Default: the end. |
| `limit` | integer | no | 5000 |  (min 1, max 50000) |

Returns: `capture_id`, `tick_rate`, `coordinates`, `target`, `clicks`, `scroll`, `move_source`, `permission_missing`, `from`, `to`, `count`, `total_in_range`, `events`, `next_from`, ....

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_cursor_track", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "from": 0, "to": 30, "limit": 500}}
```

### Editing a recording

#### `trim`

Keep only start..end of a recording (session time).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit trim`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `start` | number or string | yes |  | Start of the kept part, in session time: seconds (a number) or "t:<ticks>" (120,000 ticks per second). |
| `end` | number or string | yes |  | End of the kept part, in session time: seconds (a number) or "t:<ticks>" (120,000 ticks per second). |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "trim", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "start": 2.5, "end": 118}}
```

#### `cut_segments`

Cut ranges out of a recording; they are skipped in preview and export.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit cut`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `ranges` | array of array of number or string | yes |  |  (min items 1) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `invalid_input`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "cut_segments", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "ranges": [[10, 15], [40, 42]]}}
```

#### `restore_segments`

Bring cut ranges back (a range may restore only part of a cut).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit restore`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `ranges` | array of array of number or string | yes |  |  (min items 1) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "restore_segments", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "ranges": [[10, 15]]}}
```

#### `split`

Add a segment boundary at a time; speed and layout settings apply per segment.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit split`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `at` | number or string | yes |  | Where to split, in session time: seconds (a number) or "t:<ticks>" (120,000 ticks per second). |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "split", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "at": 60}}
```

#### `set_speed`

Play a range at 0.25x to 8x.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit speed`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `range` | array of number or string | yes |  | The range: [start, end], end after start. (min items 2, max items 2) |
| `factor` | number | yes |  |  (min 0.25, max 8) |
| `keep_audio` | boolean | no | false |  |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_speed", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "range": [30, 50], "factor": 2}}
```

#### `set_background`

Frame the screen on a background: none, a color, a two-stop gradient or an image, with padding, corner radius and shadow.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit background`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `kind` | none \| color \| gradient \| image | yes |  |  |
| `value` | any | no |  | `#rrggbb` for color, ["#rrggbb", "#rrggbb"] for gradient, a file path for image. |
| `padding_pct` | number | no |  | Percent of the frame width. (min 0, max 20) |
| `radius_px` | integer | no |  |  (min 0, max 24) |
| `shadow` | boolean | no |  |  |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_background", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "kind": "color", "value": "#1E1E2E", "padding_pct": 6, "radius_px": 12, "shadow": true}}
```

#### `set_audio_mix`

Balance the separate mic and system-audio tracks: gain in dB (-96 to +12), mute, and the denoised or raw mic file.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit audio`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `mic_gain_db` | number | no |  |  (min -96, max 12) |
| `system_gain_db` | number | no |  |  (min -96, max 12) |
| `mic_mute` | boolean | no |  |  |
| `system_mute` | boolean | no |  |  |
| `denoise` | boolean | no |  | true: the denoised mic file; false: the raw one. |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`mic_gain_db`) or (`system_gain_db`) or (`mic_mute`) or (`system_mute`) or (`denoise`).

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_audio_mix", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "mic_gain_db": 3, "denoise": true}}
```

#### `set_camera_layout`

Set how the screen and the camera share the frame over a range: `bubble` (screen with the camera bubble; the default), `screen` (camera hidden), `camera` (camera full frame) or `side_by_side` (screen 2/3, camera 1/3).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit camera`
- **Spec:** EDT-1, EDT-2, EDT-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `range` | array of number or string | yes |  | The range: [start, end], end after start. (min items 2, max items 2) |
| `layout` | bubble \| screen \| camera \| side_by_side | yes |  |  |
| `shape` | circle \| rounded | no |  | `bubble` only. |
| `rect` | array of number | no |  | `bubble` only: [x, y, w, h] as fractions of the frame, w from 0.05 to 1. (min items 4, max items 4) |
| `ring` | any | no |  | `bubble` only: a fraction of the bubble width (0 to 0.5), true for the default ring, false for none. |
| `mirror` | boolean | no |  |  |
| `transition` | cut \| smooth | no | "cut" |  |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_camera_layout", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "range": [0, 15], "layout": "bubble", "shape": "circle"}}
```

#### `set_zooms`

Add zoom regions to a recording: each starts at `at`, lasts `length`, keeps `focus` (fractions of the frame) in the middle at `scale` 1.2 to 3, easing in and out over 400 ms; `follow_cursor` keeps the cursor in view.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit zoom`
- **Spec:** EDT-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `zooms` | array of object {at, length, focus, scale, follow_cursor} | yes |  |  |
| `replace` | boolean | no | false |  |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_zooms", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "zooms": [{"at": 12, "length": 3, "focus": [0.4, 0.5], "scale": 1.8}]}}
```

#### `set_cursor_style`

Style the telemetry cursor of a recording made with "Record cursor separately": size 1x to 3x, smoothing, click effect and hide-when-idle (2 s).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit cursor`
- **Spec:** EDT-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `size` | number | no |  |  (min 1, max 3) |
| `smoothing` | off \| light \| strong | no |  |  |
| `click_effect` | none \| ring \| ripple | no |  |  |
| `hide_idle` | boolean | no |  |  |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`size`) or (`smoothing`) or (`click_effect`) or (`hide_idle`).

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_cursor_style", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "size": 1.5, "click_effect": "ring"}}
```

#### `set_reframe`

Reframe a 16:9 recording for a vertical (9:16) or square (1:1) export: `fit` letterboxes the whole screen, `fixed` crops a window centered on `focus` (per segment: `at` picks the segment), `follow` keeps the window on the cursor.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann edit reframe`
- **Spec:** EDT-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `aspect` | 9:16 \| 1:1 | yes |  |  |
| `mode` | fit \| fixed \| follow \| follow_cursor | no |  | `follow_cursor` is the same as `follow`. |
| `focus` | array of number | no |  | The window centre: [x, y] as fractions of the frame, y down. (min items 2, max items 2) |
| `at` | number or string | no |  | A time inside the segment the focus is for (default: the first segment), in session time: seconds (a number) or "t:<ticks>" (120,000 ticks per second). |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`mode`) or (`focus`).

Returns: `capture_id`, `revision`, `action`, `by`, `changed`, `duration`, `duration_s`, `edits`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_reframe", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "aspect": "9:16", "mode": "follow"}}
```

### Export

#### `export_video`

Render a recording to one combined MP4 (screen, round camera bubble with the rec ring, microphone and system audio) in the capture's `exports/` folder, in the background.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann export`
- **Spec:** EXP-1, EXP-4, REC-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `preset` | string | no | "quick" | `quick`, `youtube-4k`, `youtube-1440p`, `youtube-1080p`, `vertical-1080`, `square-1080`, `source`, `gif` or `webm` (the preset table can be extended with a rules file). Unknown ids fail with `invalid_input`. |
| `format` | mp4 \| gif \| webm | no |  | `gif` or `webm` is the same as that preset. Default: MP4. |
| `width` | integer | no |  | GIF (480, 640, 800 or 1024; default 800) and WebM (even width; default 1280) only. (min 16, max 7680) |
| `fps` | number | no |  | GIF (10, 15 or 24; default 15) and WebM (default 30) only. (min 1, max 60) |
| `max_mb` | number | no |  | GIF and WebM only: "fit under N MB" (10^6 bytes). A GIF steps the width and then the frame rate down, a WebM lowers the bitrate; fails if it cannot fit. (max 1000, above 0) |
| `size` | source \| 1440p \| 1080p \| 720p \| 480p or 1440 \| 1080 \| 720 \| 480 | no |  | MP4 and WebM: the frame. `source` (the default), or a height step: 1440, 1080, 720 or 480 (also "720p"). Never above the recording; a step equal to the recording is the recording's size; the shape is kept. |
| `loudness_lufs` | number | no |  | Normalise the integrated loudness to this level in LUFS (AUD-4; -14 is the usual platform target) with a -1 dBTP true-peak limiter. Omit to keep the recorded level. (min -40, max -5) |
| `captions` | burn \| none | no |  | `burn` burns the captions into the picture (white text on a dark box inside the platform's safe area; the user's edited lines, else the transcript, with the edits applied) and fails with `not_found` before `transcribe` has run... |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `capture_id`, `preset`, `loudness_lufs`, `captions`, `size`, `estimate`, `state`, `reused`.

Errors: `invalid_input`, `not_found`, `not_implemented`, `user_declined` (plus the common errors above).

```json
{"name": "export_video", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "preset": "youtube-1080p"}}
```

#### `export_audio`

Export the audio of a capture (an audio recording, or "Export audio only" of a screen recording) as M4A (AAC-LC, 48 kHz, 128 kbps mono or 192 kbps stereo; the default) or WAV (48 kHz, 24-bit), in the background.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann export --audio`
- **Spec:** VOC-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `format` | m4a \| wav | no | "m4a" |  |
| `tracks` | mix \| mic \| system | no | "mix" | `mix` sums the microphone and the system audio, `mic` and `system` export one of them. |
| `loudness_lufs` | number or null | no |  | Integrated loudness target in LUFS. Default: -16 for M4A, none for WAV. `null` turns it off. (min -40, max -5) |
| `out_dir` | string | no |  | Absolute path of an existing folder to also copy the file to. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `capture_id`, `kind`, `format`, `tracks`, `loudness_lufs`, `out_dir`, `state`, `reused`.

Errors: `invalid_input`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "export_audio", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "format": "m4a", "tracks": "mix"}}
```

#### `estimate_export`

Estimate the size of an export before it runs: a best guess and a hard upper bound, with no decoding and no file written.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann export --estimate`
- **Spec:** EXP-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `format` | mp4 \| webm \| gif | no | "mp4" |  |
| `size` | source \| 1440p \| 1080p \| 720p \| 480p or 1440 \| 1080 \| 720 \| 480 | no |  | MP4 and WebM: `source` (the default) or a height step 1440, 1080, 720 or 480 (also "720p"). |
| `width` | 480 \| 640 \| 800 \| 1024 | no |  | GIF only (default 800); the WebM width of older scripts is accepted too. |
| `max_mb` | number | no |  | GIF and WebM: "fit under N MB"; accepted so a request can be checked as it will be sent. (max 1000, above 0) |

Returns: `max_bytes`, `label`, `typical_bytes`, `typical_label`, `codec`, `width`, `height`, `fps`, `duration_s`, `known_after_export`, `format`, `options`.

Errors: `invalid_input`, `not_found` (plus the common errors above).

```json
{"name": "estimate_export", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "format": "webm", "size": "720p"}}
```

### Jobs

#### `get_job`

Read one background job (export, upload, transcribe, publish): state, progress 0..1, elapsed, ETA, result or error.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann jobs get`
- **Spec:** EXP-4, STO-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `job_id` | string | yes |  | Job id, e.g. from export_video or upload. (min length 1) |

Returns: `job_id`, `type`, `state`, `progress`, `message`, `attempts`, `depends_on`, `inputs`, `result`, `error`, `pause_reason`, `created_at`, `started_at`, `finished_at`, ....

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_job", "arguments": {"job_id": "job_f7b27a8e508ae4a1"}}
```

#### `list_jobs`

List background jobs, newest first, optionally filtered by state and type.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann jobs list`
- **Spec:** EXP-4, STO-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `state` | queued \| running \| paused \| failed \| done \| cancelled | no |  |  |
| `type` | string | no |  | Job type, e.g. `upload`. |
| `limit` | integer | no | 50 |  (min 1, max 200) |

Returns: `jobs`.

Errors:  (plus the common errors above).

```json
{"name": "list_jobs", "arguments": {"state": "running"}}
```

#### `cancel_job`

Cancel a job.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann jobs cancel`
- **Spec:** EXP-4, STO-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `job_id` | string | yes |  | Job id, e.g. from export_video or upload. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `cancelling`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "cancel_job", "arguments": {"job_id": "job_f7b27a8e508ae4a1"}}
```

#### `retry_job`

Try a job again.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann jobs retry`
- **Spec:** STO-3, EXP-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `job_id` | string | yes |  | Job id, e.g. from export_video or upload. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`.

Errors: `invalid_input`, `auth_expired`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "retry_job", "arguments": {"job_id": "job_f7b27a8e508ae4a1"}}
```

### Speech and text models

#### `list_models`

List the speech models: which are installed on this computer, which one transcribe uses by default (`active`), their download size, and the folder they are kept in.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai models`
- **Spec:** AI-1

No input.

Returns: `models`, `active`, `dir`.

Errors:  (plus the common errors above).

```json
{"name": "list_models", "arguments": {}}
```

#### `download_model`

Download a speech model from the internet, in the background.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann ai models download`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A speech model id from list_models, e.g. `base`, `small` or `medium`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `reused`, `model`.

Errors: `invalid_input`, `network`, `not_implemented`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "download_model", "arguments": {"model": "small"}}
```

#### `import_model`

Install a speech model from a file the user already has (for example a downloaded `ggml-small.bin`), without using the network.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai models import`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A speech model id from list_models, e.g. `base`, `small` or `medium`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `path` | string | yes |  | Path of the model file on this computer. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `model`, `installed`.

Errors: `network`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "import_model", "arguments": {"model": "small", "path": "/Users/me/Downloads/ggml-small.bin"}}
```

#### `select_model`

Choose the installed speech model that `transcribe` uses when no model is named.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai models use`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A speech model id from list_models, e.g. `base`, `small` or `medium`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `active`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "select_model", "arguments": {"model": "small"}}
```

#### `remove_model`

Delete an installed speech model from this computer.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no (destructive: removes data)
- **CLI:** `capturemann ai models rm`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A speech model id from list_models, e.g. `base`, `small` or `medium`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `model`, `removed`.

Errors: `busy`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "remove_model", "arguments": {"model": "small"}}
```

#### `list_text_models`

List the text models of the engine built into CaptureMann (provider `embedded`).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai text-models`
- **Spec:** AI-7

No input.

Returns: `models`, `active`, `dir`.

Errors:  (plus the common errors above).

```json
{"name": "list_text_models", "arguments": {}}
```

#### `download_text_model`

Download a text model for the built-in engine from the internet, in the background.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann ai text-models download`
- **Spec:** AI-7

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A built-in text model id from list_text_models, e.g. `gemma-4-e2b`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `reused`, `model`.

Errors: `invalid_input`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "download_text_model", "arguments": {"model": "gemma-4-e2b"}}
```

#### `select_text_model`

Choose the downloaded text model the built-in engine runs, the same as the Use button on the AI page.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai text-models use`
- **Spec:** AI-7

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A built-in text model id from list_text_models, e.g. `gemma-4-e2b`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `active`, `provider`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "select_text_model", "arguments": {"model": "gemma-4-e2b"}}
```

#### `remove_text_model`

Delete a downloaded text model from this computer.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no (destructive: removes data)
- **CLI:** `capturemann ai text-models rm`
- **Spec:** AI-7

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model` | string | yes |  | A built-in text model id from list_text_models, e.g. `gemma-4-e2b`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `model`, `removed`.

Errors: `busy`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "remove_text_model", "arguments": {"model": "gemma-4-e2b"}}
```

#### `get_text_model`

Read how the text model (summaries, translation to any language, reports) is set up.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai text-model`
- **Spec:** AI-7

No input.

Returns: `provider`, `model`, `embedded_model`, `engine`, `context_tokens`, `timeout_s`, `structured_output`.

Errors:  (plus the common errors above).

```json
{"name": "get_text_model", "arguments": {}}
```

#### `test_text_model`

Send one short fixed prompt to the configured text model and report `ok`, `latency_ms`, `model` and whether structured output worked on the first try (`structured_output`).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai text-model test`
- **Spec:** AI-7

No input.

Returns: `ok`, `latency_ms`, `model`, `structured_output`, `reason`, `message`.

Errors: `invalid_input` (plus the common errors above).

```json
{"name": "test_text_model", "arguments": {}}
```

### Transcript and captions

#### `transcribe`

Transcribe a recording on this computer with an installed speech model (no network).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai transcribe`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `language` | string | no | "auto" | `auto` to detect, a language code such as `en`, `vi` or `es`, or codes separated by commas to choose among, such as `vi,en` (default: the languages in the `ai.languages` setting, else detect among all). |
| `model` | string | no |  | A speech model id from list_models, e.g. `base`, `small` or `medium`. (pattern `^[a-z0-9][a-z0-9._-]{0,39}$`) |
| `original_audio` | boolean | no |  | true: read the original microphone, as it was recorded; false: read the noise-reduced copy a recording may store. Omitted: the user's setting `ai.transcribe_raw_mic`, which is on by default (the original microphone... |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `reused`, `capture_id`, `model`, `language`, `original_audio`.

Errors: `invalid_input`, `network`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "transcribe", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "language": "en"}}
```

#### `get_transcript`

Read a recording's transcript (words with start and end in session time, ticks of 1/120000 s, and confidence) and its caption lines.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai transcript`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `partial`, `covered_secs`, `total_secs`, `transcript`, `speakers`, `captions`, `captions_source`.

Errors: `not_found`, `busy` (plus the common errors above).

```json
{"name": "get_transcript", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

#### `get_captions`

Read a recording's caption lines (start and end in session time, ticks of 1/120000 s, and text of at most 2 rows of 42 characters).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai captions get`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `language` | string | no |  | A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions. |

Returns: `capture_id`, `lines`, `source`, `language`, `engine`, `model`, `edited`, `edited_lines`, `untranslated_lines`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_captions", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

#### `set_captions`

Replace a recording's caption lines with edited ones (with `language`, the lines of that translation.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai captions set`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `lines` | array of object {start, end, text} | yes |  |  (max items 5000) |
| `language` | string | no |  | A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `revision`, `action`, `by`, `lines`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_captions", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "lines": [{"start": 1.5, "end": 3, "text": "Welcome to the demo."}]}}
```

#### `replace_in_captions`

Find and replace across all caption lines of a recording (the edited lines, else the lines generated from the transcript).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai captions replace`
- **Spec:** AI-6a

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `replacements` | array of object {find, replace, match_case, whole_word} | yes |  |  (min items 1, max items 100) |
| `language` | string | no |  | A translation's language code (`vi`, `pt-br`) made by translate_captions; omit for the original captions. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `replacements`, `lines_touched`, `changed`, `revision`, `action`, `by`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "replace_in_captions", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "replacements": [{"find": "cube control", "replace": "kubectl"}]}}
```

#### `set_caption_options`

Turn a recording's captions on or off.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai captions options`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `enabled` | boolean | no |  | Show the captions in the editor's preview. |
| `burn_in` | boolean | no |  | Burn the captions into exported videos by default. |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`enabled`) or (`burn_in`).

Returns: `capture_id`, `enabled`, `burn_in`, `changed`, `revision`, `action`, `by`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_caption_options", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "burn_in": true}}
```

#### `export_captions`

Write the captions as an SRT, VTT or plain-text (`txt`) file in the capture's `exports/` folder (with `language`, a translation.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann export --captions <srt|vtt|txt|md>`
- **Spec:** AI-1

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `format` | srt \| vtt \| txt \| md | yes |  |  |
| `language` | string | no |  | A translation's language code (`vi`, `pt-br`) made by translate_captions (srt, vtt or txt); omit for the original captions. |
| `timestamps` | boolean | no |  | Only with `txt`: prefix every line with [mm:ss] (or [hh:mm:ss] for long recordings). Default false. |
| `speaker_names` | boolean | no |  | Only with `srt`: start every cue with `NAME:` (AI-8). Default false. Once the recording has speakers, VTT always uses voice tags (`<v Name>`), and `txt` and `md` write `Name: text` per turn. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `format`, `path`, `lines`, `speaker_names`.

Errors: `not_found`, `user_declined` (plus the common errors above).

```json
{"name": "export_captions", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "format": "srt"}}
```

#### `translate_captions`

Translate a recording's captions into another language.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai translate`
- **Spec:** AI-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `target_language` | string | yes |  | The language to translate into: a code such as `en`, `vi`, `es` or `pt-br`. |
| `engine` | auto \| whisper \| text_model | no | "auto" |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `job_id`, `state`, `reused`, `capture_id`, `language`, `engine`, `model`, `source_language`.

Errors: `invalid_input`, `not_found`, `user_declined` (plus the common errors above).

```json
{"name": "translate_captions", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "target_language": "vi"}}
```

#### `normalize_transcript`

Add punctuation, capital letters and paragraph breaks to a recording's transcript with the built-in text model (any other provider gives `no_text_model`), without changing a word.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai normalize`
- **Spec:** AI-10

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `language` | string | no |  | Language of the transcript for the prompt (`vi`, `en`, `pt-br`); default the transcript's. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `job_id`, `state`, `reused`, `capture_id`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "normalize_transcript", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

#### `get_normalized_transcript`

Read the result of normalize_transcript: `text` (the transcript with punctuation, capitals and paragraph breaks, speaker turns separated by a blank line) and `normalized` (per turn.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai normalized`
- **Spec:** AI-10

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `stale`, `text`, `normalized`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_normalized_transcript", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

### Speakers

#### `diarize`

Find who spoke when in a recording, on this computer (no network), so the transcript can show speaker names.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai speakers`
- **Spec:** AI-8

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `num_speakers` | integer | no |  | Exactly this many speakers; wins over min and max. (min 1, max 32) |
| `min_speakers` | integer | no |  |  (min 1, max 32) |
| `max_speakers` | integer | no |  |  (min 1, max 32) |
| `headset` | boolean | no |  | My microphone only hears me. Default false. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `state`, `reused`, `capture_id`, `headset`.

Errors: `invalid_input`, `network`, `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "diarize", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "num_speakers": 2}}
```

#### `set_speakers`

Rename, merge and reassign the speakers of a diarized recording (run `diarize` first).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai speakers rename`
- **Spec:** AI-8

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `rename` | object | no |  | Speaker id (as text) to the new name. |
| `merge` | array of object {from, into} | no |  |  (max items 500) |
| `assign` | array of object {start, end, speaker} | no |  |  (max items 500) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `changed`, `revision`, `action`, `by`, `speakers`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_speakers", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "rename": {"1": "Linh"}}}
```

### Suggestions

#### `suggest_zooms`

Suggest zoom regions from the recording's clicks and cursor activity (deterministic, no network).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai suggest zooms`
- **Spec:** AI-3

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `count`, `suggestions`, `apply_with`.

Errors: `network`, `not_found` (plus the common errors above).

```json
{"name": "suggest_zooms", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

#### `suggest_cuts`

Suggest cuts for silences (from the audio) and filler words (from the transcript, when one exists).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai suggest cuts`
- **Spec:** AI-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `count`, `suggestions`, `apply_with`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "suggest_cuts", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

### Reports

#### `list_report_templates`

List the built-in report templates (Summary, Meeting minutes, One-on-one notes, Interview notes, Product presentation recap, Lecture notes): their sections, the context fields each one uses and which recording presets suggest it.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai report templates`
- **Spec:** AI-9

No input.

Returns: `templates`.

Errors:  (plus the common errors above).

```json
{"name": "list_report_templates", "arguments": {}}
```

#### `generate_report`

Write a structured report (meeting minutes, notes, recap, summary) from a recording's transcript with the built-in text model (any other provider gives `no_text_model`).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai report`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `template` | summary \| minutes \| one_on_one \| interview \| product_recap \| lecture | yes |  | A template id from list_report_templates. |
| `language` | string | no |  | Language of the note text (`vi`, `es`, `pt-br`); default the transcript's. Quotes stay as spoken. |
| `context` | object {title, attendees, agenda, role, competencies} | no |  | Optional facts the notes may use. Only the fields the template lists in `context_fields` are accepted. |
| `report_id` | string | no |  | Regenerate this report. (pattern `^rpt_[0-9a-f]{8}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `job_id`, `state`, `reused`, `capture_id`, `template`, `report_id`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "generate_report", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "template": "minutes"}}
```

#### `get_report`

Read a report as JSON (`report.content`: sections of items with `at` times in seconds) and as Markdown.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann ai report get`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `report_id` | string | no |  |  (pattern `^rpt_[0-9a-f]{8}$`) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `report_id`, `report`, `markdown`, `reports`, `latest`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_report", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46"}}
```

#### `export_report`

Write a report as a Markdown (`md`) or plain text (`txt`) file, into `out_dir` (an existing absolute folder) or the recording's `exports/` folder.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai report export`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `report_id` | string | yes |  |  (pattern `^rpt_[0-9a-f]{8}$`) |
| `format` | md \| txt | yes |  |  |
| `out_dir` | string | no |  | An existing absolute folder. |
| `include_times` | boolean | no |  | Print the citation times ([mm:ss]) after each item. Default false: the text is cleaner without them; the stored report keeps them. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `report_id`, `format`, `path`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "export_report", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "report_id": "rpt_00000001", "format": "md"}}
```

#### `delete_report`

Delete one report of a recording.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai report delete`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `report_id` | string | yes |  |  (pattern `^rpt_[0-9a-f]{8}$`) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `report_id`, `revision`, `action`, `by`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "delete_report", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "report_id": "rpt_00000001"}}
```

#### `edit_report`

Replace a report's Markdown with your own text (the Report panel's edit).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai report edit`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `report_id` | string | yes |  |  (pattern `^rpt_[0-9a-f]{8}$`) |
| `markdown` | string | yes |  |  (min length 1, max length 400000) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `report_id`, `revision`, `action`, `by`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "edit_report", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "report_id": "rpt_00000001", "markdown": "# Minutes\n\n- Ship 0.3.4 on Friday"}}
```

#### `check_report_item`

Tick or untick one task of a report (an item of `action_items`, `agreed_actions`, `follow_ups` or `next_steps`; `index` counts from 0 in the section of `get_report`'s `report.content`).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann ai report check`
- **Spec:** AI-9

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `report_id` | string | yes |  |  (pattern `^rpt_[0-9a-f]{8}$`) |
| `section` | action_items \| agreed_actions \| follow_ups \| next_steps | yes |  |  |
| `index` | integer | yes |  |  (min 0, max 999) |
| `checked` | boolean | no | true |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |

Returns: `capture_id`, `report_id`, `section`, `index`, `checked`, `changed`, `revision`, `action`, `by`, `can_undo`, `can_redo`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "check_report_item", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "report_id": "rpt_00000001", "section": "action_items", "index": 0, "checked": true}}
```

### Click Guide (Preview)

#### `start_guide`

Start a Click Guide: the user clicks through a task once and CaptureMann takes a screenshot of each mouse-down in the followed target, with a numbered badge and a ring at the click point and a draft title from the text nearest the...

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann guide start`
- **Preview:** yes. `_meta.stage` is `preview`; the tool may still change.
- **Spec:** GDE-1, GDE-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `target` | object {display_id} or object {window_id} or object {display_id, rect} | yes |  | What to follow, as in capture_screenshot (`pick` is not supported). |
| `title` | string | no |  | Default: `Click Guide <date time>`. |
| `countdown_s` | number | no | 3 |  (min 0, max 10) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `guide_id`, `title`, `intro`, `numbering`, `state`, `step_count`, `window_closed`, `steps`, `can_undo`, `can_redo`, `discarded`.

Errors: `permission_missing`, `busy`, `not_implemented`, `user_declined`, `confirmation_timeout` (plus the common errors above).

```json
{"name": "start_guide", "arguments": {"target": {"display_id": 1}, "title": "Export a video", "countdown_s": 3}}
```

#### `stop_guide`

Stop the Click Guide that is recording and return the finished guide (`get_guide` shape).

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann guide stop`
- **Preview:** yes. `_meta.stage` is `preview`; the tool may still change.
- **Spec:** GDE-1, GDE-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `guide_id` | string | no |  | The guide to stop; default: the one recording. (min length 1) |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `guide_id`, `title`, `intro`, `numbering`, `state`, `step_count`, `window_closed`, `steps`, `can_undo`, `can_redo`, `discarded`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "stop_guide", "arguments": {}}
```

#### `get_guide`

Read a guide: title, intro, numbering, and the ordered steps with title, note, click point and `image_id` (a capture id for the step's screenshot, so `get_capture` returns the image and `recognize_text` reads the step's text).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann guide get`
- **Preview:** yes. `_meta.stage` is `preview`; the tool may still change.
- **Spec:** GDE-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `guide_id` | string | no |  | Default: the guide that is recording. (min length 1) |

Returns: `guide_id`, `title`, `intro`, `numbering`, `state`, `step_count`, `window_closed`, `steps`, `can_undo`, `can_redo`, `discarded`.

Errors: `not_found` (plus the common errors above).

```json
{"name": "get_guide", "arguments": {"guide_id": "guide_REGRESSFIXTURE01"}}
```

#### `set_guide`

Edit a guide.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann guide set`
- **Preview:** yes. `_meta.stage` is `preview`; the tool may still change.
- **Spec:** GDE-2, GDE-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `guide_id` | string | no |  | Default: the guide that is recording. (min length 1) |
| `title` | string | no |  |  |
| `intro` | string | no |  |  |
| `numbering` | number \| upper \| lower | no |  |  |
| `steps` | array of object {id, title, note, delete, duplicate} | no |  |  |
| `focus_crop` | boolean | no |  |  |
| `crop_steps` | array of string | no |  |  |
| `retake` | array of string | no |  |  |
| `paused` | boolean | no |  |  |
| `note_step` | object {title, note} | no |  |  |
| `remove_last` | boolean | no |  |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `guide_id`, `title`, `intro`, `numbering`, `state`, `step_count`, `window_closed`, `steps`, `can_undo`, `can_redo`, `discarded`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "set_guide", "arguments": {"guide_id": "guide_REGRESSFIXTURE01", "title": "Export a video", "numbering": "number"}}
```

#### `export_guide`

Export a stopped guide. Markdown goes straight into `dest` as `guide-YYYY-MM-DD-HHMMSS.md` and `images-YYYY-MM-DD-HHMMSS/step-NN.png` (local time); nothing is overwritten, a `dest` that is a file that already exists is refused, and so is a guide with no step.

- **Tier:** Free, changes something: runs at once inside the library and the user's export folders; the user can set it to Ask first. A `dest` anywhere else is Confirm for that call (0.3.5), and the dialog shows the exact path.
- **Read-only:** no
- **CLI:** `capturemann guide export`
- **Preview:** yes. `_meta.stage` is `preview`; the tool may still change.
- **Spec:** GDE-3, GDE-4

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `guide_id` | string | yes |  | The guide to export. (min length 1) |
| `format` | markdown \| html \| pdf \| rich_text | yes |  |  |
| `dest` | string | yes |  | Absolute path: a folder (markdown), a file or folder (html, pdf). Required; with `rich_text` it is ignored (pass any path). |
| `headings` | numbered \| step \| title_only | no | "numbered" |  |
| `image_scale` | 1 \| 2 | no | 1 |  |
| `paper` | a4 \| letter | no | "a4" | Only for pdf. |
| `intro` | boolean | no | true |  |
| `notes` | boolean | no | true |  |
| `note_steps` | boolean | no | true |  |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `guide_id`, `format`, `steps`, `path`, `file`, `renamed`, `files`, `copied_to_clipboard`, `html`, `warnings`.

Errors: `user_declined`, `not_found`, `invalid_input` (a `dest` that is a file that already exists; a guide with no step) (plus the common errors above).

```json
{"name": "export_guide", "arguments": {"guide_id": "guide_REGRESSFIXTURE01", "format": "markdown", "dest": "/Users/me/Documents/guides"}}
```

### Quick Capture presets (Preview)

#### `list_presets`

List the user's Quick Capture presets: named setups for a capture (a screenshot, a video or an audio recording). Run one by passing its name or id as `preset` to `capture_screenshot` or `start_recording`. Four starter presets exist at first: Bug report, Docs shot, Demo video and Meeting. Presets are changed only in the CaptureMann app; a call that tries to change one over MCP is refused.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann presets list`
- **Spec:** QC-5
- **Preview:** yes

No input.

Returns: `presets` (each with `id`, `name`, `kind`, `target`, `options`, `save_to`, `after`, `shortcut`, `summary`) and `notice` (a sentence naming presets of a hand-edited file that were skipped because they are not valid, else null).

```json
{"name": "list_presets", "arguments": {}}
```

### Destinations and publishing

#### `list_destinations`

List the destinations the user set up in Settings (a folder on this computer, a drive or a network share, or a connected Google Drive, OneDrive or Dropbox account).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann destinations`
- **Spec:** STO-1

No input.

Returns: `destinations`.

Errors: `network`, `auth_expired` (plus the common errors above).

```json
{"name": "list_destinations", "arguments": {}}
```

#### `upload`

Copy a capture (or one file) to a destination in the background.

- **Tier:** Free, changes something: runs at once; the user can set it to Ask first.
- **Read-only:** no
- **CLI:** `capturemann upload`
- **Spec:** STO-1, STO-3, STO-5

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | no |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `file` | string | no |  | Absolute path of a file to copy instead of a capture. (min length 1) |
| `destination_id` | string | no |  | A destination id from list_destinations; the default one when absent. |
| `folder` | string | no |  | Folder inside the destination; replaces its folder pattern. (min length 1) |
| `what` | export \| original \| both | no |  | What to send of a capture; the destination's own choice when absent. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

At least one of: (`capture_id`) or (`file`).

Returns: `job_id`, `destination_id`, `what`, `state`.

Errors: `user_declined`, `not_found` (plus the common errors above).

```json
{"name": "upload", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "destination_id": "gdrive_main", "what": "export"}}
```

#### `list_publish_accounts`

List the accounts the user connected for publishing (YouTube channels today).

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann publish accounts`
- **Spec:** PUB-1, PUB-2

No input.

Returns: `accounts`, `platforms`.

Errors:  (plus the common errors above).

```json
{"name": "list_publish_accounts", "arguments": {}}
```

#### `get_platform_rules`

Read the limits of a publishing platform before exporting or uploading.

- **Tier:** Free, read-only: always allowed, never asks, no setting.
- **Read-only:** yes
- **CLI:** `capturemann platform-rules`
- **Spec:** EXP-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `platform` | youtube \| tiktok \| facebook | yes |  | Only `youtube` has rules today. |

Returns: `platform`, `limits`, `required_fields`, `privacy_options`, `default_privacy`, `unverified_project_privacy`, `default_category`, `recommended_preset`, `presets`, `presets_source`, `warning`.

Errors: `invalid_input`, `not_implemented` (plus the common errors above).

```json
{"name": "get_platform_rules", "arguments": {"platform": "youtube"}}
```

#### `publish_video`

Publish a recording to YouTube.

- **Tier:** Confirm, locked: always asks on screen; the setting cannot be changed.
- **Read-only:** no
- **CLI:** `capturemann publish`
- **Spec:** PUB-2

| Name | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `capture_id` | string | yes |  | Capture id, e.g. from capture_screenshot or list_library. (min length 1) |
| `platform` | youtube | yes |  | Only `youtube` is built; `tiktok` and `facebook` fail with `not_implemented`. |
| `account_id` | string | no |  | From list_publish_accounts; the default account when absent. |
| `title` | string | no |  | Required, at most 100 characters, no < or >. (max length 100) |
| `description` | string | no |  | At most 5,000 bytes, no < or >. |
| `tags` | array of string | no |  | At most 500 characters in total. |
| `privacy` | private \| unlisted \| public | no |  | The account's default (never public) when absent. |
| `made_for_kids` | boolean | no |  | Required by YouTube; there is no default, ask the user. |
| `thumbnail` | string | no |  | Absolute path of a JPG or PNG up to 2 MB. |
| `export_preset` | string | no |  | Export the recording with this preset first (for example `youtube-1080p`); the newest existing export otherwise. |
| `export_path` | string | no |  | Absolute path of an export of this capture to send instead of the newest one; not together with `export_preset`. |
| `captions` | boolean | no | false | Also upload the recording's captions as an SRT track (needs captions or a transcript). |
| `category` | string | no |  | YouTube category id; 28 (Science & Technology) when absent. |
| `playlist_id` | string | no |  | Add the video to this playlist (a playlist id). The confirmation dialog shows the id, not the title. |
| `metadata` | object | no |  | The same fields as above in one object (title, description, tags, privacy, made_for_kids, thumbnail, captions, category, playlist_id); top-level fields win. |
| `request_id` | string | no |  | Idempotency key; a repeat within 24 h returns the first result. (min length 1, max length 256) |

Returns: `job_id`, `capture_id`, `platform`, `account_id`, `account`, `title`, `privacy`, `made_for_kids`, `file`, `export_preset`, `export_job_id`, `captions`, `state`.

Errors: `invalid_input`, `network`, `user_declined`, `confirmation_timeout`, `not_found` (plus the common errors above).

```json
{"name": "publish_video", "arguments": {"capture_id": "rec_06GYEVMCKZSRHR46", "platform": "youtube", "title": "Kubernetes probes in 5 minutes", "privacy": "unlisted", "made_for_kids": false}}
```

