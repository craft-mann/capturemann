#!/usr/bin/env bash
# The Linux branch of scripts/testlab/regress.sh (sourced by it; it uses die, lab, root and ttl from there).
# Runs the release regression suite (cm-regress) on the lab laptop (Ubuntu 24.04, X11 or GNOME Wayland) over ssh and
# brings the report back into verification/. See docs/development/regression.md.
#
#   scripts/testlab/regress.sh linux [--tier smoke|standard|full|hardware] [--allow screen,sound,mic,camera,second_monitor,models]
#                                    [--binary <cm-regress built for Linux x86_64, on this Mac>]  pushed to ~/lab/cm-regress (else the one already there)
#                                    [--package-dir <folder on the laptop>]  default: what is installed (~/.local/lib/capturemann, then /opt/capturemann/lib/capturemann)
#                                    [--installer <.deb or .tar.gz on this Mac>]  pushed first; runs linux.deb_install or linux.tarball_install (tier full)
#                                    [--session x11|wayland]  refuse to run unless the logged-in session is of that kind (switching is done at the login screen)
#                                    [--no-recording]  sets CM_LINUX_RECORDING=0 (recording is on by default since 0.3.0; the rec.* cases then SKIP); --recording is accepted and changes nothing
#                                    [--restore-token <screencast-token file on the laptop>]  CM_REGRESS_RESTORE_TOKEN: copied into each case's private config folder (desktops that ask for the screen once)
#                                    [--only ...] [--soak-minutes N] [--whisper-model F] [--speech F] [--speaker-models D] [--label X] [--commit SHA] [--who NAME]
#
# The lock is ~/lab/lab.lock on the laptop ("owner|epoch seconds"; LOCK_TTL_MIN and FORCE as for Windows). The desktop session's environment
# (DISPLAY, WAYLAND_DISPLAY, XAUTHORITY, DBUS_SESSION_BUS_ADDRESS, XDG_RUNTIME_DIR) is read from the running gnome-shell, so the ssh run sees the
# logged-in desktop. Saved and put back at the end: GNOME idle delay and AC sleep (set to never for the run) and, with `--allow sound`, the
# volume and mute state of the default output (wpctl; the default device itself is never changed). The display layout is only read.
# The report goes to verification/L1-linux-<version>-regression-<tier>-<session>-<date>-claude.md (+ .json). Needs the laptop up in Ubuntu with the
# test user logged in to the desktop (auto-login, scripts/testlab/linux-setup.sh). The suite never touches the real library: every case has its own.

# Globals (not `local`): the EXIT trap runs after run_linux has returned.
LX_ME=""; LX_SOUND_ON=0; LX_VOL_LEVEL=""; LX_VOL_MUTE=0; LX_POWER=""; LX_STAMP=""

lx() { "$lab" linux "$@"; }

cleanup_linux() {
  # Settings first, the lock last, so nobody takes the laptop while it is still changed.
  if [ -n "$LX_POWER" ]; then
    lx "set -a; . ~/lab/session.env; set +a; gsettings set org.gnome.desktop.session idle-delay ${LX_POWER%%,*}; gsettings set org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type '${LX_POWER##*,}'" >/dev/null 2>&1 || true
  fi
  if [ "$LX_SOUND_ON" = 1 ] && [ -n "$LX_VOL_LEVEL" ]; then
    lx "set -a; . ~/lab/session.env; set +a; wpctl set-volume @DEFAULT_AUDIO_SINK@ $LX_VOL_LEVEL; wpctl set-mute @DEFAULT_AUDIO_SINK@ $LX_VOL_MUTE" >/dev/null 2>&1 || true
  fi
  if [ -n "$LX_STAMP" ]; then
    lx "rm -rf ~/lab/regress-out/$LX_STAMP ~/lab/regress-out/$LX_STAMP.console.txt; pkill -x cm-regress || true" >/dev/null 2>&1 || true
  fi
  lx "rm -f ~/lab/session.env; if grep -q '^$LX_ME|' ~/lab/lab.lock 2>/dev/null; then rm -f ~/lab/lab.lock; fi" >/dev/null 2>&1 || true
}

