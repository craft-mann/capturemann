#!/bin/bash
export WORK=/tmp/cm-velo-mac
export DOTNET_ROOT=$WORK/dotnet
export PATH=$WORK/dotnet:$WORK/tools:$PATH
export DOTNET_CLI_HOME=$WORK/dotnethome DOTNET_NOLOGO=1 DOTNET_CLI_TELEMETRY_OPTOUT=1
export CARGO_BUILD_JOBS=4
exec "$@"
