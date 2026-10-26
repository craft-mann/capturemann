#!/usr/bin/env bash
# Points the Homebrew cask at a published release: sets `version` and `sha256` from the release's
# .sha256 asset. Run it in a checkout of the tap (craft-mann/homebrew-tap), then commit and push there.
#   scripts/update-homebrew-cask.sh 0.1.1 ~/Documents/homebrew-tap
set -euo pipefail

version="${1:?usage: update-homebrew-cask.sh <version, e.g. 0.1.1> [tap checkout, default: .]}"
tap="${2:-.}"
version="${version#v}"
cask="$tap/Casks/capturemann.rb"
[ -f "$cask" ] || { echo "no $cask (pass the path of the homebrew-tap checkout)" >&2; exit 1; }

asset="CaptureMann-v${version}-macos-arm64.dmg"
url="https://github.com/craft-mann/capturemann/releases/download/v${version}/${asset}.sha256"
sum="$(curl -fsSL "$url" | awk -v f="$asset" '$2 == f {print $1}')"
case "$sum" in
  *[!0-9a-f]* | "") echo "could not read a SHA-256 for $asset from $url" >&2; exit 1 ;;
esac
[ "${#sum}" -eq 64 ] || { echo "unexpected checksum length: $sum" >&2; exit 1; }

# BSD and GNU sed differ on -i; write through a temp file instead.
tmp="$(mktemp)"
sed -E -e "s/^(  version )\"[^\"]*\"/\1\"${version}\"/" \
       -e "s/^(  sha256 )\"[0-9a-f]*\"/\1\"${sum}\"/" "$cask" > "$tmp"
cat "$tmp" > "$cask"
rm -f "$tmp"
echo "cask now at ${version}, sha256 ${sum}"
grep -nE '^  (version|sha256) ' "$cask"
