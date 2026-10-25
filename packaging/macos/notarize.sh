#!/usr/bin/env bash
# Submits a DMG (or a zip) to Apple's notary service, waits for the verdict and staples the ticket.
#
# Usage:   packaging/macos/notarize.sh [--dry-run] [--check-env] <file.dmg|file.zip>
#   --dry-run    validate the arguments and the environment, print the exact commands (secrets
#                masked) and run none of them.
#   --check-env  validate only the credentials environment (no file argument), then exit.
# Credentials, either:
#   App Store Connect API key:  APPLE_API_KEY_ID, APPLE_API_ISSUER, APPLE_API_KEY_PATH (the .p8 file)
#   Apple ID:                   APPLE_ID, APPLE_TEAM_ID, APPLE_APP_PASSWORD (app-specific password)
#   or a stored keychain profile (`xcrun notarytool store-credentials`): CM_NOTARY_PROFILE
# The API key wins when both are complete. The Apple ID password has to be passed to notarytool on
# its command line (visible in `ps` for the duration of the call); prefer the API key on shared machines.
# Other: CM_NOTARY_TIMEOUT (default 2h) bounds the wait.
# On "Invalid" (or any non-Accepted verdict) the notarytool log is fetched and printed, and the
# script exits 1. On "Accepted" a DMG is stapled and validated (a zip cannot be stapled).
set -euo pipefail

die() { echo "notarize.sh: ERROR: $*" >&2; exit 1; }

dry=0; check_env=0; file=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) dry=1 ;;
    --check-env) check_env=1 ;;
    -h|--help) sed -n '2,17p' "$0"; exit 0 ;;
    -*) die "unknown option: $1" ;;
    *) [ -z "$file" ] || die "only one file may be given"; file="$1" ;;
  esac
  shift
done

# --- credentials: cred = real arguments, shown = the same with secrets masked ---
mode=""; cred=(); shown=()
missing_of() { # missing_of VAR... -> prints the unset ones
  local v out=""
  for v in "$@"; do [ -n "${!v:-}" ] || out="$out $v"; done
  printf '%s' "$out"
}
api_missing="$(missing_of APPLE_API_KEY_ID APPLE_API_ISSUER APPLE_API_KEY_PATH)"
id_missing="$(missing_of APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD)"
if [ -z "$api_missing" ]; then
  mode="App Store Connect API key"
  [ -f "$APPLE_API_KEY_PATH" ] && [ -r "$APPLE_API_KEY_PATH" ] || die "APPLE_API_KEY_PATH is not a readable file: $APPLE_API_KEY_PATH"
  cred=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER")
  shown=("${cred[@]}")
elif [ -z "$id_missing" ]; then
  mode="Apple ID"
  cred=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
  shown=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password '********')
elif [ -n "${CM_NOTARY_PROFILE:-}" ]; then
  mode="keychain profile"
  cred=(--keychain-profile "$CM_NOTARY_PROFILE")
  shown=("${cred[@]}")
else
  {
    echo "notarize.sh: ERROR: no notarization credentials in the environment."
    if [ "$api_missing" != " APPLE_API_KEY_ID APPLE_API_ISSUER APPLE_API_KEY_PATH" ]; then
      echo "  The App Store Connect API key is incomplete; missing:$api_missing"
    fi
    if [ "$id_missing" != " APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD" ]; then
      echo "  The Apple ID login is incomplete; missing:$id_missing"
    fi
    echo "  Set either APPLE_API_KEY_ID + APPLE_API_ISSUER + APPLE_API_KEY_PATH (path to the .p8),"
    echo "  or APPLE_ID + APPLE_TEAM_ID + APPLE_APP_PASSWORD (an app-specific password),"
    echo "  or CM_NOTARY_PROFILE (a profile made with \`xcrun notarytool store-credentials\`)."
  } >&2
  exit 1
fi
echo "==> credentials: $mode"
if [ "$check_env" = "1" ]; then echo "environment OK"; exit 0; fi

# --- arguments and tools ---
[ -n "$file" ] || die "usage: notarize.sh [--dry-run] <file.dmg|file.zip>"
[ -f "$file" ] || die "file not found: $file"
case "$file" in
  *.dmg) kind=dmg ;;
  *.zip) kind=zip ;;
  *) die "only a .dmg or .zip can be submitted: $file" ;;
esac
[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
xcrun --find notarytool >/dev/null 2>&1 || die "xcrun notarytool not found (install the Xcode command line tools, Xcode 13 or later)"
xcrun --find stapler >/dev/null 2>&1 || die "xcrun stapler not found"
command -v python3 >/dev/null 2>&1 || die "python3 not found (used to read notarytool's JSON)"
timeout="${CM_NOTARY_TIMEOUT:-2h}"

if [ "$dry" = "1" ]; then
  echo "==> dry run: nothing is sent. Commands that would run:"
  printf '  %q' xcrun notarytool submit "$file" --wait --timeout "$timeout" --output-format json "${shown[@]}"; echo
  echo "  # on Invalid / Rejected:"
  printf '  %q' xcrun notarytool log "<submission-id>" "${shown[@]}"; echo
  if [ "$kind" = "dmg" ]; then
    printf '  %q' xcrun stapler staple "$file"; echo
    printf '  %q' xcrun stapler validate "$file"; echo
  else
    echo "  # a zip cannot be stapled: staple the app before zipping, or submit the DMG"
  fi
  echo "dry run OK"
  exit 0
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cmnotary.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
json_field() {
  python3 -I -c 'import json,sys
try: print(json.load(open(sys.argv[1])).get(sys.argv[2],""))
except Exception: print("")' "$1" "$2"
}

echo "==> submitting $file (this waits for Apple; up to $timeout)"
rc=0
xcrun notarytool submit "$file" --wait --timeout "$timeout" --output-format json "${cred[@]}" >"$tmp/submit.json" 2>"$tmp/submit.err" || rc=$?
id="$(json_field "$tmp/submit.json" id)"
status="$(json_field "$tmp/submit.json" status)"
# The JSON holds no secrets; stderr is printed with the password masked just in case.
if [ -s "$tmp/submit.err" ]; then
  if [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    python3 -I -c 'import sys; sys.stdout.write(sys.stdin.read().replace(sys.argv[1], "********"))' "$APPLE_APP_PASSWORD" <"$tmp/submit.err" >&2
  else
    cat "$tmp/submit.err" >&2
  fi
fi
echo "    submission id: ${id:-unknown}   status: ${status:-unknown}"

if [ "$status" != "Accepted" ]; then
  if [ -n "$id" ]; then
    echo "==> notarytool log $id" >&2
    xcrun notarytool log "$id" "${cred[@]}" >&2 || echo "notarize.sh: could not fetch the log" >&2
  else
    cat "$tmp/submit.json" >&2 || true
  fi
  die "notarization failed: status '${status:-none}' (notarytool exit $rc). The log above lists the issues (typical: a binary without the hardened runtime or a secure timestamp, or an unsigned or ad hoc binary)."
fi
[ "$rc" -eq 0 ] || die "notarytool returned $rc although the status is Accepted"

if [ "$kind" = "dmg" ]; then
  echo "==> stapling"
  xcrun stapler staple "$file" || die "stapler staple failed (the ticket may not be published yet: retry in a minute)"
  xcrun stapler validate "$file" || die "stapler validate failed"
  echo "notarized and stapled: $file"
else
  echo "notarized (zip: not stapled): $file"
fi
