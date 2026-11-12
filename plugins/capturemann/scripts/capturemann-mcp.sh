#!/bin/sh
# Starts the CaptureMann MCP server (`capturemann mcp`) for this plugin. The plugin's .mcp.json runs this file,
# so the program that starts is the one CaptureMann installed on this computer, not something the plugin ships.
# The command comes from the plugin option "CaptureMann command", which .mcp.json passes in CAPTUREMANN_COMMAND;
# empty or unset means `capturemann` on the PATH.
cmd="${CAPTUREMANN_COMMAND:-capturemann}"
if ! command -v "$cmd" >/dev/null 2>&1; then
  echo "CaptureMann: cannot find the command '$cmd'. Install CaptureMann (https://www.craftmann.app/capturemann/) and set the plugin option 'CaptureMann command' (/plugin configure capturemann) to the full path of the capturemann program." >&2
  exit 127
fi
exec "$cmd" mcp "$@"
