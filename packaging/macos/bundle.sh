#!/usr/bin/env bash
# Assembles target/bundle/CaptureMann.app so macOS privacy permissions (Screen Recording, later
# Camera, Microphone, Accessibility, Input Monitoring) attach to "CaptureMann" itself instead of
# the terminal that launched the binary.
#
# Usage (any directory):   packaging/macos/bundle.sh        or   make bundle
# Environment:
#   CM_APP_NAME       name of the .app and of the app in macOS (default CaptureMann; bundle-dev: CaptureMann-dev)
#   CM_BUNDLE_ID      bundle identifier (default dev.capturemann.app). The owner may change it
#                     before the first public release; it is part of the TCC identity, so changing
#                     it later makes users re-grant permissions.
#   CM_SIGN_IDENTITY  codesign identity (certificate common name or SHA-1). When unset, the
#                     identity "CaptureMann Local Dev" is used if it already exists in the keychain
#                     (made once by create-dev-identity.sh, which you run yourself); otherwise
#                     ad-hoc ("-"), whose signature changes every build and makes macOS drop the
#                     Screen Recording grant. The mode used is printed. This script never creates
#                     or changes keychain items. See packaging/macos/README.md.
#   CM_HARDENED=1     add the hardened runtime + secure timestamp (needed for notarization; use
#                     with a Developer ID identity, not ad-hoc).
#   CM_SKIP_BUILD=1   do not run cargo; reuse the binaries in <target>/release (CARGO_TARGET_DIR respected).
#   CM_BUILD_NUMBER   CFBundleVersion (digits and dots, default: the workspace version).
#   CM_BUNDLE_OUT     output directory (default: <target>/../bundle, i.e. target/bundle). Tests use a
#                     temp dir so the app you are running is never touched.
#   CM_FFMPEG_BUNDLE  path to an LGPL ffmpeg binary (ffprobe is taken from the same directory) to
#                     copy into Contents/MacOS and sign. Refused if its build is GPL / libx264 /
#                     nonfree unless CM_ALLOW_GPL_FFMPEG=1 (then a loud warning; never release it).
#                     Without it, packaging/ffmpeg/out/ffmpeg (made by `make ffmpeg`) is bundled
#                     when it exists. If the folder also holds LICENSES/, SOURCE-OFFER.txt and
#                     BUILD-INFO.txt they go to Contents/Resources/FFmpeg/ (the LGPL source offer).
#   CM_NO_FFMPEG=1    bundle no FFmpeg even when packaging/ffmpeg/out exists (a recording-only app).
#   CM_REQUIRE_FFMPEG=1  fail unless an LGPL FFmpeg with its licence files is bundled
#                     (`make bundle-release` sets it).
#   CM_LLAMA_BUNDLE   path to a llama-server binary (MIT; the helper of the built-in text engine) to
#                     copy into Contents/MacOS and sign. Without it, packaging/llama/out/llama-server
#                     (made by `make llama`) is bundled when it exists, with its licence files and
#                     build record in Contents/Resources/Llama/. Refused if it links curl or OpenSSL.
#   CM_NO_LLAMA=1     bundle no llama-server even when packaging/llama/out exists (the built-in text
#                     model then answers "engine missing"; Ollama and LM Studio still work).
#   CM_REQUIRE_LLAMA=1  fail unless llama-server with its licence files is bundled
#                     (`make bundle-release` sets it).
# Output: $CM_BUNDLE_OUT/CaptureMann.app. It is assembled and verified in a hidden temp folder next
# to it and swapped in with renames only after verification, so a failed build leaves the previous
# app intact. Entitlements: packaging/macos/CaptureMann.entitlements (camera, microphone).
set -euo pipefail

die() { echo "bundle.sh: $*" >&2; exit 1; }

[ "$(uname -s)" = "Darwin" ] || die "this script only runs on macOS"
for tool in codesign plutil sed; do
  command -v "$tool" >/dev/null 2>&1 || die "required tool not found: $tool"
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bundle_id="${CM_BUNDLE_ID:-dev.capturemann.app}"
app_name="${CM_APP_NAME:-CaptureMann}" # file and display name; make bundle-dev sets CaptureMann-dev
dev_identity="CaptureMann Local Dev"
if [ -n "${CM_SIGN_IDENTITY:-}" ]; then
  identity="$CM_SIGN_IDENTITY"
  sign_mode="explicit identity (CM_SIGN_IDENTITY)"
elif ids="$(security find-identity -v -p codesigning 2>/dev/null || true)"; grep -q -F "$dev_identity" <<<"$ids"; then
  # (captured first: `security ... | grep -q` would get SIGPIPE under pipefail and fall back to ad-hoc.)
  # Read-only lookup: nothing in the keychain is created or modified here.
  identity="$dev_identity"
  sign_mode="local dev identity found in the keychain"
