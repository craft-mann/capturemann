#!/usr/bin/env bash
# Runs on the lab laptop (lock held by the caller): the rec.* cases of cm-regress against the release build,
# silent (screen and microphone only).   rec-regress.sh [--only ids]
. "$HOME/lab/rec-env.sh"
only="${1:-rec.screen_cursor,rec.region,rec.region_min,rec.kill9_recovery,rec.mic}"; [ "$only" = crash ] && only=rec.kill9_recovery
out="$HOME/rec-regress-out"
rm -rf "$out"
"$HOME/wt/rec-target/debug/cm-regress" --dev --package-dir "$HOME/wt/rec-target/release" --tier hardware \
  --allow screen,mic --only "$only" \
  --out "$out" 2>&1 | tail -40
echo "report: $out/report-hardware.md"
sed -n 1,60p "$out/report-hardware.md"
