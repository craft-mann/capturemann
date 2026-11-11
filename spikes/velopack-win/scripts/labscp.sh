#!/bin/sh
# usage: labscp.sh <dest dir under C:/lab/velospike/ or empty> <files...>
. "$HOME/.config/capturemann-lab"
D=$1; shift
exec scp -i "${LAB_SSH_KEY:-$HOME/.ssh/id_ed25519}" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 -o BatchMode=yes -o LogLevel=ERROR -r "$@" "$LAB_WINDOWS:C:/lab/velospike/$D"