else
  identity="-"
  sign_mode="ad-hoc (no \"$dev_identity\" identity in the keychain)"
fi
# Resolve cargo's target dir (honours CARGO_TARGET_DIR and .cargo/config), so the binaries we
# package are the ones cargo built.
command -v cargo >/dev/null 2>&1 || die "cargo not found"
command -v python3 >/dev/null 2>&1 || die "python3 not found"
target_dir="$(cd "$root" && cargo metadata --format-version 1 --no-deps --locked 2>/dev/null \
  | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')" \
  || die "could not resolve the cargo target directory"
[ -d "$target_dir" ] || mkdir -p "$target_dir"
out="${CM_BUNDLE_OUT:-$root/target/bundle}"
mkdir -p "$out" || die "cannot create $out"
out="$(cd "$out" && pwd)"
app="$out/$app_name.app"
stage="$out/.$app_name.app.tmp.$$"
old="$out/.$app_name.app.old.$$"
entitlements="$root/packaging/macos/CaptureMann.entitlements"
cleanup() { rm -rf "$stage" "$old"; }
trap cleanup EXIT
icon="$root/design/brand/app-icon/AppIcon.icns"
plist_tpl="$root/packaging/macos/Info.plist"

case "$bundle_id" in
  *[!A-Za-z0-9.-]* | "") die "CM_BUNDLE_ID may only contain letters, digits, '.' and '-': '$bundle_id'" ;;
esac
case "$app_name" in
  *[!A-Za-z0-9.-]* | "") die "CM_APP_NAME may only contain letters, digits, '.' and '-': '$app_name'" ;;
esac

[ -f "$icon" ] || die "icon not found: $icon"
[ -f "$plist_tpl" ] || die "Info.plist template not found: $plist_tpl"
[ -f "$entitlements" ] || die "entitlements not found: $entitlements"
plutil -lint "$entitlements" >/dev/null || die "invalid entitlements plist"
for f in LICENSE NOTICE THIRD_PARTY_LICENSES.md; do
  [ -f "$root/$f" ] || die "missing $f (run: make licenses)"
done

# Workspace version: the first `version = "..."` after [workspace.package].
version="$(awk '/^\[workspace\.package\]/{f=1;next} /^\[/{f=0} f && /^version *=/{gsub(/^[^"]*"|".*$/,""); print; exit}' "$root/Cargo.toml")"
[ -n "$version" ] || die "could not read the workspace version from Cargo.toml"
build_number="${CM_BUILD_NUMBER:-$version}"
# Both end up in a sed replacement and in Info.plist: digits and dots only.
case "$version" in *[!0-9.]* | "") die "workspace version must be digits and dots: '$version'" ;; esac
case "$build_number" in *[!0-9.]* | "") die "CM_BUILD_NUMBER must be digits and dots: '$build_number'" ;; esac

if [ "${CM_SKIP_BUILD:-0}" != "1" ]; then
  echo "==> cargo build --release (target dir $target_dir)"
  (cd "$root" && cargo build --release --locked --target-dir "$target_dir" -p cm-desktop -p cm-overlay -p cm-cli -p cm-mcp)
fi

# source binary in target/release -> name inside Contents/MacOS
bins="cm-desktop:cm-desktop cm-overlay:cm-overlay capturemann:capturemann cm-mcp:cm-mcp"
for pair in $bins; do
  [ -x "$target_dir/release/${pair%%:*}" ] || die "missing $target_dir/release/${pair%%:*} (run without CM_SKIP_BUILD)"
done

# Everything below happens in $stage; $app is only replaced at the very end.
echo "==> assembling $app (id $bundle_id, version $version) in $stage"
app_final="$app"
app="$stage"
rm -rf "$stage"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
for pair in $bins; do
  # Binaries keep their symbols (no `strip`) so backtraces stay readable.
  cp "$target_dir/release/${pair%%:*}" "$app/Contents/MacOS/${pair##*:}"
done
cp "$root/LICENSE" "$root/NOTICE" "$root/THIRD_PARTY_LICENSES.md" "$app/Contents/Resources/"

# LGPL FFmpeg (NFR-12 / NFR-15). Nothing is downloaded here: CM_FFMPEG_BUNDLE, or the output of
# `make ffmpeg` (packaging/ffmpeg/build-macos.sh), is copied in.
ffmpeg_bundled=0
ffmpeg_src="${CM_FFMPEG_BUNDLE:-}"
if [ -z "$ffmpeg_src" ] && [ "${CM_NO_FFMPEG:-0}" != "1" ] && [ -x "$root/packaging/ffmpeg/out/ffmpeg" ]; then
  ffmpeg_src="$root/packaging/ffmpeg/out/ffmpeg"
  echo "==> bundling the FFmpeg built by make ffmpeg ($ffmpeg_src)"
