#!/usr/bin/env bash
# Checks a release candidate: a DMG (mounted read-only, always unmounted) or a CaptureMann.app.
#
# Usage:   packaging/macos/verify-release.sh [--allow-unsigned] <file.dmg|CaptureMann.app>
#   --allow-unsigned  the build is not release-signed (ad hoc / local identity / not notarized):
#                     Gatekeeper, stapler, hardened runtime, timestamp, a missing FFmpeg and a
#                     missing DMG signature become WARNINGs instead of ERRORs. Everything else
#                     (signature validity, entitlements, licences, version, arm64, minimum macOS,
#                     LGPL FFmpeg) is still an ERROR. Without the flag every check is an ERROR,
#                     except that Gatekeeper rejecting a signature that is not Developer ID is a
#                     labelled WARNING (that is what a local identity gives).
# Environment: CM_EXPECT_VERSION overrides the version read from the workspace Cargo.toml.
# Exit status: 1 when any ERROR was printed, else 0.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
allow=0; target=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --allow-unsigned) allow=1 ;;
    -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
    -*) echo "verify-release.sh: unknown option: $1" >&2; exit 2 ;;
    *) [ -z "$target" ] || { echo "verify-release.sh: only one target" >&2; exit 2; }; target="$1" ;;
  esac
  shift
done
[ -n "$target" ] || { echo "usage: verify-release.sh [--allow-unsigned] <file.dmg|CaptureMann.app>" >&2; exit 2; }
[ "$(uname -s)" = "Darwin" ] || { echo "verify-release.sh: macOS only" >&2; exit 2; }

errors=0; warnings=0
ok()   { echo "  ok       $*"; }
err()  { echo "  ERROR    $*" >&2; errors=$((errors + 1)); }
warn() { echo "  WARNING  $*"; warnings=$((warnings + 1)); }
# soft <message>: an ERROR for a real release, a WARNING under --allow-unsigned.
soft() { if [ "$allow" = "1" ]; then warn "$*"; else err "$*"; fi; }
section() { echo "== $*"; }

mnt=""
cleanup() {
  if [ -n "$mnt" ]; then
    hdiutil detach "$mnt" -quiet >/dev/null 2>&1 || hdiutil detach "$mnt" -force -quiet >/dev/null 2>&1 || true
    rmdir "$mnt" 2>/dev/null || true
  fi
}
trap cleanup EXIT

from_dmg=0; dmg_stapled=0
case "$target" in
  *.dmg)
    [ -f "$target" ] || { echo "verify-release.sh: not found: $target" >&2; exit 2; }
    from_dmg=1
    section "DMG $target"
    if codesign --verify --strict "$target" >/dev/null 2>&1; then
      ok "the DMG is signed and the signature verifies"
    else
      soft "the DMG is not signed (or its signature is invalid)"
    fi
    if out="$(xcrun stapler validate "$target" 2>&1)"; then
      ok "stapler validate (DMG): ticket present"; dmg_stapled=1
    else
      soft "stapler validate failed for the DMG: $(tail -n 1 <<<"$out")"
    fi
    if out="$(spctl --assess --type open --context context:primary-signature -v "$target" 2>&1)"; then
      ok "Gatekeeper accepts the DMG: $out"
    else
      soft "Gatekeeper rejects the DMG: $out"
    fi
    mnt="$(mktemp -d "${TMPDIR:-/tmp}/cmverify.XXXXXX")"
    if ! hdiutil attach -readonly -nobrowse -noautoopen -noverify -mountpoint "$mnt" "$target" >/dev/null 2>&1; then
      err "could not mount the DMG read-only"
      rmdir "$mnt" 2>/dev/null; mnt=""
      echo "result: $errors error(s), $warnings warning(s)" >&2; exit 1
    fi
    app="$mnt/CaptureMann.app"
    [ -d "$app" ] || { err "no CaptureMann.app at the top of the DMG"; echo "result: $errors error(s)" >&2; exit 1; }
    ok "mounted read-only at $mnt"
    if [ -L "$mnt/Applications" ] && [ "$(readlink "$mnt/Applications")" = "/Applications" ]; then
      ok "Applications symlink present"
    else
      err "the DMG has no /Applications symlink"
    fi
    ;;
  *.app)
    [ -d "$target" ] || { echo "verify-release.sh: not found: $target" >&2; exit 2; }
    app="$(cd "$target" && pwd)"
    ;;
  *) echo "verify-release.sh: expected a .dmg or .app: $target" >&2; exit 2 ;;
