# QA 0.3.4 hygiene items, version label, library chips and the Agents & CLI icon on Windows (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (agent); nobody watched
- **Machine:** ThinkPad T490, Windows 11 Pro 10.0.26200, 1920x1080 at 150 %
- **Build:** the Windows package of main `26a05100` (first build) and the final build with commits `e8f8e122` and `ac730b17` (zip SHA-256 `2A3717F6B1C6B2955130874B1FC651128DE39F71388F7F2EDFF760AE98AA8F13`)
- **Method:** the app window captured with `CopyFromScreen` (only the app's own window, synthetic content), looked at by the agent. A picture of the Agents & CLI page in each state is kept in `verification/QA-0.3.4-shots/` (`windows-agents-cli-mcp-ready-2026-11-15.png`, `windows-agents-cli-mcp-offline-2026-11-15.png`), and the Audio page (`windows-library-audio-2026-11-15.png`). The other library pictures show thumbnails of an Edge window with a profile picture and were not kept.

## Steps

| Step | Result | Evidence |
| --- | --- | --- |
| App version label | PASS | the sidebar chip reads `v0.3.4` on every page; `capturemann --version` prints `capturemann 0.3.4` |
| Library filter chips, All captures | PASS (looked at) | `Imported 9`, `Uploaded 0`, `Published 0`, `Boards 5`, `Click Guides 4`, plus the button `New Click Guide`; sidebar counts Recordings 7, Audio 5, Screenshots 33, Click Guides 4, Published 0 |
| Library filter chips, Audio | PASS (looked at) | `Imported 0`, `Uploaded 0` only; button `Record audio` |
| Library filter chips, Recordings, Screenshots, Click Guides, Published | not re-checked on this build | the earlier report (`L1-windows-0.3.4-main-a1bf57dc-feature-cases-...`) lists them (Recordings: Imported, Uploaded, Published; Screenshots: Imported, Uploaded, Boards; Click Guides: none); my pictures of these pages were taken but not looked at |
| Agents & CLI icon and pill, MCP ready | PASS (looked at, pixel read) | the sidebar icon is blue (pixel at (60,871) in the window picture = `#4A5778`), the page pill says `MCP ready` and `No agent requests yet` |
| Agents & CLI icon and pill, MCP not ready (a second process, `cm-daemon.exe`, owned the socket before the app started) | PASS (looked at, pixel read) | the app starts and stays up; the sidebar icon is grey (`#4C4C51`), the pill says `MCP offline`, the line `Another app owns the socket`, and the client-config note reads `MCP is offline: Another app owns the socket.` in the warning colour |
| No MCP card in the sidebar | PASS | none in either picture |
| `library list` rows without a pixel size | PASS on the final build | `-` (see the Click Guide report) |
| Blue "new" dots on the new editor tools are gone | not run | editor not driven |
| PLUGIN-1 check, Windows CI GPU crashes, the flaky socket test, a full test run on a quiet machine, `scripts/check-release-assets.sh`, the Vietnamese review | not run | not Windows lab work (CI, the plugin check against Settings, a review) |

## Verdict

PARTIAL
