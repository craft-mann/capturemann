# Agent (AGT)

The agent interface is a first-class client of the same action bus as the GUI (Agent interface tab). These requirements define the server, the safety model and the undo/event contract; each feature spec above lists its own tools.

### AGT-1 MCP server, CLI and JSON-RPC over one action bus
**Priority** P0 · **Status** In progress (built on macOS and tested with fakes and scripted clients: the action bus, JSON-RPC over the Unix socket, the CLI, the `cm-mcp` server with 53 tools over stdio and HTTP (the 11 local-AI tools run against a scripted engine; with no speech engine plugged in they answer `not_implemented`), MCP event notifications (tested with fakes), `capturemann mcp`, the parity tests and the resource limits; MCP resources and prompts (Oct 24, 2026, tested with a scripted rmcp client over the daemon with fakes, over stdio and in-process; the same `McpServer` serves `--http`); not built: the cursor resource, the keychain-stored HTTP token, the Windows named pipe; not run: an MCP conformance test with the official inspector, a real Claude Desktop session) · **Crates** cm-daemon, cm-mcp, cm-cli · **Depends on** —

**User story** — As a developer using an AI agent, I want CaptureMann exposed through MCP and a CLI, so that my agent and my scripts can do everything I can do in the app.

**Description**
- `cm-daemon` runs the action bus; the GUI, overlay, CLI and MCP server are its clients. If the GUI isn't running, the CLI or MCP server starts a headless daemon (no windows except overlays and confirmations).
- **MCP** (`rmcp`): transports stdio and local Streamable HTTP (bound to 127.0.0.1, random port, bearer token, shown once in Settings → Agents & CLI). **As built, the MCP server is the separate `cm-mcp` binary** (`cm-mcp` for stdio, `cm-mcp --http`); the `capturemann mcp` subcommand launches it, and the Settings client snippet runs `capturemann mcp` (with `--socket` when the app serves a non-default socket). The HTTP server caps connections at 64 and applies a 30 s header-read timeout and an idle timeout. Built: **53 tools** (counted from `crates/cm-mcp/src/tools.rs`; the daemon registry `ACTIONS` has 56 entries, the other 6 being `INTERNAL_ACTIONS` for first-party clients), each with JSON Schema input and output. **Resources and prompts are built (Oct 24, 2026, tested with fakes):** `initialize` advertises `resources` (no `subscribe`, no `listChanged`: the spec does not ask for them) and `prompts`; `resources/list` (library, jobs, YouTube rules), `resources/templates/list` (capture, transcript, platform rules), `resources/read` and `prompts/list`, `prompts/get` (`record-tutorial`, `publish-everywhere`, `bug-report-screenshot`, `lesson-to-drive`, plus `caption-cleanup` and `make-short-clip`). A read is the Free daemon action of the matching tool (`list_library`, `list_jobs`, `get_capture` without the image, `get_transcript`, and the `platforms` part of `list_publish_accounts`), so the per-client policy and errors are the same as for tools (a Blocked client gets the daemon's refusal with `data.reason`; a Read-only client may read); a missing capture or platform is the MCP `resource_not_found` (-32002), a malformed id `invalid_params`, a body over 1 MB `too_large` (not cut). Nothing returns a token or keychain key. **Not built:** the cursor resource (`capturemann://capture/{id}/cursor`: no daemon action serves the cursor log), the poster frame in `capturemann://capture/{id}` (use `get_capture`), a real client session, and the notification topics left out on purpose (`recording.stats`, `recording.health`, `recording.countdown`). **Event notifications are built:** each daemon event is sent as an MCP `notifications/message` (logger `capturemann.events`, `data` = `{ seq, event, at, data }`), the server declares the `logging` capability and honours `logging/setLevel`, the daemon policy for the client decides (blocked: none), and a slow client gets `events.dropped` (AGT-1).
- **JSON-RPC 2.0**: Unix socket (macOS) / named pipe (Windows), one request per line; method names equal MCP tool names; `events.subscribe` streams events.
- **CLI**: `clap`; every command has `--json`; exit codes per `specs/README.md`.
- Tool list is generated from one Rust definition per action (name, schema, tier, handler), so MCP, JSON-RPC and CLI can't drift; a test fails if any action lacks a tool or CLI mapping.
- Settings UI (kit `Settings` → Agents & CLI, DESIGN §5.8): header with an agent-blue status pill ("MCP server running · stdio"; the transports in use are listed after the dot, and the pill shows "stopped" in `mist` when the daemon is unreachable); a **Client config** card with the JSON snippet and a Copy button; a per-capability list with an Allow / Ask first control (mapping in the Agent interface tab, "Two controls, one model"). **As built:** the snippet's command is the real path of the bundled `capturemann` binary (resolving the kit's `[PATH TO CLI]`) with `args: ["mcp"]`, plus `--socket` when the app serves a non-default socket. The status pill reads "MCP ready" or "MCP offline".
- Setup helpers (**not built**; only Copy exists): Settings → Agents & CLI → "Add to Claude Desktop / Claude Code / Cursor" writes the client's MCP config (with confirmation), or shows the JSON to paste. The kit does not show this button or the client list; keep them below the Client config card.

