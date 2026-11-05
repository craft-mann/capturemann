#!/usr/bin/env bash
# From the CI package artifact to the release files: release-files.sh <artifact dir> <output dir> [release tag]
#
# <artifact dir> holds capturemann-<version>-windows-x64.zip and capturemann-<version>-windows-x64-setup.exe with
# their .sha256 files (the "capturemann-windows-x64" artifact of the "Windows package" workflow, any folder depth).
# Writes into <output dir>: SHA256SUMS.txt (both files, `sha256 *name`, readable by `sha256sum -c` and `shasum -a 256 -c`),
# and the winget manifest folder through winget/render.sh. Nothing is uploaded or pushed.
# The version comes from the file names; the installer url is the GitHub release asset of tag v<version> (or <release tag>).
# Needs only bash, find, shasum or sha256sum, grep, sed.
set -euo pipefail
[ $# -ge 2 ] || { sed -n '2,3p' "$0"; exit 1; }
art="$1"; out="$2"
here="$(cd "$(dirname "$0")" && pwd)"
sum() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
find_one() { # pattern
  local n; n="$(find "$art" -type f -name "$1" | wc -l | tr -d ' ')"
  [ "$n" = 1 ] || { echo "expected one file named $1 under $art, found $n" >&2; exit 1; }
  find "$art" -type f -name "$1"
}
zip="$(find_one 'capturemann-*-windows-x64.zip')"
setup="$(find_one 'capturemann-*-windows-x64-setup.exe')"
version="$(basename "$zip" | sed -E 's/^capturemann-(.*)-windows-x64\.zip$/\1/')"
[ "$(basename "$setup")" = "capturemann-$version-windows-x64-setup.exe" ] || { echo "zip and installer disagree on the version" >&2; exit 1; }
tag="${3:-v$version}"
mkdir -p "$out"
: > "$out/SHA256SUMS.txt"
for f in "$zip" "$setup"; do
  actual="$(sum "$f")"
  recorded="$(cut -d' ' -f1 "$f.sha256" 2>/dev/null || true)"
  [ "$actual" = "$recorded" ] || { echo "$(basename "$f"): computed $actual, the .sha256 file says '$recorded'" >&2; exit 1; }
  printf '%s *%s\n' "$actual" "$(basename "$f")" >> "$out/SHA256SUMS.txt"
done
cat "$out/SHA256SUMS.txt"
setup_sha="$(grep -F 'setup.exe' "$out/SHA256SUMS.txt" | cut -d' ' -f1)"
url="https://github.com/craft-mann/capturemann/releases/download/$tag/$(basename "$setup")"
"$here/winget/render.sh" "$version" "$url" "$setup_sha" "$out/winget"
echo "release files for $version written to $out (copy the zip, the installer, their .sha256 files and SHA256SUMS.txt to the release)"
