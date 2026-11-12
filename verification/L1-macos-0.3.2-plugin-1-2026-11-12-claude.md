# L1 macOS 0.3.2 PLUGIN-1 (Claude Code plugin), scripted run (Nov 12, 2026)

- **Verdict:** PARTIAL (checks 1 to 4, 6 and 7 run by an agent with `claude -p`; checks 5, 8, 9 and 10 not run; no person looked)
- **Setup:** Claude Code 2.1.311, `--plugin-dir plugins/capturemann` at commit `5d493d08`; `capturemann` 0.3.2 from `target/bundle/CaptureMann.app` (the notarized DMG build of Nov 12); private config, library and socket in a scratch folder (removed afterwards); `cm-daemon` 0.3.2 from `target/release` started by hand with `capturemann daemon start`; the CaptureMann app was not running; no screenshot, no recording, no hardware.

| # | Check | Result |
| --- | --- | --- |
| 1 | `claude plugin validate --strict plugins/capturemann`, `claude plugin validate .`, `check_plugin_version.py` | PASS (both "Validation passed", version 0.3.2 matches) |
| 2 | Session loads with `--plugin-dir` | PASS (plugin listed, no error) |
| 3 | Server connected, tools present | PASS (`plugin:capturemann:capturemann` connected, 81 tools, `list_library`, `capture_screenshot`, `start_recording`, `publish_video` present) |
| 4 | Read-only calls | PASS (`list_library` returned an empty library, `get_settings` returned `{}` on the private config, no dialog; a guessed key `recording.size` gave `not_found`) |
| 5 | Client shown in Settings, Agents and CLI | NOT RUN (needs the app window; the client list is in-process, not exposed by the daemon) |
| 6 | Skills `capturemann-basics` and `meeting-notes` | PASS (both explained and called no tool) |
| 7 | Confirm tool without consent | PASS (`start_recording` ended `user_declined`, `reason: no_confirmer`; nothing recorded) |
| 8 | App closed, headless daemon | **FAIL against the docs**: with no daemon running, `list_library` and `get_settings` returned `daemon_unavailable: cannot connect to the CaptureMann daemon`. `capturemann mcp` runs `cm-mcp`, which connects to an existing daemon and does not start one; and the macOS bundle contains no `cm-daemon` binary (the app embeds the daemon), so the CLI cannot start one from the bundle either (`cannot run cm-daemon`). `plugins/capturemann/README.md` ("Limits") and `skills/capturemann-basics/SKILL.md` say the server starts a headless daemon: not true for this setup. The error itself is clear and the skill tells Claude to ask the user to open the app. |
| 9 | Wrong path fails clearly | NOT RUN |
| 10 | Marketplace install | NOT RUN (changes the user's Claude Code settings) |

Not tested: the app running with its real dialogs (check 5, and a confirmation dialog for check 7), Windows and Linux.