**Acceptance criteria**
- **AC1** Given Claude Desktop configured with `capturemann mcp`, then it lists all tools with schemas and can call `list_targets`.
- **AC2** Given the GUI isn't running, when the CLI runs `capturemann capture screenshot --display 1`, then a headless daemon starts and the command completes. A CLI-started daemon behaves like the app: same default library (`CAPTUREMANN_LIBRARY`, then `library.path`, then `~/Movies/CaptureMann`) and the same file-backed `settings.json`, so the 60 fps verdict and settings survive. If the app already serves the socket, the CLI uses it. A headless daemon has no confirmation UI: Confirm-tier actions are refused with `user_declined` and `data.reason: "no_confirmer"` ("No confirmation UI is attached to this daemon"). It shuts down on SIGTERM or SIGINT (a second signal exits at once), finalizing a running recording and waiting at most 10 s for running jobs.
- **AC3** Given `--http`, then requests without the bearer token get 401 and the server isn't reachable from other machines.
- **AC4** Given a new action added without a CLI mapping, then CI fails (parity test), unless the action is listed in `INTERNAL_ACTIONS` (first-party GUI helpers, not exposed as CLI commands or MCP tools: `freeze_display`, `release_frame`, `discard_recording`, `restart_recording`, `skip_countdown` and `rescan_library`; the two that delete a take are Confirm tier for agents, and the list has a test that pins its length).
- **AC5** Given 10 concurrent tool calls from two clients, then they're processed without deadlock and each returns.
- **AC6** (added Oct 21, 2026, after the agent-layer review) Given a client exceeds a resource limit, then the daemon degrades predictably instead of growing without bound: at most 64 requests in flight per connection and 64 connections (more get `busy`), at most 32 MiB of unread output per connection, requests of at most 16 MiB per line, a confirm queue of at most 3 pending per client and 16 in total (excess get `busy`), an activity log of at most 2,000 entries, and an idempotency cache of at most 512 entries and 64 MB in which images are dropped from replayed results above 64 KiB. `rpc.cancel { id }` cancels one in-flight request of the same connection.

**Edge cases and errors** — Daemon crash → clients get a connection error and the next call restarts it; MCP protocol version mismatch → clear error naming supported versions. A client that cannot reach the daemon gets `daemon_unavailable` and CLI exit 5 (also for `capturemann daemon status`). JSON-RPC: an `id` of `null` is accepted but discouraged and is answered with `id: null`; an `id` that is an object, array or boolean is `-32600`; batches (arrays) are refused with `-32600`. Existing non-socket files at the socket path are never deleted (`invalid_input`), and the socket is created under a restrictive umask (0700 directory, 0600 socket).

**Agent interface** — This is the interface.

**Out of scope** — Remote (non-localhost) access, multi-user servers.

**Test notes** — MCP conformance tests with the official inspector; parity test; concurrency stress test.

---

### AGT-2 User confirmation for recording, publishing and deleting
**Priority** P0 · **Status** In progress (built on macOS: tiers, the gate with one dialog at a time, the 5-minute timeout, per-client policy, the activity log, the confirmation window in `apps/desktop`; tested with fakes and scripted clients; how the real window renders the recording rows is unverified; the Never tier has no operations because none exist) · **Crates** cm-daemon, apps/desktop · **Depends on** AGT-1, DESIGN §5.6, DESIGN §5.8 (kit `Settings`)

