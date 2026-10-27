#!/usr/bin/env bash
# Hand-run test of the release scripts (not run in CI). Everything is tested with fakes and a
# throwaway self-signed identity in a throwaway keychain; a real Developer ID import and a real
# notarization cannot be tested here.
#
# Usage:  packaging/macos/tests/release-scripts.sh
# Needs the release binaries in target/release (cargo build --release for cm-desktop, cm-overlay,
# cm-cli, cm-mcp; bundle.sh is run with CM_SKIP_BUILD=1, CM_NO_FFMPEG=1 and CM_NO_LLAMA=1). Never touches the login
# keychain, except that the throwaway keychain is put in the user search list for a moment and the
# list is restored (and compared) at the end. Set CM_TEST_KEEP=1 to keep the temp folder.
set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
mac="$root/packaging/macos"
work="$(mktemp -d "$root/.cmrel-test.XXXXXX")"
pass=0; fail=0
orig_list="$(security list-keychains -d user)"
kc=""

cleanup() {
  [ -z "$kc" ] || security delete-keychain "$kc" >/dev/null 2>&1 || true
  # restore the search list whatever happened
  restore=()
  while IFS= read -r l; do l="${l#"${l%%[![:space:]]*}"}"; l="${l#\"}"; l="${l%\"}"; [ -z "$l" ] || restore+=("$l"); done <<<"$orig_list"
  security list-keychains -d user -s "${restore[@]}" >/dev/null 2>&1 || true
  hdiutil detach "$work/mnt" -force -quiet >/dev/null 2>&1 || true
  if [ "${CM_TEST_KEEP:-0}" = "1" ]; then echo "kept: $work"; else rm -rf "$work"; fi
}
trap cleanup EXIT

check() { # check "name" command...   (PASS when the command exits 0)
  local name="$1"; shift
  # A follow-up (name starting with two spaces) reads the output of the last primary check
  # from prev.out and does not replace it.
  local last="$work/last.out"
  [[ "$name" == "  "* ]] && last="$work/scratch.out"
  if "$@" >"$last" 2>&1; then echo "PASS  $name"; pass=$((pass + 1)); else echo "FAIL  $name"; sed 's/^/        /' "$last" | tail -n 12; fail=$((fail + 1)); fi
  [ "$last" = "$work/last.out" ] && cp -f "$last" "$work/prev.out"
}
check_fail() { # PASS when the command exits non-zero
  local name="$1"; shift
  # A follow-up (name starting with two spaces) reads the output of the last primary check
  # from prev.out and does not replace it.
  local last="$work/last.out"
  [[ "$name" == "  "* ]] && last="$work/scratch.out"
  if "$@" >"$last" 2>&1; then echo "FAIL  $name (expected a failure)"; sed 's/^/        /' "$last" | tail -n 8; fail=$((fail + 1)); else echo "PASS  $name"; pass=$((pass + 1)); fi
  [ "$last" = "$work/last.out" ] && cp -f "$last" "$work/prev.out"
}
out_has() { grep -q -F -e "$1" "$work/prev.out"; }
out_lacks() { ! grep -q -F -e "$1" "$work/prev.out"; }

echo "== work dir $work"

# --- 1. an ad hoc bundle from the already built release binaries ---
export CM_BUNDLE_OUT="$work/bundle"
check "bundle.sh builds an ad hoc app from target/release" \
  env CM_SKIP_BUILD=1 CM_NO_FFMPEG=1 CM_NO_LLAMA=1 CM_SIGN_IDENTITY=- bash "$mac/bundle.sh"
app="$work/bundle/CaptureMann.app"
unset CM_BUNDLE_OUT

# --- 1b. llama-server (the built-in text engine's helper): bundled, signed, licence travels ---
fake_llama="$work/fake-llama"
mkdir -p "$fake_llama/LICENSES"
printf '#!/bin/sh\necho llama-server fake\n' > "$fake_llama/llama-server"
chmod +x "$fake_llama/llama-server"
printf 'MIT License\n' > "$fake_llama/LICENSES/llama.cpp-LICENSE.txt"
llama_app="$work/llama-bundle/CaptureMann.app"
check "bundle.sh bundles a llama-server with its licence" \
  env CM_BUNDLE_OUT="$work/llama-bundle" CM_SKIP_BUILD=1 CM_NO_FFMPEG=1 CM_LLAMA_BUNDLE="$fake_llama/llama-server" CM_SIGN_IDENTITY=- bash "$mac/bundle.sh"
