#!/usr/bin/env bash
# Runs ON the lab laptop in a Wayland session (lab lock taken by the caller): `capturemann capture screenshot --pick` against
# a private library, answered by a virtual keyboard: Return (the shell saves its picture) and Esc (cancelled).
# Usage: desk-wl-pick.sh <target-dir>
set -u
T="$1"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
W="$(mktemp -d /tmp/cmd-pick.XXXXXX)"
export CAPTUREMANN_CONFIG_DIR="$W/config" CAPTUREMANN_LIBRARY="$W/library" CAPTUREMANN_SOCKET="$W/s.sock"
mkdir -p "$W/config" "$W/library"
trap 'pkill -f "$W/s.sock" 2>/dev/null; rm -rf "$W"' EXIT
UI="sudo python3 $(dirname "$0")/desk-uinput.py"
echo "session: ${XDG_SESSION_TYPE:-?}"
echo "--- pick, then Return in GNOME's screenshot UI"
"$T/debug/capturemann" capture screenshot --pick > "$W/a.txt" 2>&1 &
P=$!
sleep 7
$UI seq "key Return" "wait 2"
wait $P; echo "exit $?"; cat "$W/a.txt"
echo "--- pick, then Esc"
"$T/debug/capturemann" capture screenshot --pick > "$W/b.txt" 2>&1 &
P=$!
sleep 7
$UI seq "key Escape" "wait 2"
wait $P; echo "exit $?"; cat "$W/b.txt"
ls -lt "$HOME/Pictures/Screenshots" | head -3
