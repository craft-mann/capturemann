#!/usr/bin/env bash
# Run one of the synced scripts on the lab laptop in the foreground: desk-run.sh <script-under-scripts/testlab> [args...]
set -euo pipefail
cd "$(dirname "$0")/../.."
s="$1"; shift
scripts/testlab/lab.sh linux "bash \$HOME/wt/desk/scripts/testlab/$s $*"
