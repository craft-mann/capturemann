# QA 0.3.4 Settings rows, launcher entries, Accessibility card, Agents icon, macOS pass 2 (PARTIAL)

- **Date:** 2026-11-15
- **Who:** Claude (QA agent, second pass)
- **Machine:** Apple M4 Pro, macOS 26.6.2, one built-in display 1512x982 pt at 2x
- **Build:** installed dev bundle `~/Applications/CaptureMann-dev.app` of main `84bc00cd`, isolated library/config/socket under `/tmp/cmq2`; the maintainer's app was not running.

| Item | Result | Evidence |
| --- | --- | --- |
| Settings, Shortcuts: Long Shot row | PASS | "Long Shot", "Not set" and a Set button; Set showed "Press the new shortcut" with Cancel; pressing Option+Shift+4 stored it and the row showed the three keys with a Change button (`macos-pass2-settings-shortcuts.png` shows the row after setting) |
| Menu bar entries | PASS | the app menu lists Library, Long Shot, New Click Guide, Click Guide of a window..., Import file..., Settings... (read through System Events) |
| Launcher entries | NOT SEEN | the launcher (Option+Shift+Space) opens a 760x786 window that screen capture cannot see (floating windows are excluded). The source (`apps/desktop/src/launcher/commands.rs`) has "Long Shot" and "New Click Guide" commands; "Click Guide of a window" in the launcher not checked |
| Library buttons "New Click Guide" and "Click Guide of a window..." | PASS (seen) | on All captures and Click Guides pages. The window picker itself is hidden from capture and was not driven |
| Accessibility card (Long Shot) | PARTIAL | Settings, Permissions, last card: "Accessibility", Optional, Granted, text "Lets Long Shot scroll for you. Without it, scroll by hand and Long Shot still works.", needed for "Long Shot: Auto scroll" (`macos-pass2-permissions-accessibility.png`). Only the Granted state seen (this Mac has the grant); not-granted state and Grant access not run. The card uses the same keyboard icon as Input Monitoring |
| Sidebar MCP card gone | PASS | the sidebar has no MCP card in rail or full mode |
| Agents & CLI icon, MCP ready | PASS | blue icon; page badge "MCP ready" |
| Agents & CLI icon, MCP offline | PASS | started the app with an unusable socket path: icon grey, badge "MCP offline" (`macos-pass2-agents-mcp-offline.png`). Remark A1 (low): the text under it says "Another app owns the socket" although the cause was a missing folder. Deleting the socket file under a running app changed nothing (status is read at start) |

Not run: Input Monitoring not-granted card, Accessibility not-granted card, launcher entry text, picker.

## Verdict

PARTIAL.
