#!/usr/bin/env bash
# Runs on the lab laptop: take the lab lock, run another rec-*.sh script from the same folder, release the lock.
#   rec-hold.sh rec-kill9.sh 20
here="$(cd "$(dirname "$0")" && pwd)"
bash "$here/rec-lock.sh" take || exit 1
trap 'bash "$here/rec-lock.sh" release' EXIT
script="$1"; shift
bash "$here/$script" "$@" 2>&1 | tail -40
