#!/usr/bin/env bash
# Waits (on this Mac) until the laptop's background cargo build (~/wt/rec-build.log) has finished.
set -u
here="$(cd "$(dirname "$0")" && pwd)"
until [ -n "$("$here/lab.sh" linux 'tail -2 ~/wt/rec-build.log | grep -E "Finished|error"')" ]; do sleep 10; done
"$here/lab.sh" linux 'tail -3 ~/wt/rec-build.log'
