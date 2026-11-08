#!/usr/bin/env bash
# Runs ON the lab laptop (X11 session, lab lock taken by the caller): a display capture with capture.delay_s=5, once forwarded by
# `cm-desktop --capture display` (a second launch) and once from the tray menu ("Capture display", dbusmenu item 3).
# Prints the seconds from the request to the new capture. Usage: desk-delay.sh <target-dir>
set -u
T="$1"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
W="$(mktemp -d /tmp/cmd-delay.XXXXXX)"
export CAPTUREMANN_CONFIG_DIR="$W/config" CAPTUREMANN_LIBRARY="$W/library" CAPTUREMANN_SOCKET="$W/s.sock"
mkdir -p "$W/config" "$W/library"
"$T/debug/cm-desktop" > "$W/app.log" 2>&1 &
APP=$!
trap 'kill $APP 2>/dev/null; sleep 1; kill -9 $APP 2>/dev/null; rm -rf "$W"' EXIT
sleep 6
"$T/debug/capturemann" settings set capture.delay_s 5 | head -2
count() { ls -d "$W"/library/*.cmshot 2>/dev/null | wc -l; }
wait_new() { # wait_new <before> -> seconds until the count grows (max 25)
  local before="$1" t0; t0=$(date +%s.%N)
  for i in $(seq 1 125); do
    [ "$(count)" -gt "$before" ] && { echo "capture after $(echo "$(date +%s.%N) - $t0" | bc) s"; return; }
    sleep 0.2
  done
  echo "NO capture within 25 s"
}
echo "--- forwarded: cm-desktop --capture display"
[ -n "${SKIP_FWD:-}" ] || { b=$(count); "$T/debug/cm-desktop" --capture display > "$W/fwd.log" 2>&1; wait_new "$b"; }
echo "--- tray menu: Capture display"
b=$(count)
gdbus call --session --dest "org.kde.StatusNotifierItem-$APP-1" --object-path /MenuBar --method com.canonical.dbusmenu.Event 3 clicked "<0>" 0 2>&1 | head -2
wait_new "$b"
echo "--- tray menu again (after 4 s)"; sleep 4
b=$(count)
gdbus call --session --dest "org.kde.StatusNotifierItem-$APP-1" --object-path /MenuBar --method com.canonical.dbusmenu.Event 3 clicked "<0>" 0 2>&1 | head -1
wait_new "$b"
grep -v "zbus::proxy" "$W/app.log" | cut -c1-900 | grep -i overlay | tail -3
