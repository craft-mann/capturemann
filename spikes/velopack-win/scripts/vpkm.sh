#!/bin/sh
# Mac vpk wrapper (scratch)
export DOTNET_ROLL_FORWARD=Major
export DOTNET_ROOT=/opt/homebrew/opt/dotnet/libexec
export DOTNET_CLI_TELEMETRY_OPTOUT=1
exec /private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad/tools/vpk "$@"
