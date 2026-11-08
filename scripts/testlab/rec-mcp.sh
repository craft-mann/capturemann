#!/usr/bin/env bash
# Runs on the lab laptop: any cm-regress cases against the debug build (all four programs), on the desktop session.
#   rec-mcp.sh <case ids, comma separated> [--allow list]
. "$HOME/lab/rec-env.sh"
out="$HOME/rec-regress-out"
rm -rf "$out"
"$HOME/wt/rec-target/debug/cm-regress" --dev --package-dir "$HOME/wt/rec-target/debug" --tier "${TIER:-full}" \
  --allow "${ALLOW:-screen}" --only "$1" --out "$out" 2>&1 | tail -20
sed -n '/## Steps/,$p' "$out/report-${TIER:-full}.md" | cut -c1-600 | head -70