esac

contents="$app/Contents"
plist="$contents/Info.plist"
[ -f "$plist" ] || { err "no Info.plist in $app"; echo "result: $errors error(s)" >&2; exit 1; }

# --- signature of the bundle ---
section "signature"
if out="$(codesign --verify --deep --strict "$app" 2>&1)"; then
  ok "codesign --verify --deep --strict"
else
  err "codesign --verify --deep --strict failed: $out"
fi
info="$(codesign -dvvv "$app" 2>&1)"
authority="$(sed -n 's/^Authority=//p' <<<"$info" | head -n 1)"
team="$(sed -n 's/^TeamIdentifier=//p' <<<"$info" | head -n 1)"
if grep -q '^Signature=adhoc' <<<"$info"; then
  kind="adhoc"; authority="(ad hoc)"
elif [[ "$authority" == "Developer ID Application:"* ]]; then
  kind="devid"
else
  kind="other"
fi
echo "  identity: kind=$kind authority='${authority:-none}' team='${team:-none}'"
echo "  designated requirement:"
codesign -d -r- "$app" 2>&1 | sed -n 's/^# designated => /    /p;s/^designated => /    /p'
if [ "$kind" = "devid" ]; then ok "signed with a Developer ID Application certificate"; else
  soft "not signed with a Developer ID Application certificate (kind=$kind): cannot be notarized"
fi

# strict checks (hardened runtime, timestamp) apply to every binary below too
hardened_timestamp() { # hardened_timestamp <path> <label>
  local i flags
  i="$(codesign -dvvv "$1" 2>&1)"
  flags="$(sed -n 's/^CodeDirectory .*flags=\(0x[0-9a-f]*([^)]*)\).*/\1/p' <<<"$i" | head -n 1)"
  if [[ "$flags" == *runtime* ]]; then
    ok "$2: hardened runtime ($flags)"
  else
    soft "$2: hardened runtime flag missing (flags ${flags:-none}); build with CM_HARDENED=1"
  fi
  if grep -q '^Timestamp=' <<<"$i"; then
    ok "$2: secure timestamp"
  else
    soft "$2: no secure timestamp (signed with --timestamp?)"
  fi
}

section "hardened runtime and timestamp (bundle)"
hardened_timestamp "$app" "bundle"

# --- entitlements ---
section "entitlements"
ent_keys() { # ent_keys <path> -> "true" keys, one per line (empty when none)
  codesign -d --entitlements :- "$1" 2>/dev/null | python3 -I -c '
import sys, plistlib
data = sys.stdin.buffer.read()
i = data.find(b"<?xml")
if i < 0:
    sys.exit(0)
try:
    d = plistlib.loads(data[i:])
except Exception:
    sys.exit(0)
for k, v in sorted(d.items()):
    print(k + ("" if v is True else "=" + repr(v)))
'
}
main_exe="$contents/MacOS/$(plutil -extract CFBundleExecutable raw "$plist" 2>/dev/null || echo cm-desktop)"
got="$(ent_keys "$main_exe")"
want="$(grep -o '<key>[^<]*</key>' "$root/packaging/macos/CaptureMann.entitlements" | sed 's/<[^>]*>//g' | sort)"
echo "  main executable entitlements: $(tr '\n' ' ' <<<"$got")"
if grep -q 'get-task-allow' <<<"$got"; then err "get-task-allow is present (a debug signature; notarization rejects it)"; else ok "no get-task-allow"; fi
if [ "$got" = "$want" ]; then
  ok "entitlements equal packaging/macos/CaptureMann.entitlements"
else
  err "entitlements differ from packaging/macos/CaptureMann.entitlements (expected: $(tr '\n' ' ' <<<"$want"))"
fi
bundle_ent="$(ent_keys "$app")"
[ "$bundle_ent" = "$got" ] && ok "the bundle seal carries the same entitlements" || warn "bundle-level entitlements differ from the main executable's: $(tr '\n' ' ' <<<"$bundle_ent")"

