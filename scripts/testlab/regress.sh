#!/usr/bin/env bash
# Runs the release regression suite (cm-regress) on the lab laptop against an installed package and brings the
# report back into verification/. Runs on the developer's Mac. See docs/development/regression.md.
#
#   scripts/testlab/regress.sh windows [--tier smoke|standard|full|hardware] [--allow screen,sound,mic,camera,second_monitor,models]
#                                      [--run <Windows tests run id|latest>]     fetch cm-regress.exe (and the test programs) from that run
#                                      [--package-run <Windows package run id>]  download that package's installer and install it silently first
#                                      [--installed]                             test what is installed (the default)
#                                      [--only id,area,...] [--soak-minutes N] [--whisper-model <file on the laptop>]
#                                      [--speech <speech wav on the laptop>] [--speaker-models <folder with the two .onnx files on the laptop>]
#                                        (ai.transcribe / ai.speakers; win-make-speech.ps1 makes speech2.wav; the speaker models are in
#                                         packaging/sherpa/out/models on a machine that ran `make sherpa`/fetch-models.sh; push them with lab.sh)
#                                      [--commit <sha of the package, written in the report header>]
#                                      [--label run2]   (added to the report file name so an earlier report is not overwritten)
#                                      [--installer <setup.exe on the laptop>] [--who NAME]   (the installer case replaces and restores the uninstall entry)
#
# The laptop is shared: C:\lab\lab.lock ("owner|epoch seconds") says who uses it. The script refuses while the lock is
# younger than LOCK_TTL_MIN minutes (default 45; FORCE=1 overrides) and removes its own lock when it ends.
# With `--allow sound` the volume is set to 0.3 unmuted for the run and put back to the level and mute state it had before (read first with `vol.ps1 -Action get`; 0 muted only when that
# could not be read) (C:\lab\vol.ps1).
# The report is written to verification/L1-windows-<version>-regression-<tier>-<date>-claude.md (+ .json beside it).
# Needs: gh (for --run / --package-run), python3, the lab settings of lab.sh. The laptop must be up in Windows and a user
# must be logged in (the tests run in that session through win-run-in-session.ps1).
set -euo pipefail
die() { echo "regress.sh: $*" >&2; exit 1; }
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
lab="$here/lab.sh"
repo="${REPO:-craft-mann/capturemann}"
ttl="${LOCK_TTL_MIN:-45}"

