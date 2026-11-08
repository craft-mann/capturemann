#!/usr/bin/env bash
# Run a command in ~/wt/desk on the lab laptop in the background with the build settings; log in ~/wt/<name>.log
# usage: desk-remote.sh <name> '<command>'
set -euo pipefail
cd "$(dirname "$0")/../.."
name="$1"; shift
scripts/testlab/lab.sh linux "cd ~/wt/desk; setsid nohup bash -c 'export CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR=\$HOME/wt/desk-target PATH=\$HOME/.cargo/bin:\$PATH; $*; echo EXIT=\$?' > ~/wt/$name.log 2>&1 < /dev/null &"