run_linux() {
  local tier=smoke allow="" only="" pkgdir="" installer="" binary="" recording=1 who="cm-regress via regress.sh" label="" commit=""
  local want_session="" soak=0 model="" speech="" spkdir="" token=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --tier) tier="$2"; shift 2 ;;
      --allow) allow="$2"; shift 2 ;;
      --only) only="$2"; shift 2 ;;
      --package-dir) pkgdir="$2"; shift 2 ;;
      --installer) installer="$2"; shift 2 ;;
      --binary) binary="$2"; shift 2 ;;
      --session) want_session="$2"; shift 2 ;;
      --recording) shift ;;
      --no-recording) recording=0; shift ;;
      --restore-token) token="$2"; shift 2 ;;
      --soak-minutes) soak="$2"; shift 2 ;;
      --whisper-model) model="$2"; shift 2 ;;
      --speech) speech="$2"; shift 2 ;;
      --speaker-models) spkdir="$2"; shift 2 ;;
      --who) who="$2"; shift 2 ;;
      --label) label="$2"; shift 2 ;;
      --commit) commit="$2"; shift 2 ;;
      *) die "unknown option $1" ;;
    esac
  done
  case "$tier" in smoke|standard|full|hardware) ;; *) die "unknown tier $tier" ;; esac
  case "$want_session" in ""|x11|wayland) ;; *) die "--session is x11 or wayland" ;; esac
  command -v python3 >/dev/null || die "python3 is needed"
  [ "$("$lab" status)" = "up: linux" ] || die "the laptop is not up in Ubuntu (scripts/testlab/lab.sh status)"

  # --- the lock ---
  local now cur at owner
  LX_ME="${USER:-agent}-regress-$$"
  now="$(date +%s)"
  cur="$(lx 'cat ~/lab/lab.lock 2>/dev/null || true' | tr -d '\r\n')"
  if [ -n "$cur" ] && [ "${FORCE:-0}" != 1 ]; then
    at="${cur##*|}"; owner="${cur%%|*}"
    if [[ "$at" =~ ^[0-9]+$ ]] && [ $((now - at)) -lt $((ttl * 60)) ]; then
      die "the laptop is in use by '$owner' ($(( (now - at) / 60 )) min ago); wait, or FORCE=1 if you know it is stale"
    fi
  fi
  lx "mkdir -p ~/lab && echo '$LX_ME|$now' > ~/lab/lab.lock" >/dev/null
  trap cleanup_linux EXIT

  # --- the programs under test ---
  if [ -n "$binary" ]; then
    [ -f "$binary" ] || die "no such file: $binary"
    "$lab" push linux "$binary" >/dev/null
    lx "[ \"$(basename "$binary")\" = cm-regress ] || mv -f ~/lab/$(basename "$binary") ~/lab/cm-regress; chmod 755 ~/lab/cm-regress"
  fi
  lx 'test -x ~/lab/cm-regress && ls -l ~/lab/cm-regress | awk "{print \$5, \"bytes\"}"' \
    || die "~/lab/cm-regress is missing: build it on Linux (cargo build --release -p cm-regress) and pass --binary"
  local inst_remote=""
  if [ -n "$installer" ]; then
    [ -f "$installer" ] || die "no such file: $installer"
    "$lab" push linux "$installer" >/dev/null
    inst_remote="\$HOME/lab/$(basename "$installer")"
  fi

  # --- the desktop session: its environment comes from the running shell ---
  local sess
  sess="$(lx bash -s <<'EOS' | tr -d '\r'
uid=$(id -u)
pid=$(pgrep -u "$uid" -x gnome-shell | head -1)
[ -n "$pid" ] || { echo "SESSION=none"; exit 0; }
mkdir -p ~/lab
: > ~/lab/session.env
for v in DISPLAY WAYLAND_DISPLAY XAUTHORITY DBUS_SESSION_BUS_ADDRESS XDG_RUNTIME_DIR XDG_CURRENT_DESKTOP; do
  val=$(tr '\0' '\n' < "/proc/$pid/environ" | sed -n "s/^$v=//p" | head -1)
  [ -n "$val" ] && printf '%s=%q\n' "$v" "$val" >> ~/lab/session.env