# --- macOS: runs on this Mac (the maintainer's or a second one), no lab laptop ---
#   scripts/testlab/regress.sh macos [--tier smoke|standard|full|hardware] [--allow screen,sound,mic,camera,second_monitor,models]
#        [--package-dir <folder with the programs>]   default: target/bundle/CaptureMann-dev.app/Contents/MacOS (make bundle-dev)
#        [--daemon <cm-daemon>]  default: target/release/cm-daemon (built here when missing); the app has no cm-daemon of its own
#        [--installer <.dmg or .app>] [--only ...] [--soak-minutes N] [--whisper-model F] [--speech F] [--speaker-models D]
#        [--commit SHA] [--label L] [--who NAME] [--keep]
# Never touches /Applications (a package dir there is refused), the display settings or the output device; with
# `--allow sound` only the output volume level and mute state are changed and put back. The Mac is kept awake with
# caffeinate for the run. The lock is ${TMPDIR:-/tmp}/cm-regress-mac.lock ("owner|epoch seconds", LOCK_TTL_MIN, FORCE=1).
# The permissions (Screen Recording, Microphone, Camera, Accessibility for the key and menu-bar probes) belong to
# the program that runs this script (the Terminal) and the dev bundle; hardware runs only when the maintainer asks.
regress_macos() {
  local tier=smoke allow="" only="" soak=0 model="" speech="" spkdir="" who="cm-regress via regress.sh" installer="" label="" commit="" pkg="" daemon="" keep=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --tier) tier="$2"; shift 2 ;;
      --allow) allow="$2"; shift 2 ;;
      --package-dir) pkg="$2"; shift 2 ;;
      --daemon) daemon="$2"; shift 2 ;;
      --installer) installer="$2"; shift 2 ;;
      --only) only="$2"; shift 2 ;;
      --soak-minutes) soak="$2"; shift 2 ;;
      --whisper-model) model="$2"; shift 2 ;;
      --speech) speech="$2"; shift 2 ;;
      --speaker-models) spkdir="$2"; shift 2 ;;
      --who) who="$2"; shift 2 ;;
      --label) label="$2"; shift 2 ;;
      --commit) commit="$2"; shift 2 ;;
      --keep) keep=1; shift ;;
      *) die "unknown option $1" ;;
    esac
  done
  case "$tier" in smoke|standard|full|hardware) ;; *) die "unknown tier $tier" ;; esac
  [ "$(uname -s)" = Darwin ] || die "regress.sh macos must run on a Mac"
  command -v python3 >/dev/null || die "python3 is needed"
  [ -n "$pkg" ] || pkg="$root/target/bundle/CaptureMann-dev.app/Contents/MacOS"
  [ -d "$pkg" ] || die "no package folder $pkg (make bundle-dev, or --package-dir)"
  pkg="$(cd "$pkg" && pwd -P)"
  case "$pkg" in /Applications|/Applications/*) die "refusing to test /Applications: use the dev bundle or a copy" ;; esac
  [ -n "$daemon" ] || daemon="$root/target/release/cm-daemon"
  if [ ! -x "$daemon" ]; then
    echo "building cm-daemon (release) for the headless cases"
    (cd "$root" && cargo build --release --locked -p cm-daemon) || die "cm-daemon did not build"
  fi
  local reg="$root/target/release/cm-regress"
  (cd "$root" && cargo build --release --locked -p cm-regress) || die "cm-regress did not build"

  local lock="${TMPDIR:-/tmp}/cm-regress-mac.lock" me now cur at owner
  me="${USER:-agent}-regress-$$"; now="$(date +%s)"
  if [ -f "$lock" ] && [ "${FORCE:-0}" != 1 ]; then
    cur="$(cat "$lock")"; at="${cur##*|}"; owner="${cur%%|*}"
    if [[ "$at" =~ ^[0-9]+$ ]] && [ $((now - at)) -lt $((ttl * 60)) ]; then
      die "this Mac is in use by '$owner' ($(( (now - at) / 60 )) min ago); wait, or FORCE=1 if you know it is stale"
    fi
  fi
  echo "$me|$now" > "$lock"
  local sound_on=0 vol_level=0 vol_mute=true caff=""
  mac_cleanup() {
    [ "$sound_on" = 1 ] && osascript -e "set volume output volume $vol_level" -e "set volume output muted $vol_mute" >/dev/null 2>&1 || true
    [ -n "$caff" ] && kill "$caff" >/dev/null 2>&1 || true
    [ "$(cat "$lock" 2>/dev/null)" = "$me|$now" ] && rm -f "$lock" || true
  }
  trap mac_cleanup EXIT
  if [[ ",$allow," == *,sound,* ]]; then
    # Read what the Mac has now so that the end of the run puts back exactly that (0 muted only when unreadable).
    local lv mu
    lv="$(osascript -e 'output volume of (get volume settings)' 2>/dev/null || true)"
    mu="$(osascript -e 'output muted of (get volume settings)' 2>/dev/null || true)"
    if [[ "$lv" =~ ^[0-9]+$ ]] && [[ "$mu" =~ ^(true|false)$ ]]; then
      vol_level="$lv"; vol_mute="$mu"
    else
      echo "regress.sh: could not read the volume ('$lv' '$mu'); it will be set to 0 muted at the end" >&2
    fi
    echo "volume before the run: $vol_level (muted: $vol_mute)"
    sound_on=1
    osascript -e 'set volume output volume 30' -e 'set volume output muted false'
  fi
  caffeinate -dims -w $$ & caff=$!   # display, idle and system sleep off until this script ends

  local stamp out limit code version name
  local -a args
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  out="${TMPDIR:-/tmp}/cm-regress-mac-$stamp"; mkdir -p "$out"
  args=(--tier "$tier" --package-dir "$pkg" --daemon "$daemon" --out "$out" --who "$who" --verdict PARTIAL)
  [ -n "$allow" ] && args+=(--allow "$allow")
  [ -n "$only" ] && args+=(--only "$only")
  [ "$soak" != 0 ] && args+=(--soak-minutes "$soak")
  [ -n "$model" ] && args+=(--whisper-model "$model")
  [ -n "$speech" ] && args+=(--speech "$speech")
  [ -n "$spkdir" ] && args+=(--speaker-models "$spkdir")
  [ -n "$installer" ] && args+=(--installer "$installer")
  [ -n "$commit" ] && args+=(--commit "$commit")
  [ -n "$keep" ] && args+=(--keep)
  case "$tier" in smoke) limit=600 ;; standard) limit=2400 ;; full) limit=5400 ;; hardware) limit=$((7200 + soak * 60)) ;; esac
  echo "running: cm-regress ${args[*]} (limit $limit s)"
  set +e
  python3 -c 'import subprocess, sys
try:
    sys.exit(subprocess.run(sys.argv[2:], timeout=int(sys.argv[1])).returncode)
except subprocess.TimeoutExpired:
    sys.exit(124)' "$limit" "$reg" "${args[@]}"
  code=$?
  set -e
  echo "cm-regress exit code: $code (0 pass, 1 a case failed, 124 timed out)"
  [ -f "$out/results.json" ] || die "no results.json came back; see $out"
  version="$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['run']['version'] or 'unknown')" "$out/results.json")"
  version="${version// /_}"
  name="L1-macos-$version-regression-$tier${label:+-$label}-$(date -u +%Y-%m-%d)-claude"
  cp "$out/report-$tier.md" "$root/verification/$name.md"
  cp "$out/results.json" "$root/verification/$name.json"
  python3 -c "import json,sys
d = json.load(open(sys.argv[1])); s = d['summary']
print(f\"{s['pass']} passed, {s['fail']} failed, {s['skip']} skipped of {s['total']}\")
[print('FAIL', c['id'], '-', c['message'][:200]) for c in d['cases'] if c['status'] == 'FAIL']" "$out/results.json"
  echo "report: verification/$name.md"
  [ -n "$keep" ] || rm -rf "$out"
  return "$code"
}

sys="${1:-}"; shift || true
if [ "$sys" = linux ]; then . "$here/regress-linux.sh"; code=0; run_linux "$@" || code=$?; exit "$code"; fi
if [ "$sys" = macos ]; then
  regress_macos "$@"
  exit $?
fi
[ "$sys" = windows ] || die "only 'windows', 'macos' and 'linux' are implemented; usage: regress.sh windows|macos|linux [options]"
tier=smoke; allow=""; run=""; pkgrun=""; only=""; soak=0; model=""; speech=""; spkdir=""; who="cm-regress via regress.sh"; installer=""; label=""; commit=""
while [ $# -gt 0 ]; do
  case "$1" in
    --tier) tier="$2"; shift 2 ;;
    --allow) allow="$2"; shift 2 ;;
    --run) run="$2"; shift 2 ;;
    --package-run) pkgrun="$2"; shift 2 ;;
    --installed) shift ;;
    --only) only="$2"; shift 2 ;;
    --soak-minutes) soak="$2"; shift 2 ;;
    --whisper-model) model="$2"; shift 2 ;;
    --speech) speech="$2"; shift 2 ;;
    --speaker-models) spkdir="$2"; shift 2 ;;
    --who) who="$2"; shift 2 ;;
    --installer) installer="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
    --commit) commit="$2"; shift 2 ;;
    *) die "unknown option $1" ;;
  esac
done
case "$tier" in smoke|standard|full|hardware) ;; *) die "unknown tier $tier" ;; esac
w() { "$lab" windows "$@"; }
command -v python3 >/dev/null || die "python3 is needed"
"$lab" status | grep -q 'up: windows' || die "the laptop is not up in Windows (scripts/testlab/lab.sh status)"

# --- the lock ---
me="${USER:-agent}-regress-$$"
now="$(date +%s)"
cur="$(w 'if (Test-Path C:\lab\lab.lock) { Get-Content C:\lab\lab.lock -Raw }' | tr -d '\r\n' || true)"
if [ -n "$cur" ] && [ "${FORCE:-0}" != 1 ]; then
  at="${cur##*|}"; owner="${cur%%|*}"
  if [[ "$at" =~ ^[0-9]+$ ]] && [ $((now - at)) -lt $((ttl * 60)) ]; then
    die "the laptop is in use by '$owner' ($(( (now - at) / 60 )) min ago); wait, or FORCE=1 if you know it is stale"
  fi
fi
w "Set-Content C:\\lab\\lab.lock '$me|$now' -Encoding ascii" >/dev/null
sound_on=0
vol_level=0
vol_mute=true
power_saved=""
cleanup() {
  [ -n "$power_saved" ] && w "powercfg /change monitor-timeout-ac ${power_saved%%,*}; powercfg /change standby-timeout-ac ${power_saved##*,}" >/dev/null 2>&1 || true
  [ "$sound_on" = 1 ] && w "C:\\lab\\vol.ps1 -Action set -Level $vol_level -Mute $vol_mute" >/dev/null 2>&1 || true
  w "if ((Get-Content C:\\lab\\lab.lock -ErrorAction SilentlyContinue) -like '$me|*') { Remove-Item C:\\lab\\lab.lock -Force }" >/dev/null 2>&1 || true
}
trap cleanup EXIT

# --- the programs ---
if [ -n "$run" ]; then
  "$here/win-run-tests.sh" "$run" | sed 's/^/[tests] /'
fi
w 'if (-not (Test-Path C:\lab\tests\cm-regress.exe)) { Write-Error "C:\lab\tests\cm-regress.exe is missing: run with --run <Windows tests run id>"; exit 3 }; Get-Item C:\lab\tests\cm-regress.exe | ForEach-Object { "$($_.Length) bytes" }'
"$lab" push windows "$here/win-run-in-session.ps1" >/dev/null

if [ -n "$pkgrun" ]; then
  d="$root/.testlab-downloads/pkg-$pkgrun"; rm -rf "$d"; mkdir -p "$d"
  gh run download "$pkgrun" --repo "$repo" --name capturemann-windows-x64 --dir "$d"
  setup="$(find "$d" -name "*-setup.exe" | head -1)"; [ -f "$setup" ] || die "no *-setup.exe in the package artifact"
  want="$(cut -d' ' -f1 "$setup.sha256")"; have="$(shasum -a 256 "$setup" | cut -d' ' -f1)"
  [ "$want" = "$have" ] || die "installer SHA-256 mismatch"
  "$lab" push windows "$setup"
  w "Get-Process cm-* -ErrorAction SilentlyContinue | Stop-Process -Force; Start-Process -Wait -FilePath C:\\lab\\$(basename "$setup") -ArgumentList '/S'; Remove-Item C:\\lab\\$(basename "$setup")"
fi

# --- sound ---
if [[ ",$allow," == *,sound,* ]]; then
  # Read what the laptop has now ("volume=0.62 mute=False") so that the end of the run restores exactly that.
  vol_read="$(w 'C:\lab\vol.ps1 -Action get' | tr -d '\r' | grep -o 'volume=[0-9.]* mute=[A-Za-z]*' | head -1 || true)"
  if [[ "$vol_read" =~ ^volume=([0-9.]+)\ mute=([A-Za-z]+)$ ]]; then
    vol_level="${BASH_REMATCH[1]}"; vol_mute="$(echo "${BASH_REMATCH[2]}" | tr 'A-Z' 'a-z')"
  else
    echo "regress.sh: could not read the volume ('$vol_read'); it will be set to 0 muted at the end" >&2
  fi
  echo "volume before the run: $vol_level (muted: $vol_mute)"
  sound_on=1
  w 'C:\lab\vol.ps1 -Action set -Level 0.3 -Mute false'
fi

# --- the display layout and power settings (the external monitor dropped out once during a run) ---
# Monitor and system sleep are set to never for the run and put back afterwards (values in minutes).
power_saved="$(w '$q = { param($sub,$set) $t = powercfg /query SCHEME_CURRENT $sub $set | Out-String; if ($t -match "Current AC Power Setting Index:\s+0x([0-9a-fA-F]+)") { [int][Convert]::ToInt32($Matches[1],16) / 60 } else { 0 } }; "$(& $q SUB_VIDEO VIDEOIDLE),$(& $q SUB_SLEEP STANDBYIDLE)"' | tr -d '\r\n ' || true)"
case "$power_saved" in [0-9]*,[0-9]*) ;; *) power_saved="" ;; esac
echo "power settings before (monitor,standby minutes): ${power_saved:-unknown}"
[ -n "$power_saved" ] && w 'powercfg /change monitor-timeout-ac 0; powercfg /change standby-timeout-ac 0' >/dev/null || true
want_displays="${WANT_DISPLAYS:-2}"
for i in $(seq 1 13); do
  n="$(w '(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBasicDisplayParams | Where-Object Active | Measure-Object).Count' | tr -d '\r\n ')"
  [ "$n" -ge "$want_displays" ] 2>/dev/null && break
  echo "only ${n:-?} active monitor(s) (want $want_displays); waiting 10 s ($i/13)"; sleep 10
done
echo "active monitors at the start: ${n:-?}"
# --- the run ---
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
out="C:\\lab\\regress-out\\$stamp"
args="--tier $tier --out $out --who \"$who\" --verdict PARTIAL"
[ -n "$allow" ] && args="$args --allow $allow"
[ -n "$only" ] && args="$args --only $only"
[ "$soak" != 0 ] && args="$args --soak-minutes $soak"
[ -n "$model" ] && args="$args --whisper-model \"$model\""
[ -n "$speech" ] && args="$args --speech \"$speech\""
[ -n "$spkdir" ] && args="$args --speaker-models \"$spkdir\""
[ -n "$installer" ] && args="$args --installer \"$installer\""
[ -n "$commit" ] && args="$args --commit $commit"
case "$tier" in smoke) limit=600 ;; standard) limit=2400 ;; full) limit=5400 ;; hardware) limit=$((7200 + soak * 60)) ;; esac
echo "running: cm-regress.exe $args (limit $limit s)"
set +e
w "C:\\lab\\win-run-in-session.ps1 -Exe C:\\lab\\tests\\cm-regress.exe -Arguments '$args' -OutFile C:\\lab\\regress-out\\$stamp.console.txt -TimeoutSec $limit -WorkDir C:\\lab\\tests"
code=$?
set -e
n2="$(w '(Get-CimInstance -Namespace root\wmi -ClassName WmiMonitorBasicDisplayParams | Where-Object Active | Measure-Object).Count' | tr -d '\r\n ')"
echo "active monitors at the end: ${n2:-?} (at the start: ${n:-?})"
echo "cm-regress exit code: $code (0 pass, 1 a case failed, 124 timed out)"

# --- bring the report back ---
tmp="$root/.testlab-downloads/regress-$stamp"; mkdir -p "$tmp"
"$lab" pull windows "C:/lab/regress-out/$stamp/results.json" "$tmp" || true
"$lab" pull windows "C:/lab/regress-out/$stamp/report-$tier.md" "$tmp" || true
"$lab" pull windows "C:/lab/regress-out/$stamp.console.txt" "$tmp" || true
[ -f "$tmp/results.json" ] || die "no results.json came back; see $tmp and C:\\lab\\regress-out on the laptop"
version="$(python3 -c "import json,sys; print(json.load(open('$tmp/results.json'))['run']['version'] or 'unknown')")"
version="${version// /_}"
name="L1-windows-$version-regression-$tier${label:+-$label}-$(date -u +%Y-%m-%d)-claude"
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

# --- leave the laptop clean (the report is a copy; the run's files go) ---
w "Remove-Item -Recurse -Force C:\\lab\\regress-out\\$stamp, C:\\lab\\regress-out\\$stamp.console.txt -ErrorAction SilentlyContinue; Get-Process cm-regress -ErrorAction SilentlyContinue | Stop-Process -Force" >/dev/null || true
exit "$code"