**User story** — As a user, I want to approve any agent request that records me, publishes publicly or deletes my work, so that an agent can't do those things without me.

**Description**
- Tiers (Agent interface tab): **Free** (runs immediately), **Confirm** (`start_recording`, `run_fps_selftest` (it captures real pixels), `publish_video`, `delete_capture`, moving the library, deleting remote uploads, and the internal `discard_recording` and `restart_recording`, which delete a take; in-process user calls skip the dialog), **Never** (reading tokens/keys, hiding the recording indicator, removing default excluded apps).
- Confirm flow: the daemon shows a native modal on the active display (DESIGN §5.6): agent chip ("Requested by Claude Desktop"), one-sentence summary, details (file, account, privacy, size), Cancel (default focus) and a primary button naming the action. Enter is ignored for 1 s after the dialog appears.
- **Dialog rows for recording** (the details list of the Confirm dialog, in this order):
  - `start_recording`: **Target** (display and size, window title and app, or region and display), **Output** (size in px, fps, codec, estimated MB per minute), **Screen** ("On (always recorded)"), **Microphone** (device and noise suppression, or Off), **Camera** (device and resolution, "the camera light turns on", or Off), **System audio**, **Cursor** (position, clicks and scroll direction, "never keystrokes", or Off), **Save to** (library folder and title), **Max duration** ("No limit" or the time), **Countdown**, **Excluded windows** ("CaptureMann's own windows and your password managers never appear").
  - `restart_recording`: the same rows for the new take, plus **Discarded** ("The current take is deleted and cannot be recovered"); the summary starts "Discard the current take and start over: record ...".
  - `discard_recording`: summary "Discard the current recording and delete its files" with rows **Recording** (id), **Title**, **Recorded** (time) and **This cannot be undone**.
  The rows are an **ordered list** (the JSON object keeps insertion order, `serde_json` with `preserve_order`), shown in the order above. **The dialog shows exactly the plan that runs:** the plan built for the dialog is the plan the engine executes after approval, with no second planning pass that could differ (permission or device changes in between fail the start rather than change the recording). Rows include `disk_space` and a `disk_low` warning when under 10 minutes are free (REC-8). The dialog text is built by the recording engine in `cm-daemon` and tested with fakes; how the app's confirm dialog renders these rows for a real recording has not been verified. `skip_countdown` is **first-party only**: an agent calling it is refused with `first_party_only`, so it cannot shorten the countdown the user approved.
- The tool call blocks until answered, up to 5 minutes → `user_declined` or `confirmation_timeout`.
- Per-client policy (Settings → Agents & CLI): each client can be Allowed (default), Read-only (Confirm and mutating Free actions refused) or Blocked. **A policy is keyed by the client's self-declared name** (`session.hello`) and is **not a security boundary against a malicious local process**, which can declare any name or connect under a different one; it limits well-behaved agents and shows the user who asked. What is enforced regardless of name: the Confirm gate, the app-only settings list (below), and the resource limits. A connection's name is **immutable** once declared (a second `session.hello` with a different name is refused), and `session.hello`, `events.subscribe` and `actions.list` are policy-gated like every action (a Blocked client gets `client_blocked` from all three). **Fail closed (built Oct 21, 2026):** `ClientPolicies::set_default(Blocked)` makes a client without an entry refuse with `client_blocked`-style error data `reason: "default_deny"`; a named Allowed entry still passes. The desktop sets it when the policy store was unreadable and could not be restored, remembers it in the app-only key `agents.deny_unknown_clients`, and shows the banner "Unknown clients are blocked until you review the list" on the Agents page with an "Allow unknown clients" button that returns the default to Allowed. On a normal install the default stays Allowed. This is distinct from the kit's per-capability Allow / Ask first control, which is a UI label for Free vs Confirm (see the Agent interface tab); a Confirm-tier capability cannot be set to Allow. Decision OD-2 (Oct 21, 2026) fixes this: the kit screen's "Allow" for recording is overridden, and per-capability tightening of Free actions is P2, not v0.1.
- "Allow for this session" checkbox for `start_recording` only (not for publish or delete), expiring when the client disconnects.
- Every confirmed and declined request is logged in an Agent activity log (Settings → Agents & CLI), kept 30 days locally and capped at 2,000 entries (long client names and summaries are truncated).
- **`set_settings` is limited for agents.** Socket clients cannot write the app-only keys `library.*`, `privacy.*`, `shortcuts.*`, `permissions.*`, `agents.*`, `storage.*`, `export.*`, `mcp.*`, `auth.*`, `appearance.language`, `recording.fps60_selftest`, `recording.hud_positions`, `app.*` and `launch_at_login` (refused with `user_declined` and `data.reason: "app_only_setting"`); only the in-process app can. A key is a dotted lowercase path of at most 128 bytes, a value at most 16 KiB serialized and 8 levels deep, and a call at most 64 keys. Any key containing `token`, `secret`, `password`, `passwd`, `api_key`, `apikey`, `credential`, `private_key` or `bearer` is refused (substring match, not segment equality). The registry of keys is in the Technical design tab.