check "  ... llama-server is in Contents/MacOS and signed" \
  bash -c "[ -x '$llama_app/Contents/MacOS/llama-server' ] && codesign --verify '$llama_app/Contents/MacOS/llama-server'"
check "  ... the MIT licence is in Resources/Llama" test -f "$llama_app/Contents/Resources/Llama/LICENSES/llama.cpp-LICENSE.txt"
check_fail "bundle.sh CM_REQUIRE_LLAMA fails when there is none" \
  env CM_BUNDLE_OUT="$work/llama-none" CM_SKIP_BUILD=1 CM_NO_FFMPEG=1 CM_REQUIRE_LLAMA=1 CM_SIGN_IDENTITY=- bash "$mac/bundle.sh"
check "  ... and says what to run" out_has "make llama"

# --- 2. dmg.sh ---
dmg="$work/out/CaptureMann-test.dmg"
check "dmg.sh builds an unsigned DMG" env -u CM_SIGN_IDENTITY CM_DMG_VOLNAME="CaptureMann Test" bash "$mac/dmg.sh" "$app" "$dmg"
check "dmg.sh printed path and sha256" bash -c "grep -q '^sha256: [0-9a-f]\{64\}$' '$work/prev.out'"
check "the .sha256 file matches the DMG" bash -c "cd '$work/out' && shasum -a 256 -c CaptureMann-test.dmg.sha256"
check "the DMG is UDZO and read-only" bash -c "hdiutil imageinfo '$dmg' | grep -q 'Format: UDZO'"
mkdir -p "$work/mnt"
check "the DMG mounts read-only" hdiutil attach -readonly -nobrowse -noautoopen -noverify -mountpoint "$work/mnt" "$dmg"
check "DMG holds the app and the Applications symlink, volume name set" bash -c "[ -d '$work/mnt/CaptureMann.app/Contents/MacOS' ] && [ \"\$(readlink '$work/mnt/Applications')\" = /Applications ] && diskutil info '$work/mnt' | grep -q 'Volume Name: *CaptureMann Test'"
check "the app inside still verifies after ditto" codesign --verify --deep --strict "$work/mnt/CaptureMann.app"
hdiutil detach "$work/mnt" -quiet >/dev/null 2>&1
check_fail "dmg.sh rejects a non-app" bash "$mac/dmg.sh" "$work" "$work/x.dmg"
check_fail "dmg.sh rejects a wrong extension" bash "$mac/dmg.sh" "$app" "$work/x.img"
check_fail "dmg.sh rejects a bad volume name" env CM_DMG_VOLNAME='a;b' bash "$mac/dmg.sh" "$app" "$work/x.dmg"

# --- 3. verify-release.sh ---
check "verify-release --allow-unsigned passes on the ad hoc app" bash "$mac/verify-release.sh" --allow-unsigned "$app"
check "  ... and reports the Gatekeeper warning" out_has "WARNING"
check_fail "verify-release without the flag fails on the ad hoc app" bash "$mac/verify-release.sh" "$app"
check "verify-release --allow-unsigned passes on the DMG and unmounts it" bash "$mac/verify-release.sh" --allow-unsigned "$dmg"
check "  ... nothing from verify-release is left mounted" bash -c "! mount | grep -q cmverify"
check_fail "verify-release flags a wrong version" env CM_EXPECT_VERSION=9.9.9 bash "$mac/verify-release.sh" --allow-unsigned "$app"
check_fail "verify-release rejects a missing file" bash "$mac/verify-release.sh" "$work/none.dmg"

