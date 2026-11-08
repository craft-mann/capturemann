#!/usr/bin/env bash
# Runs on the lab laptop: wait for the lab lock, run the long A/V recording (rec-live-av.sh) with the
# release build, release the lock.   rec-ten.sh [seconds, default 600]
secs="${1:-600}"
here="$(cd "$(dirname "$0")" && pwd)"
bash "$here/rec-lock.sh" take || exit 1
trap 'bash "$here/rec-lock.sh" release' EXIT
REGEN=1 BIN="${BIN:-$HOME/wt/rec-target/release}" bash "$here/rec-live-av.sh" "$secs" 2>&1 | tail -40
