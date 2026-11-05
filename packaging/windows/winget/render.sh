#!/usr/bin/env bash
# Fills the winget manifest templates: render.sh <version> <installer url> <installer sha256> <output dir>
set -euo pipefail
[ $# -eq 4 ] || { sed -n '2p' "$0"; exit 1; }
version="$1"; url="$2"; sha="$3"; out="$4"
case "$version" in [0-9]*.[0-9]*.[0-9]*) ;; *) echo "version must look like 0.2.8" >&2; exit 1 ;; esac
case "$url" in https://*) ;; *) echo "the installer url must be https" >&2; exit 1 ;; esac
printf '%s' "$sha" | grep -Eq '^[0-9A-Fa-f]{64}$' || { echo "sha256 must be 64 hex digits" >&2; exit 1; }
here="$(cd "$(dirname "$0")" && pwd)"
dir="$out/manifests/c/CaptureMann/CaptureMann/$version"
mkdir -p "$dir"
date="$(date -u +%Y-%m-%d)"
sha="$(printf '%s' "$sha" | tr 'a-f' 'A-F')"
for f in "$here"/*.template; do
  name="$(basename "${f%.template}")"
  sed -e "s|@VERSION@|$version|g" -e "s|@URL@|$url|g" -e "s|@SHA256@|$sha|g" -e "s|@DATE@|$date|g" "$f" > "$dir/$name"
done
echo "$dir"
