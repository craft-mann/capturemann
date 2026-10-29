# Diagnostics (DIAG)

Problems on a user's Mac are invisible to us unless the user can hand us the facts. Diagnostics keeps a log
file on the computer and builds one block of text the user can read and paste into an issue. Nothing is sent
anywhere, there is no telemetry, and the text holds no audio, transcript text, recording titles or file names.

### DIAG-1 Log file and Copy diagnostics
**Priority** P1 · **Status** In progress (0.2.4: the log file, redaction, panic records and the unclean-exit marker are built and tested headless; `get_diagnostics`, `capturemann diagnostics` and the About page card are built and tested with fakes; `tests/manual/DIAG-1.md` has no stored run) · **Crates** cm-core, cm-daemon, cm-cli, cm-mcp, apps/desktop · **Depends on** none

**User story** — As a user with a problem, I want to press one button and get a text I can check and paste into a bug report, so that I never have to describe my Mac or find crash logs.

**Description**
- **Log file.** One file per process (`desktop.log`, `daemon.log`, `overlay.log`) in `<config folder>/logs` (`CAPTUREMANN_CONFIG_DIR` moves it). Each process keeps the current file and the previous two, each at most 2 MB (fixed, no setting). Lines are `<UTC time> <process> <LEVEL> <target>: <message>`, one record per line, written without our own buffering. The levels are the stderr logger's (`cm_*` at info, everything else at warn, `RUST_LOG` overrides).
- **Redaction.** Before a line is written the home folder becomes `~`, the job store's `scrub_secrets` removes tokens, bearer values, secret-looking `key=value` pairs and credentials in URLs, newlines are folded and a line is cut at 4000 characters. Redaction is a safety net: code must not log transcript or report text, tool parameters, tokens or recording titles.
- **Panics and unclean exits.** A panic is logged with its place, message and a one-line backtrace. A `running-<process>.pid` marker exists while a process runs and is removed by a normal exit; a marker left by a process that is gone makes the next start log "the previous run did not end normally" once.
- **Copy diagnostics.** Settings → About → Diagnostics: "Show diagnostics" reads the report (nothing is read until the button is pressed), the text is shown in a box the user can scroll and read, "Copy" puts it on the clipboard, "Reveal log folder" opens the folder. The same text is `capturemann diagnostics` and the read-only MCP tool `get_diagnostics` (Free).
- **Sections:** App (version, bundle or plain binary, config and log folders), System (OS, chip, memory, cores), Permissions (microphone, screen recording, camera; read without asking), Models and engines (installed speech models, the text model, FFmpeg and the text engine helper, whether the speaker engine is built in), Settings (an allow-list of prefixes `recording.`, `capture.`, `ai.`, `appearance.language`, without secrets, long values cut), Library (counts and sizes, unreadable bundles, free space; never titles), Last recording (codec, size, fps, bitrate, late frames, stalls from its manifest) and the last 200 warnings and errors. A section that cannot be read says so; the report never fails because one part is broken.
- A last redaction pass runs over the finished text.

**Acceptance criteria**
- **AC1** Given a fresh start, then `logs/<process>.log` exists with the start line and version; given more than 2 MB of log lines, then the folder holds at most three files of the cap for that process.
- **AC2** Given a log record with a bearer token, a path under the home folder and a secret-looking value, then the file has none of them (the home folder is `~`).
- **AC3** Given a panic on a thread, then the message, its place and a backtrace are in the file.
- **AC4** Given a marker left by a dead process, then the next start logs the unclean exit once; a normal exit leaves no marker.
- **AC5** Given no network, then Copy diagnostics produces every section, and nothing is sent.
- **AC6** `get_diagnostics`, `capturemann diagnostics` and the About card give the same text; an agent may call the tool; a call with an extra parameter is refused.

**Edge cases and errors** — No daemon in the app: the card says the engine is not running. A library or models section that cannot be read shows the reason in its place. No log file yet: the log section is empty.

**Agent interface** — `get_diagnostics` → `{ text, sections: [{ title, lines }] }` (Free, read-only, no parameters); CLI `capturemann diagnostics` (`--json` prints the whole answer).

**Not yet** — Display count and scale factors and the Input Monitoring and Accessibility permissions (the app, not the daemon, knows them); the performance summary of the last run (PERF-1, 0.2.4).
