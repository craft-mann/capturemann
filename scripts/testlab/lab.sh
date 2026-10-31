#!/usr/bin/env bash
# Drives the dedicated test laptop (ThinkPad T490, dual boot Ubuntu + Windows) from the developer's Mac.
#
#   scripts/testlab/lab.sh status                      which OS answers on ssh right now
#   scripts/testlab/lab.sh linux  '<command>'          run a command on Ubuntu
#   scripts/testlab/lab.sh windows '<command>'         run a PowerShell command on Windows
#   scripts/testlab/lab.sh push linux|windows <file>…  copy files to the machine's ~/lab (Ubuntu) or C:\lab (Windows)
#   scripts/testlab/lab.sh pull linux|windows <remote path> [local dir]
#   scripts/testlab/lab.sh switch ubuntu|windows       reboot the machine into the other OS and wait for it
#   scripts/testlab/lab.sh wait linux|windows [secs]   wait until that OS answers on ssh
#
# Settings (environment, or a file ~/.config/capturemann-lab with KEY=value lines):
#   LAB_LINUX=user@cm-t490.local      LAB_WINDOWS=user@cm-t490.local      LAB_SSH_KEY=~/.ssh/id_ed25519
# Both OSes share one machine and usually one hostname; `status` tells them apart by what they answer.
# Hardware tests (screen, camera, microphone) run only when the owner asked for them.
set -euo pipefail
die() { echo "lab.sh: $*" >&2; exit 1; }

conf="$HOME/.config/capturemann-lab"
[ -f "$conf" ] && set -a && . "$conf" && set +a
LAB_SSH_KEY="${LAB_SSH_KEY:-$HOME/.ssh/id_ed25519}"
ssh_opts=(-i "$LAB_SSH_KEY" -o StrictHostKeyChecking=accept-new -o ConnectTimeout=5 -o BatchMode=yes -o LogLevel=ERROR)

target() { case "$1" in
  linux|ubuntu) echo "${LAB_LINUX:?set LAB_LINUX=user@host}" ;;
  windows|win) echo "${LAB_WINDOWS:?set LAB_WINDOWS=user@host}" ;;
  *) die "unknown system: $1 (linux|windows)" ;; esac; }

# Which OS is up: ask for uname (Ubuntu) or ver (Windows PowerShell has $PSVersionTable).
which_os() {
  local out
  out="$(ssh "${ssh_opts[@]}" "$(target linux)" 'uname -s' 2>/dev/null || true)"
  [ "$out" = Linux ] && { echo linux; return; }
  out="$(ssh "${ssh_opts[@]}" "$(target windows)" 'Write-Output $env:OS' 2>/dev/null || true)"
  case "$out" in *Windows*) echo windows; return ;; esac
  echo none
}

wait_for() { # wait_for linux|windows [secs]
  local want="$1" secs="${2:-300}" t=0
  while [ "$t" -lt "$secs" ]; do
    [ "$(which_os)" = "$want" ] && { echo "$want is up"; return 0; }
    sleep 5; t=$((t + 5))
  done
  die "$want did not answer within ${secs}s"
}

cmd="${1:-}"; shift || true
case "$cmd" in
  status) echo "up: $(which_os)" ;;
  linux|ubuntu) ssh "${ssh_opts[@]}" "$(target linux)" "$@" ;;
  windows|win) ssh "${ssh_opts[@]}" "$(target windows)" "$@" ;;
  push)
    sys="$1"; shift
    if [ "$sys" = linux ] || [ "$sys" = ubuntu ]; then
      ssh "${ssh_opts[@]}" "$(target linux)" 'mkdir -p ~/lab'
      rsync -a -e "ssh ${ssh_opts[*]}" "$@" "$(target linux):lab/"
    else
      ssh "${ssh_opts[@]}" "$(target windows)" 'New-Item -ItemType Directory -Force C:\lab | Out-Null'
      scp "${ssh_opts[@]}" -r "$@" "$(target windows):C:/lab/"
    fi ;;
  pull)
    sys="$1"; remote="$2"; local_dir="${3:-.}"
    if [ "$sys" = linux ] || [ "$sys" = ubuntu ]; then rsync -a -e "ssh ${ssh_opts[*]}" "$(target linux):$remote" "$local_dir/"
    else scp "${ssh_opts[@]}" -r "$(target windows):$remote" "$local_dir/"; fi ;;
  switch)
    to="${1:-}"; from="$(which_os)"
    case "$to" in
      windows|win)
        [ "$from" = linux ] || die "Ubuntu must be up to switch to Windows (it is: $from)"
        ssh "${ssh_opts[@]}" "$(target linux)" 'lab-next-boot windows --now' || true
        sleep 20; wait_for windows 420 ;;
      ubuntu|linux)
        [ "$from" = windows ] || die "Windows must be up to switch to Ubuntu (it is: $from)"
        ssh "${ssh_opts[@]}" "$(target windows)" 'powershell -File C:\lab\lab-next-boot.ps1 -Now' || true
        sleep 20; wait_for linux 420 ;;
      *) die "usage: switch ubuntu|windows" ;;
    esac ;;
  wait) wait_for "${1:?linux|windows}" "${2:-300}" ;;
  ""|-h|--help) sed -n '2,17p' "$0" ;;
  *) die "unknown command: $cmd" ;;
esac