# --- every Mach-O ---
section "Mach-O files under Contents"
count=0
while IFS= read -r f; do
  desc="$(file -b "$f" 2>/dev/null || true)"
  [[ "$desc" == *Mach-O* ]] || continue
  count=$((count + 1))
  rel="${f#"$app"/}"
  if ! out="$(codesign --verify --strict "$f" 2>&1)"; then err "$rel: signature invalid: $out"; continue; fi
  i="$(codesign -dvvv "$f" 2>&1)"
  a="$(sed -n 's/^Authority=//p' <<<"$i" | head -n 1)"; t="$(sed -n 's/^TeamIdentifier=//p' <<<"$i" | head -n 1)"
  if grep -q '^Signature=adhoc' <<<"$i"; then a="(ad hoc)"; fi
  if [ "$a" = "$authority" ] && [ "$t" = "$team" ]; then
    ok "$rel: same identity"
  else
    err "$rel: signed by '${a:-none}' team '${t:-none}', not by '$authority' team '${team:-none}'"
  fi
  hardened_timestamp "$f" "$rel"
  if grep -q 'get-task-allow' <<<"$(ent_keys "$f")"; then err "$rel: get-task-allow"; fi
  archs="$(lipo -archs "$f" 2>/dev/null || true)"
  if [ "$archs" = "arm64" ]; then :; else err "$rel: architectures '$archs' (expected arm64 only)"; fi
done < <(find "$contents" -type f -not -name '*.plist' -not -name '*.md' -not -name '*.txt' -not -name '*.icns' 2>/dev/null)
if [ "$count" -ge 4 ]; then ok "$count Mach-O files checked (identity, hardened runtime, timestamp, arm64)"; else err "only $count Mach-O files found under Contents (expected cm-desktop, cm-overlay, cm-mcp, capturemann at least)"; fi

# --- Gatekeeper and ticket ---
section "Gatekeeper and notarization ticket"
if out="$(spctl --assess --type execute -v "$app" 2>&1)"; then
  ok "Gatekeeper: $out"
else
  if [ "$kind" = "devid" ] && [ "$allow" = "0" ]; then
    err "Gatekeeper rejects the app: $out"
  elif [ "$kind" = "devid" ]; then
    warn "Gatekeeper rejects the app (downgraded by --allow-unsigned): $out"
  else
    warn "Gatekeeper rejects the app: $out (expected for a local or ad hoc signature; a release must be Developer ID and notarized)"
  fi
fi
if out="$(xcrun stapler validate "$app" 2>&1)"; then
  ok "stapler validate (app)"
elif [ "$from_dmg" = "1" ] && [ "$dmg_stapled" = "1" ]; then
  ok "the ticket is stapled to the DMG (the app inside carries none, which is normal)"
else
  soft "stapler validate failed: $(tail -n 1 <<<"$out")"
fi

# --- FFmpeg (LGPL) ---
section "FFmpeg"
ff="$contents/MacOS/ffmpeg"
if [ -x "$ff" ]; then
  conf="$("$ff" -hide_banner -version 2>&1; "$ff" -hide_banner -buildconf 2>&1)"
  if grep -q -E -e '--enable-(gpl|nonfree|libx264|libx265|libfdk-aac)' <<<"$conf"; then
    err "bundled ffmpeg is built with --enable-gpl/nonfree/libx264/libx265/libfdk-aac (NFR-12)"
  elif grep -q 'configuration:' <<<"$conf"; then
    ok "bundled ffmpeg is an LGPL build (no gpl, nonfree, libx264, libx265, libfdk-aac)"
  else
    err "bundled ffmpeg did not print its configuration (cannot start?): $(head -n 1 <<<"$conf")"
  fi
  [ -x "$contents/MacOS/ffprobe" ] && ok "ffprobe bundled" || warn "no ffprobe bundled"
  if [ -f "$contents/Resources/FFmpeg/SOURCE-OFFER.txt" ] && [ -d "$contents/Resources/FFmpeg/LICENSES" ]; then
    ok "FFmpeg SOURCE-OFFER.txt and LICENSES present"
  else
    err "Resources/FFmpeg has no SOURCE-OFFER.txt and LICENSES/ (the LGPL notices must travel with the binary)"
  fi
else
  soft "no ffmpeg in Contents/MacOS (a release needs the LGPL build: make ffmpeg, make bundle-release)"
fi

