#!/usr/bin/env bash
# Signs the Linux release files with minisign (https://jedisct1.github.io/minisign/), on the
# maintainer's machine. The secret key never goes to CI.
#
# Usage:  packaging/linux/sign-linux.sh [folder]      (default: packaging/linux/out)
# Signs every capturemann-*-linux-*.tar.gz and .sha256 in the folder, writing <file>.minisig, then
# verifies each signature against the PUBLIC key committed as packaging/linux/minisign.pub.
#
# Environment:
#   CM_MINISIGN_KEY   the secret key file (default: ~/.minisign/minisign.key). minisign asks for its password.
#   CM_MINISIGN_PUB   the public key to verify with (default: packaging/linux/minisign.pub).
#
# One-time setup:  minisign -G -p packaging/linux/minisign.pub -s ~/.minisign/minisign.key
# then commit minisign.pub and publish the key's fingerprint (the second line of the .pub file) on the
# release page and the website, so a user can compare it with the file they download.
set -euo pipefail
die() { echo "sign-linux.sh: $*" >&2; exit 1; }

here="$(cd "$(dirname "$0")" && pwd)"
dir="${1:-$here/out}"
key="${CM_MINISIGN_KEY:-$HOME/.minisign/minisign.key}"
pub="${CM_MINISIGN_PUB:-$here/minisign.pub}"

command -v minisign >/dev/null || die "minisign is not installed (brew install minisign; apt install minisign)"
[ -d "$dir" ] || die "no such folder: $dir"
[ -f "$key" ] || die "secret key not found: $key (set CM_MINISIGN_KEY, or create one: see the header of this script)"
[ -f "$pub" ] || die "public key not found: $pub; commit the key made with 'minisign -G' so signatures can be checked"

shopt -s nullglob
files=("$dir"/capturemann-*-linux-*.tar.gz "$dir"/capturemann-*-linux-*.tar.gz.sha256)
[ "${#files[@]}" -gt 0 ] || die "no capturemann-*-linux-* files in $dir"

for f in "${files[@]}"; do
  name="$(basename "$f")"
  # The trusted comment is signed too: it names the file, so a signature cannot be moved to another file.
  minisign -S -s "$key" -m "$f" -t "CaptureMann Linux early build: $name"
  minisign -V -p "$pub" -m "$f" -q || die "the signature of $name does not verify with $pub (wrong key?)"
  echo "signed and verified: $name.minisig"
done
