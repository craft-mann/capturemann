#!/bin/sh
# usage: rps.sh <script name without .ps1> [args...]  -- copies ps/<name>.ps1 to C:\lab\velospike and runs it on the laptop
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad
. "$HOME/.config/capturemann-lab"
N=$1; shift
K="${LAB_SSH_KEY:-$HOME/.ssh/id_ed25519}"
O="-i $K -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -o BatchMode=yes -o LogLevel=ERROR"
scp $O "$S/ps/$N.ps1" "$LAB_WINDOWS:C:/lab/velospike/$N.ps1" || exit 1
exec ssh $O "$LAB_WINDOWS" "& 'C:\\lab\\velospike\\$N.ps1' $*"