# --- llama-server (MIT), the helper of the built-in text engine ---
section "llama-server"
ll="$contents/MacOS/llama-server"
if [ -x "$ll" ]; then
  if otool -L "$ll" 2>/dev/null | grep -q -i -E 'curl|ssl|crypto'; then
    err "bundled llama-server links curl or OpenSSL (it must not reach the network by itself)"
  else
    ok "bundled llama-server links no curl or OpenSSL"
  fi
  if [ -f "$contents/Resources/Llama/LICENSES/llama.cpp-LICENSE.txt" ]; then
    ok "llama.cpp licence present"
  else
    err "Resources/Llama has no LICENSES/llama.cpp-LICENSE.txt (the MIT notice must travel with the binary)"
  fi
else
  soft "no llama-server in Contents/MacOS (a release needs it: make llama, make bundle-release)"
fi

# --- speaker-label models (AI-8): data files inside the app, hashes pinned ---
section "speaker models"
spk="$contents/Resources/Models/speaker"
if [ -d "$spk" ]; then
  for pair in "pyannote-segmentation-3-0.onnx:220ad67ca923bef2fa91f2390c786097bf305bceb5e261d4af67b38e938e1079" \
              "campplus-sv-en-voxceleb-16k.onnx:357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b"; do
    f="${pair%%:*}"; want="${pair##*:}"
    if [ -f "$spk/$f" ] && [ "$(shasum -a 256 "$spk/$f" | awk '{print $1}')" = "$want" ]; then
      ok "$f matches its pinned SHA-256"
    else
      err "Resources/Models/speaker/$f is missing or does not match its pinned SHA-256"
    fi
  done
  for l in pyannote-segmentation-3.0-LICENSE.txt campplus-3D-Speaker-NOTICE.txt; do
    [ -s "$spk/LICENSES/$l" ] && ok "speaker model notice $l" || err "Resources/Models/speaker/LICENSES/$l is missing"
  done
else
  soft "no speaker models in Resources/Models/speaker (a release needs them: make sherpa, make bundle-release)"
fi

# --- licences, version, platform ---
section "licences, version, platform"
for f in LICENSE NOTICE THIRD_PARTY_LICENSES.md; do
  [ -s "$contents/Resources/$f" ] && ok "Resources/$f" || err "Resources/$f is missing or empty"
done
want_ver="${CM_EXPECT_VERSION:-}"
if [ -z "$want_ver" ] && [ -f "$root/Cargo.toml" ]; then
  want_ver="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version *=/{gsub(/^[^"]*"|".*$/,""); print; exit}' "$root/Cargo.toml")"
fi
have_ver="$(plutil -extract CFBundleShortVersionString raw "$plist" 2>/dev/null || true)"
if [ -z "$want_ver" ]; then
  warn "no workspace version to compare with (no Cargo.toml, no CM_EXPECT_VERSION)"
elif [ "$have_ver" = "$want_ver" ]; then
  ok "Info.plist version $have_ver equals the workspace version"
else
  err "Info.plist version '$have_ver' differs from the workspace version '$want_ver'"
fi
minos="$(plutil -extract LSMinimumSystemVersion raw "$plist" 2>/dev/null || true)"
if [ "$minos" = "13.0" ]; then ok "LSMinimumSystemVersion 13.0"; else err "LSMinimumSystemVersion is '${minos:-unset}', expected 13.0"; fi
if command -v vtool >/dev/null 2>&1 || xcrun --find vtool >/dev/null 2>&1; then
  while IFS= read -r f; do
    [[ "$(file -b "$f" 2>/dev/null)" == *Mach-O* ]] || continue
    m="$(xcrun vtool -show-build "$f" 2>/dev/null | awk '/minos/{print $2; exit}')"
    [ -n "$m" ] || continue
    if [ "$(printf '%s\n13.0\n' "$m" | sort -V | tail -n 1)" = "13.0" ]; then :; else err "${f#"$app"/}: built for macOS $m or later (> 13.0)"; fi
  done < <(find "$contents/MacOS" -type f)
  ok "binary deployment targets checked with vtool (all <= 13.0 unless reported above)"
else
  warn "vtool not available: binary deployment targets not checked"
fi

echo
if [ "$errors" -gt 0 ]; then
  echo "result: FAILED, $errors error(s), $warnings warning(s)" >&2
  exit 1
fi
echo "result: passed, $warnings warning(s)"
