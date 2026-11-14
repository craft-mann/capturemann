#!/usr/bin/env bash
# s2_run.sh <set-subdir> <label> [step]: runs the S2 measurer on a folder of screenshots kept
# outside the repository and writes the BOX lines to <label>.txt next to it.
set -eu
export CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_BUILD_JOBS=4
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/60bc0427-25e0-4ba6-a03e-32e606bb68fa/scratchpad/s2-shots
mkdir -p "$S/crops_$2"
CM_S2_DIR="$S/$1" CM_S2_CROPS="$S/crops_$2" CM_S2_STEP="${3:-25}" \
  cargo test -p cm-render --test touchup_034 --locked s2_real_set -- --ignored --nocapture >"$S/$2.txt" 2>&1
grep -c '^BOX' "$S/$2.txt"