fi
if [ -z "$ffmpeg_src" ] && [ "${CM_REQUIRE_FFMPEG:-0}" = "1" ]; then
  die "CM_REQUIRE_FFMPEG=1 but there is no FFmpeg to bundle: run \`make ffmpeg\` (or set CM_FFMPEG_BUNDLE to an LGPL build)"
fi
if [ -n "$ffmpeg_src" ]; then
  ff="$ffmpeg_src"
  [ -x "$ff" ] && [ -f "$ff" ] || die "the FFmpeg to bundle is not an executable file: $ff"
  ff_info="$("$ff" -hide_banner -version 2>&1 || true)"
  ff_conf="$("$ff" -hide_banner -buildconf 2>&1 || true)"
  if grep -q -E -e '--enable-(gpl|nonfree|libx264|libx265|libfdk-aac)' <<<"$ff_info
$ff_conf"; then
    if [ "${CM_ALLOW_GPL_FFMPEG:-0}" = "1" ]; then
      echo "bundle.sh: WARNING: bundling a GPL/nonfree FFmpeg (CM_ALLOW_GPL_FFMPEG=1). This build MUST NOT be distributed (NFR-12)." >&2
    else
      die "$ff was built with --enable-gpl/nonfree/libx264/libx265/libfdk-aac: not allowed in a shipped bundle (NFR-12). Use an LGPL build (\`make ffmpeg\`), or CM_ALLOW_GPL_FFMPEG=1 for a local-only bundle."
    fi
  fi
  cp "$ff" "$app/Contents/MacOS/ffmpeg"
  # The licence texts and the source offer travel with the binary (LGPL).
  ff_dir="$(dirname "$ff")"
  if [ -f "$ff_dir/SOURCE-OFFER.txt" ] && [ -d "$ff_dir/LICENSES" ]; then
    mkdir -p "$app/Contents/Resources/FFmpeg"
    cp "$ff_dir/SOURCE-OFFER.txt" "$app/Contents/Resources/FFmpeg/"
    [ ! -f "$ff_dir/BUILD-INFO.txt" ] || cp "$ff_dir/BUILD-INFO.txt" "$app/Contents/Resources/FFmpeg/"
    cp -R "$ff_dir/LICENSES" "$app/Contents/Resources/FFmpeg/LICENSES"
  elif [ "${CM_REQUIRE_FFMPEG:-0}" = "1" ]; then
    die "$ff_dir has no SOURCE-OFFER.txt and LICENSES/: a release must ship the LGPL notices (build it with \`make ffmpeg\`)"
  else
    echo "bundle.sh: warning: no SOURCE-OFFER.txt / LICENSES next to $ff; this bundle must not be distributed" >&2
  fi
  ffprobe_src="$ff_dir/ffprobe"
  if [ -x "$ffprobe_src" ]; then
    cp "$ffprobe_src" "$app/Contents/MacOS/ffprobe"
  else
    echo "bundle.sh: warning: no ffprobe next to $ff; bundling ffmpeg only" >&2
  fi
  ffmpeg_bundled=1
fi
# llama-server (MIT), the helper process of the built-in text engine (AI-7). It is a separate
# program next to the app's executables, never linked into the app. Nothing is downloaded here:
# CM_LLAMA_BUNDLE, or the output of `make llama` (packaging/llama/build-macos.sh), is copied in.
llama_bundled=0
llama_src="${CM_LLAMA_BUNDLE:-}"
if [ "${CM_REQUIRE_LLAMA:-0}" = "1" ] && [ "${CM_NO_LLAMA:-0}" = "1" ]; then
  die "CM_REQUIRE_LLAMA=1 and CM_NO_LLAMA=1 contradict each other"
fi
if [ -z "$llama_src" ] && [ "${CM_NO_LLAMA:-0}" != "1" ] && [ -x "$root/packaging/llama/out/llama-server" ]; then
  llama_src="$root/packaging/llama/out/llama-server"
  echo "==> bundling the llama-server built by make llama ($llama_src)"
fi
if [ -z "$llama_src" ] && [ "${CM_REQUIRE_LLAMA:-0}" = "1" ]; then
  die "CM_REQUIRE_LLAMA=1 but there is no llama-server to bundle: run \`make llama\` (or set CM_LLAMA_BUNDLE)"
