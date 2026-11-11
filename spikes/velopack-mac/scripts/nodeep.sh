#!/bin/bash
# Does vpk keep our per-binary signatures/entitlements with --signDisableDeep? (real 0.3.1 layout, ad-hoc outer signature)
cd /tmp/cm-velo-mac || exit 1
rm -rf feedND ndunz
ENT=/Users/trungtran/source/repos/capturemann/.claude/worktrees/agent-a5b37077b3611f8ef/packaging/macos/CaptureMann.entitlements
./run.sh vpk pack --packId dev.capturemann.velorealspike --bundleId dev.capturemann.app --packVersion 0.3.1 --packDir real31/CaptureMann.app --mainExe cm-desktop --runtime osx-arm64 --channel osx-arm64 --outputDir feedND --signAppIdentity - --signDisableDeep --signEntitlements $ENT 2>&1 | grep -E "sign|Sign|ERR|rror"
mkdir ndunz && (cd ndunz && unzip -q ../feedND/dev.capturemann.velorealspike-osx-arm64-Portable.zip)
A=ndunz/dev.capturemann.app.app
[ -d "$A" ] || A=$(ls -d ndunz/*.app | head -1)
echo "app=$A"
echo "--- verify:"; codesign --verify --deep --strict "$A" 2>&1 | tail -3
for f in cm-desktop cm-overlay ffmpeg llama-server UpdateMac; do
  echo "--- $f"; codesign -dv "$A/Contents/MacOS/$f" 2>&1 | grep -E "^Identifier|Authority=Dev|flags|TeamIdentifier|Signature=" | tr '\n' ' '; echo
  codesign -d --entitlements - "$A/Contents/MacOS/$f" 2>&1 | grep -oE "com.apple.security[a-z.-]+" | tr '\n' ' '; echo
done
