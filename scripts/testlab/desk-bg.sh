#!/usr/bin/env bash
# Runs ON the lab laptop: desk-bg.sh <name> <command...>  starts the command detached in the desktop session environment,
# output in /tmp/wlx/<name>.txt (read it with desk-run.sh desk-cat.sh <name>). Waits <name> seconds? No: returns at once.
set -u
name="$1"; shift
mkdir -p /tmp/wlx
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
cd "$HOME/wt/desk" || exit 1
setsid nohup "$@" > "/tmp/wlx/$name.txt" 2>&1 < /dev/null &
echo "started $name"
