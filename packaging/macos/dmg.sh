#!/usr/bin/env bash
# Builds a compressed (UDZO), read-only DMG holding CaptureMann.app and an /Applications symlink,
# signs it when CM_SIGN_IDENTITY is set (and not "-"), prints the path and the SHA-256.
#
# Usage:   packaging/macos/dmg.sh <path/to/CaptureMann.app> <out.dmg>
# Environment:
#   CM_DMG_VOLNAME    volume name (default CaptureMann)
#   CM_SIGN_IDENTITY  codesign identity for the DMG itself; unset or "-" = leave the DMG unsigned
#   CM_KEYCHAIN       optional keychain holding the identity (passed to codesign --keychain)
#   CM_DMG_NO_TIMESTAMP=1  sign without the secure timestamp (offline tests only; never for a release)
# Only hdiutil/ditto/codesign are used (no Finder, no AppleScript), so it works on CI. The window
# layout is therefore the plain default: the app and the Applications link, icon view.
set -euo pipefail

die() { echo "dmg.sh: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
[ "$#" -eq 2 ] || die "usage: dmg.sh <path/to/CaptureMann.app> <out.dmg>"
app="$1"; out="$2"
for tool in hdiutil ditto codesign shasum; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
[ -d "$app/Contents/MacOS" ] && [ -f "$app/Contents/Info.plist" ] || die "not an app bundle: $app"
case "$out" in *.dmg) ;; *) die "output must end in .dmg: $out" ;; esac
volname="${CM_DMG_VOLNAME:-CaptureMann}"
case "$volname" in *[!A-Za-z0-9._\ -]* | "") die "CM_DMG_VOLNAME may only contain letters, digits, space, '.', '_' and '-': '$volname'" ;; esac

mkdir -p "$(dirname "$out")"
out="$(cd "$(dirname "$out")" && pwd)/$(basename "$out")"
app="$(cd "$app" && pwd)"
work="$(mktemp -d "${TMPDIR:-/tmp}/cmdmg.XXXXXX")"
trap 'rm -rf "$work"' EXIT

stage="$work/stage"
mkdir -p "$stage"
# ditto keeps the code signature, extended attributes and symlinks of the bundle intact.
ditto "$app" "$stage/$(basename "$app")"
ln -s /Applications "$stage/Applications"

tmp_dmg="$work/out.dmg"
echo "==> hdiutil create ($volname)"
hdiutil create -quiet -volname "$volname" -srcfolder "$stage" -fs HFS+ -format UDZO \
  -imagekey zlib-level=9 -ov "$tmp_dmg" >/dev/null || die "hdiutil create failed"

identity="${CM_SIGN_IDENTITY:--}"
if [ "$identity" != "-" ]; then
  args=(--force --sign "$identity")
  if [ "${CM_DMG_NO_TIMESTAMP:-0}" = "1" ]; then
    echo "dmg.sh: WARNING: signing without a timestamp (CM_DMG_NO_TIMESTAMP=1)" >&2
  else
    args+=(--timestamp)
  fi
  [ -z "${CM_KEYCHAIN:-}" ] || args+=(--keychain "$CM_KEYCHAIN")
  echo "==> codesign the DMG"
  codesign "${args[@]}" "$tmp_dmg" || die "codesign of the DMG failed"
  codesign --verify --strict "$tmp_dmg" || die "the signed DMG does not verify"
else
  echo "==> CM_SIGN_IDENTITY not set (or '-'): the DMG is left unsigned"
fi

mv -f "$tmp_dmg" "$out"
hash="$(shasum -a 256 "$out" | awk '{print $1}')"
printf '%s  %s\n' "$hash" "$(basename "$out")" > "$out.sha256"
echo "dmg:    $out"
echo "sha256: $hash"