# a GPL ffmpeg in the bundle must fail the LGPL check
gpl_app="$work/gpl/CaptureMann.app"
mkdir -p "$work/gpl"
ditto "$app" "$gpl_app"
cat > "$gpl_app/Contents/MacOS/ffmpeg" <<'FF'
#!/bin/sh
echo "ffmpeg version fake"
echo "configuration: --enable-gpl --enable-libx264"
FF
chmod +x "$gpl_app/Contents/MacOS/ffmpeg"
codesign --force --sign - "$gpl_app/Contents/MacOS/ffmpeg" >/dev/null 2>&1
codesign --force --sign - --entitlements "$mac/CaptureMann.entitlements" "$gpl_app" >/dev/null 2>&1
check_fail "verify-release fails on a GPL ffmpeg" bash "$mac/verify-release.sh" --allow-unsigned "$gpl_app"
check "  ... with the GPL message" out_has "enable-gpl"

# --- 4. notarize.sh --dry-run ---
printf 'not a real key' > "$work/fake.p8"
check "notarize --dry-run with an API key" env APPLE_API_KEY_ID=KEY123 APPLE_API_ISSUER=issuer-uuid APPLE_API_KEY_PATH="$work/fake.p8" bash "$mac/notarize.sh" --dry-run "$dmg"
check "  ... prints the submit and staple commands" bash -c "grep -q 'notarytool *submit' '$work/prev.out' && grep -q 'stapler *staple' '$work/prev.out' && grep -q 'stapler *validate' '$work/prev.out'"
check "  ... does not read the key file into the output" out_lacks "not a real key"
check "notarize --dry-run with Apple ID credentials masks the password" env -u APPLE_API_KEY_ID APPLE_ID=a@example.com APPLE_TEAM_ID=TEAM123456 APPLE_APP_PASSWORD=sekrit-pass-word bash "$mac/notarize.sh" --dry-run "$dmg"
check "  ... the password is not printed" out_lacks "sekrit-pass-word"
check_fail "notarize with no credentials fails" env -i PATH="$PATH" HOME="$HOME" bash "$mac/notarize.sh" --dry-run "$dmg"
check "  ... and says what to set" out_has "APPLE_API_KEY_ID"
check_fail "notarize with half an Apple ID fails" env -i PATH="$PATH" HOME="$HOME" APPLE_ID=a@example.com bash "$mac/notarize.sh" --dry-run "$dmg"
check "  ... and names what is missing" out_has "APPLE_TEAM_ID"
check_fail "notarize with a missing .p8 fails" env -i PATH="$PATH" HOME="$HOME" APPLE_API_KEY_ID=K APPLE_API_ISSUER=I APPLE_API_KEY_PATH="$work/none.p8" bash "$mac/notarize.sh" --dry-run "$dmg"
check_fail "notarize rejects a .app" env APPLE_API_KEY_ID=K APPLE_API_ISSUER=I APPLE_API_KEY_PATH="$work/fake.p8" bash "$mac/notarize.sh" --dry-run "$app"
check_fail "notarize rejects a missing file" env APPLE_API_KEY_ID=K APPLE_API_ISSUER=I APPLE_API_KEY_PATH="$work/fake.p8" bash "$mac/notarize.sh" --dry-run "$work/none.dmg"
check "notarize --check-env needs no file" env APPLE_API_KEY_ID=K APPLE_API_ISSUER=I APPLE_API_KEY_PATH="$work/fake.p8" bash "$mac/notarize.sh" --check-env

# --- 5. ci-keychain.sh with a throwaway self-signed identity ---
cn="CM Release Test Throwaway"
cat > "$work/cert.cnf" <<EOF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $cn
[ext]
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
basicConstraints = critical, CA:false
EOF
/usr/bin/openssl req -x509 -newkey rsa:2048 -nodes -days 2 -config "$work/cert.cnf" -keyout "$work/key.pem" -out "$work/cert.pem" >/dev/null 2>&1
/usr/bin/openssl pkcs12 -export -inkey "$work/key.pem" -in "$work/cert.pem" -out "$work/test.p12" -passout pass:pw-for-test >/dev/null 2>&1
check "throwaway p12 was made" test -s "$work/test.p12"
p12b64="$(base64 < "$work/test.p12" | tr -d '\n')"
envfile="$work/github_env"; : > "$envfile"
ci_env=(CM_CI_KEYCHAIN_DIR="$work/kcdir" CM_CI_IDENTITY_MATCH="$cn" CM_CI_ALLOW_UNTRUSTED=1 GITHUB_ENV="$envfile")