fi
if [ -n "$llama_src" ]; then
  [ -x "$llama_src" ] && [ -f "$llama_src" ] || die "the llama-server to bundle is not an executable file: $llama_src"
  if otool -L "$llama_src" 2>/dev/null | grep -q -i -E 'curl|ssl|crypto'; then
    die "$llama_src links curl or OpenSSL: the text engine must not be able to reach the network by itself"
  fi
  cp "$llama_src" "$app/Contents/MacOS/llama-server"
  llama_dir="$(dirname "$llama_src")"
  if [ -d "$llama_dir/LICENSES" ] && [ -f "$llama_dir/LICENSES/llama.cpp-LICENSE.txt" ]; then
    mkdir -p "$app/Contents/Resources/Llama"
    [ ! -f "$llama_dir/BUILD-INFO.txt" ] || cp "$llama_dir/BUILD-INFO.txt" "$app/Contents/Resources/Llama/"
    cp -R "$llama_dir/LICENSES" "$app/Contents/Resources/Llama/LICENSES"
  elif [ "${CM_REQUIRE_LLAMA:-0}" = "1" ]; then
    die "$llama_dir has no LICENSES/llama.cpp-LICENSE.txt: a release must ship the MIT notice (build it with \`make llama\`)"
  else
    echo "bundle.sh: warning: no LICENSES next to $llama_src; this bundle must not be distributed" >&2
  fi
  llama_bundled=1
fi
sed -e "s|@BUNDLE_ID@|$bundle_id|g" -e "s|@VERSION@|$version|g" -e "s|@BUILD@|$build_number|g" -e "s|@APP_NAME@|$app_name|g" \
  "$plist_tpl" > "$app/Contents/Info.plist"
grep -q -E '@(BUNDLE_ID|VERSION|BUILD|APP_NAME)@' "$app/Contents/Info.plist" && die "unfilled @TOKEN@ left in Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null || die "generated Info.plist is invalid"
cp "$icon" "$app/Contents/Resources/AppIcon.icns"
printf 'APPL????' > "$app/Contents/PkgInfo"

# --- Code signing: inner binaries first (each with a fixed identifier), then the bundle. ---
opts=()
if [ "${CM_HARDENED:-0}" = "1" ]; then
  [ "$identity" != "-" ] || echo "bundle.sh: warning: CM_HARDENED with an ad-hoc identity is only useful for testing" >&2
  opts+=(--options runtime --timestamp)
fi
if [ "$identity" = "-" ]; then
  echo "==> signing mode: $sign_mode"
  echo "    The ad-hoc signature changes every build, so macOS may drop the Screen Recording grant."
  echo "    Stable alternative: run packaging/macos/create-dev-identity.sh once, then rebuild."
else
  echo "==> signing mode: $sign_mode"
  echo "==> signing with identity: $identity"
fi
sign() { # sign <path> <identifier> [extra codesign args...]
  # The linker already ad-hoc signs the Rust binaries, hence the filtered "replacing" notice.
  local path="$1" ident="$2"; shift 2
  codesign --force --sign "$identity" --identifier "$ident" ${opts[@]+"${opts[@]}"} "$@" "$path" \
    2> >(grep -v 'replacing existing signature' >&2)
}
sign "$app/Contents/MacOS/cm-overlay" "$bundle_id.overlay"
sign "$app/Contents/MacOS/capturemann" "$bundle_id.cli"
sign "$app/Contents/MacOS/cm-mcp" "$bundle_id.mcp"
if [ "$ffmpeg_bundled" = "1" ]; then
  sign "$app/Contents/MacOS/ffmpeg" "$bundle_id.ffmpeg"
  [ ! -f "$app/Contents/MacOS/ffprobe" ] || sign "$app/Contents/MacOS/ffprobe" "$bundle_id.ffprobe"
fi
if [ "$llama_bundled" = "1" ]; then
  sign "$app/Contents/MacOS/llama-server" "$bundle_id.llama"
fi
# Entitlements go on the main executable and again on the bundle (signing the bundle re-signs the
# main executable, and would otherwise drop them). Also applied without the hardened runtime so
# dev builds take the same path.
sign "$app/Contents/MacOS/cm-desktop" "$bundle_id" --entitlements "$entitlements"
sign "$app" "$bundle_id" --entitlements "$entitlements"
codesign --verify --deep --strict "$app" || die "signature verification failed"

# Swap: rename the old app aside, rename the new one in, delete the old. A failure before this
# point leaves the previous app untouched; a failure of the second rename restores it.
app="$app_final"
if [ -e "$app" ]; then
  mv "$app" "$old" || die "could not move the existing app aside"
fi
if ! mv "$stage" "$app"; then
  [ ! -e "$old" ] || mv "$old" "$app"
  die "could not move the new app into place; the previous app was restored"
fi
rm -rf "$old"

echo "built $app"
codesign -d -r- "$app" 2>&1 | sed -n 's/^# designated => /designated requirement: /p;s/^designated => /designated requirement: /p'
