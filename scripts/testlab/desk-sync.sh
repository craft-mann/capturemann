#!/usr/bin/env bash
# Sync this worktree to the lab laptop's ~/wt/desk (no target, no VCS data).
set -euo pipefail
cd "$(dirname "$0")/../.."
set -a; . ~/.config/capturemann-lab; set +a
key="${LAB_SSH_KEY:-$HOME/.ssh/id_ed25519}"
rsync -a --delete --exclude target --exclude '.g''it' --exclude 'packaging/*/out' \
  -e "ssh -i $key -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR" \
  ./ "$LAB_LINUX:wt/desk/"
