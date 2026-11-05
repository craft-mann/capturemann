#!/usr/bin/env bash
# Runs the release regression suite (cm-regress) on the lab laptop against an installed package and brings the
# report back into verification/. Runs on the developer's Mac. See docs/development/regression.md.
#
#   scripts/testlab/regress.sh windows [--tier smoke|standard|full|hardware] [--allow screen,sound,mic,camera,second_monitor,models]
#                                      [--run <Windows tests run id|latest>]     fetch cm-regress.exe (and the test programs) from that run
#                                      [--package-run <Windows package run id>]  download that package's installer and install it silently first
#                                      [--installed]                             test what is installed (the default)
#                                      [--only id,area,...] [--soak-minutes N] [--whisper-model <file on the laptop>]
#                                      [--label run2]   (added to the report file name so an earlier report is not overwritten)
#                                      [--installer <setup.exe on the laptop>] [--who NAME]   (the installer case replaces and restores the uninstall entry)
#
# The laptop is shared: C:\lab\lab.lock ("owner|epoch seconds") says who uses it. The script refuses while the lock is
# younger than LOCK_TTL_MIN minutes (default 45; FORCE=1 overrides) and removes its own lock when it ends.
# With `--allow sound` the volume is set to 0.3 unmuted for the run and put back to 0 muted afterwards (C:\lab\vol.ps1).
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

sys="${1:-}"; shift || true
[ "$sys" = windows ] || die "only 'windows' is implemented (macOS and Linux drivers come later); usage: regress.sh windows [options]"
tier=smoke; allow=""; run=""; pkgrun=""; only=""; soak=0; model=""; who="cm-regress via regress.sh"; installer=""; label=""
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
    --who) who="$2"; shift 2 ;;
    --installer) installer="$2"; shift 2 ;;
    --label) label="$2"; shift 2 ;;
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
power_saved=""
cleanup() {
  [ -n "$power_saved" ] && w "powercfg /change monitor-timeout-ac ${power_saved%%,*}; powercfg /change standby-timeout-ac ${power_saved##*,}" >/dev/null 2>&1 || true
  [ "$sound_on" = 1 ] && w 'C:\lab\vol.ps1 -Action set -Level 0 -Mute true' >/dev/null 2>&1 || true
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
[ -n "$installer" ] && args="$args --installer \"$installer\""
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