done
# A Wayland session started by gdm: gnome-shell itself has no WAYLAND_DISPLAY, DISPLAY or XAUTHORITY in its environment (it creates
# them for its children); the user manager holds them (found with `systemctl --user show-environment`, Nov 9, 2026).
for v in DISPLAY WAYLAND_DISPLAY XAUTHORITY; do
  grep -q "^$v=" ~/lab/session.env || {
    val=$(systemctl --user show-environment 2>/dev/null | sed -n "s/^$v=//p" | head -1)
    [ -n "$val" ] && printf '%s=%q\n' "$v" "$val" >> ~/lab/session.env
  }
done
sid=$(loginctl show-user "$uid" -p Display --value 2>/dev/null)
type=$(loginctl show-session "$sid" -p Type --value 2>/dev/null)
echo "XDG_SESSION_TYPE=${type:-unknown}" >> ~/lab/session.env
echo "SESSION=${type:-unknown}"
EOS
)"
  sess="${sess##*SESSION=}"
  case "$sess" in
    none|"") die "no desktop session on the laptop (gnome-shell does not run for the test user); log in at the laptop or check auto-login" ;;
  esac
  echo "desktop session: $sess"
  if [ "$sess" = wayland ]; then
    echo "Wayland run: no X11 tool drives the desktop (xrandr only lists monitors through XWayland); the window cases (shot.window, redact.window_text, desktop.recording_banner, linux.desktop_stays_up's window check) report SKIP with their reason"
  fi
  if [ -n "$want_session" ] && [ "$want_session" != "$sess" ]; then
    die "the logged-in session is '$sess', wanted '$want_session'. Choose it at the login screen (gear icon) on the laptop; this script never changes the display or session settings."
  fi

  # --- power settings: the screen must not blank and the machine must not sleep during a run ---
  LX_POWER="$(lx bash -s <<'EOS' | tr -d '\r\n '
