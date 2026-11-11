#!/bin/sh
# usage: pack-real.sh <version> <payload dir> <outdir> [extra vpk args]
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad
V=$1; P=$2; O=$3; shift 3
mkdir -p "$O"
START=$(date +%s)
sh $S/bin/vpkm.sh '[win]' pack -u dev.capturemann.spike -v "$V" -p "$P" -e cm-desktop.exe -o "$O" \
  --packTitle CaptureMann --packAuthors "CaptureMann contributors" -i "$P/app.ico" "$@" 2>&1 | cut -c1-220 | tail -40
END=$(date +%s)
echo "pack seconds: $((END-START))"
ls -l "$O"
