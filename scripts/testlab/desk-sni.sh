#!/usr/bin/env bash
# Runs ON the lab laptop: starts cm-desktop privately, waits, and prints what the StatusNotifierItem exposes and what the
# shell's journal says about it. Usage: desk-sni.sh <target-dir>
set -u
T="$1"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
W="$(mktemp -d /tmp/cmd-sni.XXXXXX)"
export CAPTUREMANN_CONFIG_DIR="$W/config" CAPTUREMANN_LIBRARY="$W/library" CAPTUREMANN_SOCKET="$W/s.sock"
mkdir -p "$W/config" "$W/library"
"$T/debug/cm-desktop" > "$W/app.log" 2>&1 &
APP=$!
trap 'kill $APP 2>/dev/null; sleep 1; kill -9 $APP 2>/dev/null; rm -rf "$W"' EXIT
sleep 10
SVC="org.kde.StatusNotifierItem-$APP-1"
for p in Id Category Status IconName Menu ItemIsMenu; do
  echo "$p = $(gdbus call --session --dest "$SVC" --object-path /StatusNotifierItem --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierItem $p 2>&1 | cut -c1-120)"
done
echo "IconPixmap bytes: $(gdbus call --session --dest "$SVC" --object-path /StatusNotifierItem --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierItem IconPixmap 2>&1 | wc -c)"
echo "watcher items: $(gdbus call --session --dest org.kde.StatusNotifierWatcher --object-path /StatusNotifierWatcher --method org.freedesktop.DBus.Properties.Get org.kde.StatusNotifierWatcher RegisteredStatusNotifierItems 2>&1 | cut -c1-300)"
echo "--- journal (gnome-shell, last 3 min)"
journalctl --user -t gnome-shell --since "-3min" --no-pager 2>&1 | grep -i -E "appindicator|indicator|StatusNotifier|capturemann" | tail -15 | cut -c1-250
echo "--- app log"; tail -5 "$W/app.log"