check_fail "ci-keychain setup with a wrong p12 password fails" env "${ci_env[@]}" APPLE_CERT_P12_BASE64="$p12b64" APPLE_CERT_PASSWORD=wrong bash "$mac/ci-keychain.sh" setup
check "  ... and leaves no keychain behind" bash -c "! ls '$work/kcdir'/cmkc.*/*.keychain-db >/dev/null 2>&1"
check "  ... and the search list is unchanged" bash -c "[ \"\$(security list-keychains -d user)\" = \"\$1\" ]" _ "$orig_list"
check_fail "ci-keychain setup without APPLE_CERT_P12_BASE64 fails" env -u APPLE_CERT_P12_BASE64 "${ci_env[@]}" APPLE_CERT_PASSWORD=x bash "$mac/ci-keychain.sh" setup
check "ci-keychain setup imports the throwaway identity" env "${ci_env[@]}" APPLE_CERT_P12_BASE64="$p12b64" APPLE_CERT_PASSWORD=pw-for-test bash "$mac/ci-keychain.sh" setup
check "  ... masks the secrets and prints none" bash -c "grep -q '^::add-mask::' '$work/prev.out' && ! grep -q 'pw-for-test' <(grep -v add-mask '$work/prev.out')"
check "  ... appended CM_SIGN_IDENTITY and CM_KEYCHAIN to GITHUB_ENV" bash -c "grep -q '^CM_SIGN_IDENTITY=[0-9A-F]\{40\}$' '$envfile' && grep -q '^CM_KEYCHAIN=.*keychain-db$' '$envfile'"
sign_id="$(sed -n 's/^CM_SIGN_IDENTITY=//p' "$envfile")"
kc="$(sed -n 's/^CM_KEYCHAIN=//p' "$envfile")"
check "  ... the keychain is in the search list" bash -c "security list-keychains -d user | grep -q cmkc"

signed_dmg="$work/out/signed.dmg"
check "dmg.sh signs the DMG with the throwaway identity" env CM_SIGN_IDENTITY="$sign_id" CM_KEYCHAIN="$kc" CM_DMG_NO_TIMESTAMP=1 bash "$mac/dmg.sh" "$app" "$signed_dmg"
check "  ... codesign --verify on the DMG" codesign --verify --strict "$signed_dmg"
check "  ... the signature is not ad hoc" bash -c "! codesign -dvv '$signed_dmg' 2>&1 | grep -q 'Signature=adhoc'"

hard="$work/hard"
check "bundle.sh CM_HARDENED with the throwaway identity" env CM_BUNDLE_OUT="$hard" CM_SKIP_BUILD=1 CM_NO_FFMPEG=1 CM_NO_LLAMA=1 CM_HARDENED=1 CM_SIGN_IDENTITY="$sign_id" bash "$mac/bundle.sh"
if [ -d "$hard/CaptureMann.app" ]; then
  check "verify-release --allow-unsigned on the hardened app" bash "$mac/verify-release.sh" --allow-unsigned "$hard/CaptureMann.app"
  check "  ... hardened runtime and timestamp are recognised on every binary" bash -c "grep -c 'hardened runtime' '$work/prev.out' | grep -qv '^0$' && grep -q 'secure timestamp' '$work/prev.out'"
  check "  ... the signature is reported as not Developer ID" out_has "kind=other"
  check_fail "verify-release without the flag fails (no Developer ID, no ticket)" bash "$mac/verify-release.sh" "$hard/CaptureMann.app"
fi

check "ci-keychain teardown" env CM_KEYCHAIN="$kc" bash "$mac/ci-keychain.sh" teardown
check "  ... the keychain file is gone" bash -c "[ ! -e '$kc' ]"
check "  ... the search list is restored" bash -c "[ \"\$(security list-keychains -d user)\" = \"\$1\" ]" _ "$orig_list"
check "teardown twice is harmless" env CM_KEYCHAIN="$kc" bash "$mac/ci-keychain.sh" teardown
kc=""

echo
echo "passed: $pass   failed: $fail"
[ "$fail" -eq 0 ]
