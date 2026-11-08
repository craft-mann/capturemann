#!/usr/bin/env bash
# Runs ON the lab laptop (X11 or Wayland desktop session, lab lock taken by the caller): starts cm-desktop with
# its own private config/library/socket, then checks the Linux tray over D-Bus (dbusmenu): the delay check
# mark after a click, the signal that announced it, and "Recent captures" listing a capture the app did not take.
# Usage: desk-tray.sh <target-dir> <label>      (target-dir holds debug/cm-desktop and debug/capturemann)
set -u
T="${1:-$HOME/wt/desk-target}"; LABEL="${2:-tray}"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env 2>/dev/null; set +a
W="$(mktemp -d /tmp/cmd-tray.XXXXXX)"
trap 'kill $MON $APP 2>/dev/null; sleep 1; kill -9 $APP 2>/dev/null; rm -rf "$W"' EXIT
export CAPTUREMANN_CONFIG_DIR="$W/config" CAPTUREMANN_LIBRARY="$W/library" CAPTUREMANN_SOCKET="$W/s.sock"
mkdir -p "$W/config" "$W/library"
echo "== $LABEL: session ${XDG_SESSION_TYPE:-?} desktop ${XDG_CURRENT_DESKTOP:-?}"
"$T/debug/cm-desktop" > "$W/app.log" 2>&1 &
APP=$!
for i in $(seq 1 40); do
  gdbus call --session --dest org.freedesktop.DBus --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner "org.kde.StatusNotifierItem-$APP-1" 2>/dev/null | grep -q true && break
  sleep 0.5
done
SVC="org.kde.StatusNotifierItem-$APP-1"
layout() { gdbus call --session --dest "$SVC" --object-path /MenuBar --method com.canonical.dbusmenu.GetLayout 0 5 '@as []' 2>&1; }
delay_marks() { layout | grep -o "'toggle-state': <[0-9]>" | tr '\n' ' '; }
echo "tray registered: $(gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems 2>&1 | grep -c "$APP")"
echo "layout raw: $(layout | cut -c1-300)"
echo "delay marks at start: $(delay_marks)"
dbus-monitor --session "sender='$SVC'" > "$W/mon.txt" 2>&1 &
MON=$!
sleep 1
# Click "3 seconds": the item ids of the delay submenu start at 100.
for id in 101; do
  gdbus call --session --dest "$SVC" --object-path /MenuBar --method com.canonical.dbusmenu.Event "$id" clicked '<0>' 0 2>&1 | head -1
done
sleep 1
echo "delay marks after clicking item 101: $(delay_marks)"
echo "signals sent: $(grep -o 'member=[A-Za-z]*' "$W/mon.txt" | sort | uniq -c | tr '\n' ' ')"
echo "AboutToShow(0) answers: $(gdbus call --session --dest "$SVC" --object-path /MenuBar --method com.canonical.dbusmenu.AboutToShow 0 2>&1)"
# A capture the app did not take: the CLI against the app's own library.
"$T/debug/capturemann" targets > "$W/targets.txt" 2>&1
DID=$(awk '/^Displays/{getline; print $1; exit}' "$W/targets.txt")
echo "targets: $(head -c 400 $W/targets.txt | tr '\n' ' ')"
"$T/debug/capturemann" capture screenshot --display "${DID:-1}" > "$W/shot.json" 2>&1; echo "cli capture exit $?: $(head -c 300 $W/shot.json)"
for i in $(seq 1 14); do
  sleep 1
  n=$(layout | grep -c "recent:" )
  nl=$(layout | grep -o "'label': <'[^']*'>" | grep -c -v -E "region|window|display|record|Stop|Pause|library|launcher|Settings|Permissions|Quit|Recent|Capture|second|Off|Import" )
  [ "$(layout | grep -c 'No captures yet')" = 0 ] && break
done
echo "recent items listed after $i s: layout mentions 'No captures yet' = $(layout | grep -c 'No captures yet')"
layout | grep -o "'label': <'[^']*'>" | tail -12 | tr '\n' ' '; echo
kill $MON 2>/dev/null
kill "$APP" 2>/dev/null; sleep 1; kill -9 "$APP" 2>/dev/null
tail -5 "$W/app.log"
rm -rf "$W"
