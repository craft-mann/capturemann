#!/usr/bin/env bash
# Temporary signing keychain for GitHub Actions.
#
# Usage:   packaging/macos/ci-keychain.sh setup | teardown
# setup    creates a keychain with a random password, imports the base64 .p12 from
#          APPLE_CERT_P12_BASE64 (password APPLE_CERT_PASSWORD), allows codesign to use the key
#          without a prompt, puts the keychain first in the user search list (the old list is saved
#          next to it), finds the "Developer ID Application" identity and appends
#            CM_SIGN_IDENTITY=<SHA-1 of the certificate>
#            CM_KEYCHAIN=<path of the keychain>
#          to $GITHUB_ENV (printed to stdout instead when GITHUB_ENV is unset). Secrets are masked
#          with ::add-mask:: and never printed.
# teardown deletes the keychain and restores the search list. It is safe to run twice.
# Environment (optional):
#   CM_KEYCHAIN             where teardown finds the keychain (set by setup through GITHUB_ENV)
#   CM_CI_KEYCHAIN_DIR      folder for the keychain (default: $RUNNER_TEMP, else a fresh temp dir)
#   CM_CI_IDENTITY_MATCH    identity name to look for (default "Developer ID Application"); tests use
#                           it with a throwaway certificate
#   CM_CI_ALLOW_UNTRUSTED=1 accept an identity the system does not trust (self-signed; tests only)
# This script never touches the login keychain except to read and restore the search list.
set -euo pipefail

die() { echo "ci-keychain.sh: $*" >&2; exit 1; }
[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
cmd="${1:-}"
[ "$#" -eq 1 ] || die "usage: ci-keychain.sh setup|teardown"

emit_env() { # emit_env NAME VALUE
  if [ -n "${GITHUB_ENV:-}" ]; then printf '%s=%s\n' "$1" "$2" >> "$GITHUB_ENV"; else printf '%s=%s\n' "$1" "$2"; fi
}

# Current user search list as one path per line (security prints them quoted).
current_list() { security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//'; }

case "$cmd" in
setup)
  : "${APPLE_CERT_P12_BASE64:?APPLE_CERT_P12_BASE64 is not set (base64 of the Developer ID Application .p12)}"
  : "${APPLE_CERT_PASSWORD:?APPLE_CERT_PASSWORD is not set (password of the .p12)}"
  base="${CM_CI_KEYCHAIN_DIR:-${RUNNER_TEMP:-}}"
  [ -n "$base" ] || base="$(mktemp -d "${TMPDIR:-/tmp}/cmci.XXXXXX")"
  mkdir -p "$base"
  dir="$(mktemp -d "$base/cmkc.XXXXXX")"
  kc="$dir/capturemann-release.keychain-db"
  pw="$(openssl rand -hex 24)"
  echo "::add-mask::$pw"
  echo "::add-mask::$APPLE_CERT_PASSWORD"
  # From here on a failure removes what was made.
  ok=0
  cleanup_on_fail() { [ "$ok" = "1" ] || { security delete-keychain "$kc" >/dev/null 2>&1 || true; rm -rf "$dir"; }; }
  trap cleanup_on_fail EXIT

  current_list > "$dir/search-list.txt"
  security create-keychain -p "$pw" "$kc"
  security set-keychain-settings -lut 21600 "$kc"
  security unlock-keychain -p "$pw" "$kc"
  p12="$dir/cert.p12"
  ( umask 077; printf '%s' "$APPLE_CERT_P12_BASE64" | { base64 --decode 2>/dev/null || base64 -D; } > "$p12" )
  [ -s "$p12" ] || die "APPLE_CERT_P12_BASE64 did not decode to anything"
  security import "$p12" -k "$kc" -P "$APPLE_CERT_PASSWORD" -f pkcs12 -T /usr/bin/codesign -T /usr/bin/security \
    >/dev/null || die "importing the .p12 failed (wrong APPLE_CERT_PASSWORD or not a PKCS#12 file)"
  rm -f "$p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" >/dev/null \
    || die "set-key-partition-list failed"
  # Search list: the new keychain first, then the previous ones.
  list=()
  while IFS= read -r line; do [ -z "$line" ] || list+=("$line"); done < "$dir/search-list.txt"
  security list-keychains -d user -s "$kc" ${list[@]+"${list[@]}"}

  match="${CM_CI_IDENTITY_MATCH:-Developer ID Application}"
  vflag=(-v)
  [ "${CM_CI_ALLOW_UNTRUSTED:-0}" != "1" ] || vflag=()
  line="$(security find-identity ${vflag[@]+"${vflag[@]}"} -p codesigning "$kc" | grep -F "$match" | head -n 1 || true)"
  [ -n "$line" ] || die "no valid \"$match\" identity found in the imported certificate (is it a Developer ID Application certificate with its private key, and not expired?)"
  sha="$(awk '{print $2}' <<<"$line")"
  name="$(sed -n 's/.*"\(.*\)".*/\1/p' <<<"$line")"
  echo "identity: $name"
  emit_env CM_SIGN_IDENTITY "$sha"
  emit_env CM_KEYCHAIN "$kc"
  ok=1
  trap - EXIT
  ;;
teardown)
  kc="${CM_KEYCHAIN:-}"
  [ -n "$kc" ] || { echo "ci-keychain.sh: CM_KEYCHAIN is not set: nothing to tear down"; exit 0; }
  dir="$(dirname "$kc")"
  if [ -f "$dir/search-list.txt" ]; then
    list=()
    while IFS= read -r line; do [ -z "$line" ] || list+=("$line"); done < "$dir/search-list.txt"
    if [ "${#list[@]}" -gt 0 ]; then security list-keychains -d user -s "${list[@]}" || true; fi
  else
    echo "ci-keychain.sh: no saved search list next to $kc; removing it from the list instead" >&2
    rest=()
    while IFS= read -r line; do [ -z "$line" ] || [ "$line" = "$kc" ] || rest+=("$line"); done < <(current_list)
    if [ "${#rest[@]}" -gt 0 ]; then security list-keychains -d user -s "${rest[@]}" || true; fi
  fi
  security delete-keychain "$kc" >/dev/null 2>&1 || true
  # Only a folder this script made (named cmkc.*) is removed.
  case "$(basename "$dir")" in cmkc.*) rm -rf "$dir" ;; esac
  echo "keychain removed"
  ;;
*) die "usage: ci-keychain.sh setup|teardown" ;;
esac
