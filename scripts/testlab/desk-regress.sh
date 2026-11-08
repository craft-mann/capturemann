#!/usr/bin/env bash
# Runs ON the lab laptop (lab lock taken by the caller, owner "desktop"): the cm-regress tier against the debug build in
# the target dir, in the logged-in desktop session. Usage: desk-regress.sh <target-dir> <tier> <label> [allow] [only]
set -u
T="$1"; TIER="$2"; LABEL="$3"; ALLOW="${4:-screen}"; ONLY="${5:-}"
bash "$(dirname "$0")/desk-env.sh" >/dev/null
set -a; . ~/lab/session.env; set +a
OUT="$HOME/wt/regress-$LABEL"
rm -rf "$OUT"; mkdir -p "$OUT"
args=(--tier "$TIER" --out "$OUT" --who "desktop agent ($LABEL)" --verdict PARTIAL --package-dir "$T/debug")
[ -n "$ALLOW" ] && args+=(--allow "$ALLOW")
[ -n "$ONLY" ] && args+=(--only "$ONLY")
echo "$(date -u +%FT%TZ) start ${args[*]}" > "$OUT.console.txt"
timeout -k 20 3000 "$T/debug/cm-regress" "${args[@]}" >> "$OUT.console.txt" 2>&1
echo "EXIT=$?" >> "$OUT.console.txt"
