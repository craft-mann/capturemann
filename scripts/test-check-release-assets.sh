#!/usr/bin/env bash
# Self-test of scripts/check-release-assets.sh on made-up release folders (no network).
#   scripts/test-check-release-assets.sh
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
check="$here/check-release-assets.sh"
work="$(mktemp -d)"; trap 'rm -rf "$work"' EXIT
v=9.9.9
fail=0
ok()   { echo "ok   $1"; }
nok()  { echo "FAIL $1"; fail=1; }
sha()  { if command -v sha256sum >/dev/null; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi; }

# A complete macOS release in a folder: two DMGs, their .sha256 and SHA256SUMS.txt.
mk_mac() {
  local d="$1"; mkdir -p "$d"
  for a in arm64 x86_64; do
    local n="CaptureMann-v$v-macos-$a.dmg"
    echo "dmg $a" > "$d/$n"
    echo "$(sha "$d/$n")  $n" > "$d/$n.sha256"
  done
  (cd "$d" && for f in CaptureMann-v$v-macos-*.dmg; do echo "$(sha "$f")  $f"; done > SHA256SUMS.txt)
}
cask="$work/capturemann.rb"
cat > "$cask" <<'EOF'
cask "capturemann" do
  arch arm: "arm64", intel: "x86_64"
  version "0.0.1"
  url "https://github.com/craft-mann/capturemann/releases/download/v#{version}/CaptureMann-v#{version}-macos-#{arch}.dmg"
end
EOF

mk_mac "$work/good"
"$check" "$v" "$work/good" --cask "$cask" >/dev/null 2>&1 && ok "a complete macOS release passes" || nok "a complete macOS release passes"

# The make-release name (CaptureMann-<v>-<arch>.dmg) is refused.
mk_mac "$work/wrongname"; mv "$work/wrongname/CaptureMann-v$v-macos-arm64.dmg" "$work/wrongname/CaptureMann-$v-arm64.dmg"
"$check" "$v" "$work/wrongname" >/dev/null 2>&1 && nok "a wrong DMG name fails" || ok "a wrong DMG name fails"

# A missing Intel DMG (system present but incomplete).
mk_mac "$work/missing"; rm "$work/missing/CaptureMann-v$v-macos-x86_64.dmg" "$work/missing/CaptureMann-v$v-macos-x86_64.dmg.sha256"
"$check" "$v" "$work/missing" >/dev/null 2>&1 && nok "a missing DMG fails" || ok "a missing DMG fails"

# A checksum that does not match.
mk_mac "$work/badsum"; echo "tampered" >> "$work/badsum/CaptureMann-v$v-macos-arm64.dmg"
"$check" "$v" "$work/badsum" >/dev/null 2>&1 && nok "a wrong checksum fails" || ok "a wrong checksum fails"

# SHA256SUMS.txt that lacks a file.
mk_mac "$work/sums"; grep -v x86_64 "$work/sums/SHA256SUMS.txt" > "$work/sums.tmp"; cat "$work/sums.tmp" > "$work/sums/SHA256SUMS.txt"
"$check" "$v" "$work/sums" >/dev/null 2>&1 && nok "an incomplete SHA256SUMS.txt fails" || ok "an incomplete SHA256SUMS.txt fails"

# Names only (the draft release's asset list), and the cask's url pattern against a renamed asset.
ls "$work/good" > "$work/names.txt"
"$check" "$v" --names "$work/names.txt" --cask "$cask" >/dev/null 2>&1 && ok "names-only passes" || nok "names-only passes"
sed 's/-macos-x86_64/-mac-intel/' "$work/names.txt" > "$work/names2.txt"
"$check" "$v" --names "$work/names2.txt" --cask "$cask" >/dev/null 2>&1 && nok "a name the cask cannot build fails" || ok "a name the cask cannot build fails"

# Linux needs its .minisig files.
lin="$work/lin"; mkdir -p "$lin"
for n in capturemann-$v-linux-x86_64.tar.gz capturemann-$v-linux-aarch64.tar.gz capturemann_${v}_amd64.deb capturemann_${v}_arm64.deb; do
  echo "x $n" > "$lin/$n"; echo "$(sha "$lin/$n")  $n" > "$lin/$n.sha256"
  echo sig > "$lin/$n.minisig"; echo sig > "$lin/$n.sha256.minisig"
done
(cd "$lin" && for f in capturemann-*.tar.gz capturemann_*.deb; do echo "$(sha "$f")  $f"; done > SHA256SUMS.txt)
"$check" "$v" "$lin" >/dev/null 2>&1 && ok "a complete Linux release passes" || nok "a complete Linux release passes"
rm "$lin/capturemann_${v}_arm64.deb.minisig"
"$check" "$v" "$lin" >/dev/null 2>&1 && nok "a missing .minisig fails" || ok "a missing .minisig fails"

exit "$fail"