set -a; . ~/lab/session.env; set +a
idle=$(gsettings get org.gnome.desktop.session idle-delay 2>/dev/null | awk '{print $NF}')
slp=$(gsettings get org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type 2>/dev/null | tr -d "'")
echo "$idle,$slp"
EOS
)" || true
  case "$LX_POWER" in [0-9]*,[a-z-]*) ;; *) LX_POWER="" ;; esac
  echo "power settings before (idle-delay s, AC sleep): ${LX_POWER:-unknown}"
  if [ -n "$LX_POWER" ]; then
    lx "set -a; . ~/lab/session.env; set +a; gsettings set org.gnome.desktop.session idle-delay 0; gsettings set org.gnome.settings-daemon.plugins.power sleep-inactive-ac-type 'nothing'" >/dev/null || true
  fi

  # --- sound: the default output's volume and mute state, put back at the end ---
  if [[ ",$allow," == *,sound,* ]]; then
    local vr
    vr="$(lx 'set -a; . ~/lab/session.env; set +a; wpctl get-volume @DEFAULT_AUDIO_SINK@' | tr -d '\r' | head -1 || true)"
    if [[ "$vr" =~ ^Volume:\ ([0-9.]+)(.*)$ ]]; then
      LX_VOL_LEVEL="${BASH_REMATCH[1]}"
      if [[ "${BASH_REMATCH[2]}" == *MUTED* ]]; then LX_VOL_MUTE=1; else LX_VOL_MUTE=0; fi
      echo "volume before the run: $LX_VOL_LEVEL (muted: $LX_VOL_MUTE)"
      LX_SOUND_ON=1
      lx 'set -a; . ~/lab/session.env; set +a; wpctl set-volume @DEFAULT_AUDIO_SINK@ 0.3; wpctl set-mute @DEFAULT_AUDIO_SINK@ 0'
    else
      echo "regress.sh: could not read the volume ('$vr'); sound is not allowed for this run and the volume is not touched" >&2
      allow="$(echo ",$allow," | sed 's/,sound,/,/; s/^,//; s/,$//')"
    fi
  fi

  # --- monitors: only read, never changed ---
  local want_displays="${WANT_DISPLAYS:-1}" n="" i
  for i in $(seq 1 13); do
    n="$(lx 'set -a; . ~/lab/session.env; set +a; xrandr --listmonitors | head -1' | tr -d '\r' | awk '{print $2}')"
    [ "$n" -ge "$want_displays" ] 2>/dev/null && break
    echo "only ${n:-?} monitor(s) (want $want_displays); waiting 10 s ($i/13)"; sleep 10
  done
  echo "monitors at the start: ${n:-?}"

  # --- the run ---
  LX_STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
  local args="--tier $tier --out \$HOME/lab/regress-out/$LX_STAMP --who '$who' --verdict PARTIAL"
  [ -n "$allow" ] && args="$args --allow $allow"
  [ -n "$only" ] && args="$args --only $only"
  [ "$soak" != 0 ] && args="$args --soak-minutes $soak"
  [ -n "$model" ] && args="$args --whisper-model '$model'"
  [ -n "$speech" ] && args="$args --speech '$speech'"
  [ -n "$spkdir" ] && args="$args --speaker-models '$spkdir'"
  [ -n "$pkgdir" ] && args="$args --package-dir '$pkgdir'"
  [ -n "$inst_remote" ] && args="$args --installer $inst_remote"
  [ -n "$commit" ] && args="$args --commit $commit"
  local limit rec_line=""
  case "$tier" in smoke) limit=600 ;; standard) limit=2400 ;; full) limit=5400 ;; hardware) limit=$((7200 + soak * 60)) ;; esac
  [ "$recording" = 0 ] && rec_line="export CM_LINUX_RECORDING=0"
  if [ -n "$token" ]; then rec_line="$rec_line"$'\n'"export CM_REGRESS_RESTORE_TOKEN='$token'"; fi
  echo "running: cm-regress $args (limit $limit s; recording $([ "$recording" = 0 ] && echo off || echo on))"
  local code
  set +e
  lx bash -s <<EOS
set -a; . ~/lab/session.env; set +a
$rec_line
mkdir -p ~/lab/regress-out
cd ~/lab
timeout -k 20 $limit ./cm-regress $args > ~/lab/regress-out/$LX_STAMP.console.txt 2>&1
exit \$?
EOS
  code=$?
  set -e
  echo "cm-regress exit code: $code (0 pass, 1 a case failed, 124 timed out)"

  # --- bring the report back ---
  local tmp="$root/.testlab-downloads/regress-$LX_STAMP"
  mkdir -p "$tmp"
  "$lab" pull linux "lab/regress-out/$LX_STAMP/results.json" "$tmp" || true
  "$lab" pull linux "lab/regress-out/$LX_STAMP/report-$tier.md" "$tmp" || true
  "$lab" pull linux "lab/regress-out/$LX_STAMP.console.txt" "$tmp" || true
  [ -f "$tmp/results.json" ] || die "no results.json came back; see $tmp and ~/lab/regress-out on the laptop"
  local version name
  version="$(python3 -c "import json; print(json.load(open('$tmp/results.json'))['run']['version'] or 'unknown')")"
  version="${version// /_}"
  name="L1-linux-$version-regression-$tier-$sess${label:+-$label}-$(date -u +%Y-%m-%d)-claude"
  cp "$tmp/report-$tier.md" "$root/verification/$name.md"
  cp "$tmp/results.json" "$root/verification/$name.json"
  python3 - "$tmp/results.json" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
s = d["summary"]
print(f"{s['pass']} passed, {s['fail']} failed, {s['skip']} skipped of {s['total']}")
for c in d["cases"]:
    if c["status"] == "FAIL":
        print("FAIL", c["id"], "-", c["message"][:200])
PY
  echo "report: verification/$name.md"
  return "$code"
}
