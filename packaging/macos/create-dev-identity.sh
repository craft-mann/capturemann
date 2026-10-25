#!/usr/bin/env bash
# Creates a SELF-SIGNED code-signing certificate in your login keychain, for LOCAL DEVELOPMENT
# only. Signing CaptureMann.app with it (CM_SIGN_IDENTITY) gives the app a designated requirement
# that stays the same across rebuilds, so macOS keeps your Screen Recording grant.
#
# Never run by bundle.sh or make. It prints what it will do and asks before touching anything.
# It changes your keychain: it imports one certificate + private key (name below) and, only if
# you answer yes to the second question, marks the certificate as trusted for code signing
# (macOS shows its own password prompt for that).
#
# Usage:   packaging/macos/create-dev-identity.sh ["Certificate Name"]
set -euo pipefail
trap 'rc=$?; [ "$rc" -eq 0 ] || echo "create-dev-identity.sh: failed (exit $rc) near line $LINENO; nothing was left behind except possibly an imported certificate: check with security find-identity -p codesigning" >&2' ERR

die() { echo "create-dev-identity.sh: $*" >&2; exit 1; }
ask() { # ask <question>; returns 0 only on an explicit yes
  local reply
  read -r -p "$1 [y/N] " reply || return 1
  case "$reply" in y | Y | yes | YES) return 0 ;; *) return 1 ;; esac
}

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
name="${1:-CaptureMann Local Dev}"
openssl_bin="/usr/bin/openssl"   # the system LibreSSL writes PKCS#12 files that `security import` accepts
for tool in "$openssl_bin" security mktemp; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done
keychain="$(security default-keychain -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//')"
[ -n "$keychain" ] && [ -f "$keychain" ] || die "could not determine your default keychain"

if security find-certificate -c "$name" "$keychain" >/dev/null 2>&1; then
  die "a certificate named '$name' already exists in $keychain; nothing to do (use it with CM_SIGN_IDENTITY=\"$name\")"
fi

cat <<TXT
This will:
  1. generate a 2048-bit RSA key and a self-signed certificate "$name"
     (valid 10 years, extended key usage: code signing) in a temporary directory,
  2. import them into your keychain: $keychain
     (codesign is allowed to use the key; macOS may ask once for your keychain password,
      choose "Always Allow"),
  3. optionally mark the certificate as trusted for code signing (asks separately),
  4. delete the temporary files.
Nothing else on your system is changed. Remove it later in Keychain Access, or with:
  security delete-identity -c "$name" "$keychain"
TXT
ask "Create the certificate now?" || { echo "aborted, nothing changed."; exit 0; }

tmp="$(mktemp -d "${TMPDIR:-/tmp}/cm-dev-identity.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT
# Not `tr ... < /dev/urandom | head -c`: under `set -o pipefail` the SIGPIPE that head causes makes the
# pipeline fail and set -e silently ends the script.
pass="$("$openssl_bin" rand -hex 16)"

cat > "$tmp/openssl.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $name
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
"$openssl_bin" req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 \
  -config "$tmp/openssl.cnf" -keyout "$tmp/key.pem" -out "$tmp/cert.pem" 2>/dev/null
# The password goes to openssl on stdin, so it never shows in `ps`. `security import -P` only takes
# it from argv: it is visible in `ps` for the fraction of a second the import runs (random,
# single-use, protects a temp file that is deleted right after).
printf '%s' "$pass" | "$openssl_bin" pkcs12 -export -inkey "$tmp/key.pem" -in "$tmp/cert.pem" \
  -name "$name" -out "$tmp/id.p12" -passout stdin
security import "$tmp/id.p12" -k "$keychain" -P "$pass" -T /usr/bin/codesign >/dev/null
echo "imported \"$name\" into $keychain"

if ask "Trust it for code signing? (REQUIRED: codesign refuses an untrusted identity; macOS will ask for your password)"; then
  security add-trusted-cert -r trustRoot -p codeSign -k "$keychain" "$tmp/cert.pem"
  echo "trusted for code signing."
else
  echo "skipped trust. Signing will NOT work until the certificate is trusted for code signing."
  echo "To do it by hand: Keychain Access > login > Certificates > \"$name\" >"
  echo "Get Info > Trust > Code Signing: Always Trust."
fi

echo
echo "The first time codesign uses the key, macOS may ask to allow access to it: choose Always Allow"
echo "(this script does not run security set-key-partition-list, which needs your login password)."
echo "Check:  security find-identity -v -p codesigning     (\"$name\" must be listed as valid)"
echo "Use:    CM_SIGN_IDENTITY=\"$name\" make bundle"
