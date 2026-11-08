#!/usr/bin/env bash
# Runs on the lab laptop: the silent long recording of the quiet hours (a flashing window, no sound), with the lab lock.
#   rec-ten-quiet.sh [seconds, default 600]
here="$(cd "$(dirname "$0")" && pwd)"
bash "$here/rec-lock.sh" take || exit 1
trap 'bash "$here/rec-lock.sh" release' EXIT
SILENT=1 REGEN=1 BIN="${BIN:-$HOME/wt/rec-target/release}" bash "$here/rec-live-av.sh" "${1:-600}" 2>&1 | tail -40
