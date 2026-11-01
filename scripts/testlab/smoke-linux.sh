#!/usr/bin/env bash
# A quick, non-invasive look at an installed Linux build and the desktop it runs on, for the first minutes of a
# real-machine test (tests/manual/LINUX-1.md). It takes no screenshot and records nothing: no screen, camera or
# microphone is touched. It uses a throw-away library, config folder and socket under a temp folder.
#
#   scripts/testlab/smoke-linux.sh [--bin <folder with capturemann and cm-mcp>] [--out <report file>]
#
# Run it inside the desktop session you want to describe (a terminal on the laptop, or over ssh with
# XDG_RUNTIME_DIR, DBUS_SESSION_BUS_ADDRESS and WAYLAND_DISPLAY or DISPLAY set; lab.sh shows how). Run it once per
# session type (Xorg, Wayland). The report says PASS, FAIL or INFO per line; send the file back.
set -uo pipefail

bin=""; out=""
while [ $# -gt 0 ]; do
  case "$1" in
    --bin) bin="$2"; shift 2 ;;
    --out) out="$2"; shift 2 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

if [ -z "$bin" ]; then
  for d in /opt/capturemann/lib/capturemann "$HOME/.local/lib/capturemann" "$(dirname "$(command -v capturemann 2>/dev/null || echo /nonexistent/x)")"; do
    [ -x "$d/capturemann" ] && bin="$d" && break
  done
fi
[ -x "$bin/capturemann" ] || { echo "smoke-linux.sh: capturemann not found; use --bin <folder>" >&2; exit 2; }

stamp="$(date +%Y%m%d-%H%M%S)"
session="${XDG_SESSION_TYPE:-unknown}"
out="${out:-$HOME/lab/smoke-$session-$stamp.txt}"
mkdir -p "$(dirname "$out")"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

export CAPTUREMANN_LIBRARY="$work/library" CAPTUREMANN_CONFIG_DIR="$work/config" CAPTUREMANN_SOCKET="$work/daemon.sock"
export CAPTUREMANN_SECRETS="${CAPTUREMANN_SECRETS:-file}"
mkdir -p "$CAPTUREMANN_LIBRARY" "$CAPTUREMANN_CONFIG_DIR"

fails=0
say()  { printf '%s\n' "$*" | tee -a "$out"; }
sec()  { say ""; say "== $* =="; }
pass() { say "PASS  $*"; }
fail() { say "FAIL  $*"; fails=$((fails + 1)); }
info() { say "INFO  $*"; }
# run <label> <command...>: PASS when it exits 0; the first lines of output are kept.
run() {
  local label="$1"; shift
  local o; o="$(timeout 60 "$@" 2>&1)"; local rc=$?
  if [ $rc -eq 0 ]; then pass "$label"; else fail "$label (exit $rc)"; fi
  [ -z "$o" ] || printf '%s\n' "$o" | head -n "${LINES_KEPT:-12}" | sed 's/^/        /' | tee -a "$out" >/dev/null
}
have() { command -v "$1" >/dev/null 2>&1; }

: > "$out"
say "CaptureMann Linux smoke report, $(date -Is)"
say "bin: $bin"

sec "System"
info "$(. /etc/os-release 2>/dev/null; echo "${PRETTY_NAME:-unknown}") on $(uname -srm)"
info "session: type=$session desktop=${XDG_CURRENT_DESKTOP:-?} wayland=${WAYLAND_DISPLAY:-none} x11=${DISPLAY:-none}"
have lspci && info "gpu: $(lspci 2>/dev/null | grep -Ei 'vga|3d|display' | head -n 2 | tr '\n' ';')"
info "memory: $(awk '/MemTotal/ {printf "%.1f GB", $2/1048576}' /proc/meminfo)"
if have vainfo; then
  v="$(vainfo 2>&1)"
  if printf '%s' "$v" | grep -q 'VAEntrypointEncSlice'; then pass "VA-API has H.264 or other encode entry points"; else info "VA-API: no encode entry points (software VP9 recording only)"; fi
  printf '%s\n' "$v" | grep -E 'Driver version|VAProfile(H264|VP9|HEVC)' | head -n 8 | sed 's/^/        /' | tee -a "$out" >/dev/null
else info "vainfo not installed (sudo apt install vainfo)"; fi

sec "Desktop services"
if have busctl; then
  names="$(busctl --user list --no-pager 2>/dev/null | awk '{print $1}')"
  for n in org.freedesktop.portal.Desktop org.freedesktop.secrets org.kde.StatusNotifierWatcher org.freedesktop.Notifications; do
    if printf '%s\n' "$names" | grep -qx "$n"; then pass "bus name $n"; else info "bus name $n is missing"; fi
  done
else info "busctl not installed"; fi
for t in tesseract xdg-open wl-copy xclip xdotool pipewire; do
  if have "$t"; then info "$t: present"; else info "$t: not installed"; fi
done

sec "Programs"
for p in capturemann cm-mcp cm-daemon cm-desktop cm-overlay ffmpeg llama-server; do
  [ -x "$bin/$p" ] && pass "$p is there" || fail "$p is missing"
done
run "capturemann --version" "$bin/capturemann" --version
[ ! -x "$bin/check-runtime.sh" ] || true
if [ -x "$bin/../../check-runtime.sh" ]; then run "check-runtime.sh" "$bin/../../check-runtime.sh"; fi
if [ -x "$bin/ffmpeg" ]; then
  e="$("$bin/ffmpeg" -hide_banner -encoders 2>/dev/null)"
  printf '%s' "$e" | grep -q libvpx-vp9 && pass "bundled FFmpeg has the VP9 encoder" || fail "bundled FFmpeg has no VP9 encoder"
  printf '%s' "$e" | grep -q libx264 && fail "bundled FFmpeg has libx264 (GPL)" || pass "bundled FFmpeg has no libx264"
fi

sec "CLI against a throw-away library (no capture, no recording)"
cm="$bin/capturemann"
run "diagnostics" "$cm" diagnostics
LINES_KEPT=6 run "targets (displays and windows)" "$cm" targets
run "library list" "$cm" --json library list
run "settings get" "$cm" --json settings get
LINES_KEPT=6 run "ai models" "$cm" ai models

sec "MCP server over stdio"
if [ -x "$bin/cm-mcp" ]; then
  init='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"smoke","version":"1"}}}'
  note='{"jsonrpc":"2.0","method":"notifications/initialized"}'
  list='{"jsonrpc":"2.0","id":2,"method":"tools/list"}'
  r="$(printf '%s\n%s\n%s\n' "$init" "$note" "$list" | timeout 30 "$bin/cm-mcp" 2>/dev/null)"
  n="$(printf '%s' "$r" | grep -o '"name":"[a-z_]*"' | wc -l)"
  if printf '%s' "$r" | grep -q '"id":2' && [ "$n" -gt 5 ]; then pass "tools/list answered ($n names)"; else fail "tools/list did not answer"; fi
fi

sec "Stop the throw-away daemon"
"$cm" daemon stop >/dev/null 2>&1 && pass "daemon stopped" || info "no daemon to stop"

say ""
if [ "$fails" -eq 0 ]; then say "Result: no FAIL lines. Report: $out"; else say "Result: $fails FAIL line(s). Report: $out"; fi
exit $(( fails > 0 ))
