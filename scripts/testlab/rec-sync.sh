#!/usr/bin/env bash
# Sync this worktree to the lab laptop (~/wt/rec) without target/ and VCS data.
set -euo pipefail
conf="$HOME/.config/capturemann-lab"; [ -f "$conf" ] && set -a && . "$conf" && set +a
key="${LAB_SSH_KEY:-$HOME/.ssh/id_ed25519}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
rsync -a --delete --exclude target --exclude '.git' --exclude 'packaging/*/out' \
  -e "ssh -i $key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR" \
  "$root/" "${LAB_LINUX:?}:wt/rec/"
echo synced