**Acceptance criteria**
- **AC1** Given an agent calls `publish_video`, then nothing is uploaded until the user clicks the action button.
- **AC2** Given the dialog appears, when the user presses Enter within 1 s, then nothing happens; after 1 s Enter activates the focused button (Cancel by default).
- **AC3** Given no answer for 5 minutes, then the call returns `confirmation_timeout` and the dialog closes.
- **AC4** Given a client set to Read-only, when it calls `trim`, then it gets `user_declined` with `data.reason: "client_read_only"` and no dialog.
- **AC5** Given any Never-tier operation, then there's no tool for it.
- **AC6** Given the activity log, then it lists client, tool, summary, decision and time for each request.

**Edge cases and errors** — Screen locked or user away → dialog waits (timeout applies); several requests at once → queued, one dialog at a time.

**Agent interface** — Events `confirmation.requested`, `confirmation.resolved { decision }`.

**Out of scope** — Remote approval from a phone (P2).

**Test notes** — UI automation for dialog timing; policy unit tests.

---

### AGT-3 One undo step per call, and live events
**Priority** P0 · **Status** In progress (built on macOS and tested with fakes: one revision per call, idempotency, per-capture serialization of annotate, undo and redo, `capture.revised` events; the editor-side behaviors (AC2 toast, the text-field wait) and trim/cut/layout actions are not built) · **Crates** cm-core, cm-daemon · **Depends on** AGT-1, ANN-3

**User story** — As a user working alongside an agent, I want each agent change to appear live and undo in one step, so that I stay in control of my edits.

**Description**
- Each tool call maps to one action, which applies one batch of commands atomically (all or nothing) and records one revision (`by: "agent:<client>"`).
- Agent-triggered actions show the agent toast (DESIGN §4: agent-blue border, "Started by agent via MCP" plus the real tool name) and update the sidebar footer card ("MCP server running", last call and client name) from bus events.
- The GUI updates live from bus events; if the user is editing the same capture, the agent's change appears with a brief highlight and a toast "Claude trimmed 2.0–58.0 s · Undo".
- Conflicts: actions are serialized per capture; an agent action on a capture with an open text field in the editor waits until the field commits (max 2 s).
- Idempotency: mutating tools accept `request_id`; repeats within 24 h return the first result. The cache is bounded (512 entries, 64 MB; images are omitted from replays above 64 KiB), so a very old or very large result may be recomputed after eviction rather than replayed.

**Acceptance criteria**
- **AC1** Given an agent call that cuts 3 ranges, when the user presses Undo once, then all 3 are restored.
- **AC2** Given the editor is open on the capture, when an agent trims it, then the timeline updates within 200 ms and a toast offers Undo.
- **AC3** Given a tool call fails halfway (e.g. invalid second range), then no part of it is applied.
- **AC4** Given the same `request_id` twice, then the second call returns the first result and changes nothing.

**Edge cases and errors** — Undo of an agent action after the user made later edits → standard linear undo (the user's later edits are undone first), as in any editor.

**Agent interface** — Events `capture.revised { capture_id, revision, by }`; every mutating tool returns `{ revision }`.

**Out of scope** — Selective undo of a past revision without undoing later ones.

**Test notes** — Atomicity tests with injected failures; idempotency tests.
