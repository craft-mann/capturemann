#!/usr/bin/env bash
# Points the Homebrew cask at a published release: sets `version` and the `sha256` values from the
# release's .sha256 assets. Run it in a checkout of the tap (craft-mann/homebrew-tap), then commit
# and push there.
#   scripts/update-homebrew-cask.sh 0.2.1 ~/Documents/homebrew-tap
#
# From 0.2.1 a release has one DMG per chip (macos-arm64 and macos-x86_64), so the cask carries two
# checksums: `sha256 arm: "...", intel: "..."`, with `arch arm: "arm64", intel: "x86_64"` and the url
# ending in `-macos-#{arch}.dmg` (the cask shape is in docs/development/releasing.md). A cask that
# still has the single-checksum form (0.2.0 and older, arm64 only) is updated in that form, and only
# when the release has no Intel DMG; a release with both asks you to change the cask shape first.
set -euo pipefail

version="${1:?usage: update-homebrew-cask.sh <version, e.g. 0.2.1> [tap checkout, default: .]}"
tap="${2:-.}"
version="${version#v}"
cask="$tap/Casks/capturemann.rb"
[ -f "$cask" ] || { echo "no $cask (pass the path of the homebrew-tap checkout)" >&2; exit 1; }

# sum_of <arch>: the SHA-256 of that chip's DMG from its .sha256 asset, or nothing when there is none.
sum_of() {
  local asset="CaptureMann-v${version}-macos-$1.dmg"
  local url="https://github.com/craft-mann/capturemann/releases/download/v${version}/${asset}.sha256"
  curl -fsSL "$url" 2>/dev/null | awk -v f="$asset" '$2 == f {print $1}' || true
}
valid() { case "$1" in *[!0-9a-f]* | "") return 1 ;; esac; [ "${#1}" -eq 64 ]; }

arm="$(sum_of arm64)"
intel="$(sum_of x86_64)"
valid "$arm" || { echo "could not read a SHA-256 for the arm64 DMG of v${version}" >&2; exit 1; }

tmp="$(mktemp)"   # BSD and GNU sed differ on -i; write through a temp file instead.
if grep -q -E '^  sha256 arm:' "$cask"; then
  valid "$intel" || { echo "the cask is per-chip but v${version} has no readable x86_64 DMG checksum" >&2; exit 1; }
  sed -E -e '/^         intel: /d' \
         -e "s/^(  version )\"[^\"]*\"/\1\"${version}\"/" \
         -e "s/^  sha256 arm: .*/  sha256 arm:   \"${arm}\",\n         intel: \"${intel}\"/" "$cask" > "$tmp"
elif valid "$intel"; then
  rm -f "$tmp"
  echo "v${version} has an Intel DMG, but the cask is still the single-checksum form." >&2
  echo "Change the cask to the per-chip form first (see docs/development/releasing.md), then run this again." >&2
  exit 1
else
  sed -E -e "s/^(  version )\"[^\"]*\"/\1\"${version}\"/" \
         -e "s/^(  sha256 )\"[0-9a-f]*\"/\1\"${arm}\"/" "$cask" > "$tmp"
fi
cat "$tmp" > "$cask"
rm -f "$tmp"
echo "cask now at ${version}, arm64 sha256 ${arm}${intel:+, x86_64 sha256 $intel}"
grep -nE '^  (version|sha256|         intel|arch) ' "$cask"
