# L1 re-check of the latest build on Linux: CLI, MCP and the app on X11 (VM)

- **Date:** 2026-11-01
- **Who:** Claude (AI agent); nothing by a person
- **Machine:** QEMU VM, Ubuntu 24.04 aarch64, private Xvfb :95 (1600x900) with openbox, a throw-away library and config under `/tmp/t5`
- **Build:** release, from `5a89114` plus the settings and wording commits up to `2acd043`; `capturemann`, `cm-mcp`, `cm-daemon`, `cm-desktop` from `target/release`
- **Status:** PARTIAL (no real desktop, no real GPU; Wayland not driven in this pass)

| Step | Result | Note |
| --- | --- | --- |
| CLI: `targets`, `capture screenshot` (display, region, window), `capture get`, `annotate` (arrow), `library list/search/rename`, `undo`, `redo`, `devices`, `destinations`, `ai models`, `jobs list`, `record status`, `diagnostics` | PASS | the CLI starts `cm-daemon` itself |
| CLI: `annotate --suggest-redactions` on a window with a fake email, API key and phone number | PASS | tesseract found all three kinds |
| CLI: `settings set general.language` | INFO | answered `invalid_input` ("not a setting an agent can change"), as designed |
| CLI and MCP: `record start` / `start_recording` against the standalone daemon | INFO | `user_declined: No confirmation UI is attached to this daemon`; with the app running: `not_implemented` ("`start_recording` is not implemented yet") |
| MCP over stdio: initialize, `tools/list` (74 tools), `list_targets`, `capture_screenshot` (display, region, window), `annotate`, `get_capture`, `rename_capture`, `suggest_redactions`, `undo`, `redo`, `list_devices`, `get_diagnostics`, `list_models`, `list_text_models`, `transcribe` (no model: clear error with a hint), `delete_capture` (needs the app's confirmation) | PASS | |
| MCP over HTTP: `cm-mcp --http`, initialize with the bearer token, 401 without it | PASS | |
| App on X11: library, capture a display through the overlay (click a display), the toast with Annotate, Copy, Save as, Delete, the editor with Auto-redact | PASS | |
| App on X11: Settings without a recorder shows General, Capture (screenshot delay only), Destinations, Permissions, Privacy, Shortcuts, About; Permissions lists only Microphone | PASS | before the label change the page still read "Capture & recording" |
| App on X11 (new build): Record and mic buttons dimmed, "Tray icon unavailable" toast | PASS | seen in the screenshot |
| Wayland (GNOME 46 headless): CLI capture | NOT RUN | the portal asked for a decision nobody answered (`permission_missing`); covered by `L1-wayland-screenshot-2026-10-31-claude.md` |

Findings fixed in this pass: the Linux packages had no `cm-daemon` (see `L1-cli-daemon-linux-2026-11-01-claude.md`); the Record
button looked usable; the Capture page and tray toast used macOS wording. Open: the sticky tray toast covers part of the
editor on a desktop without a StatusNotifier host (Ubuntu's default GNOME has one).
