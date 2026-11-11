#!/bin/bash
# usage: mkapp.sh <n> [sign-identity]  -> pack/app<n>/VeloSpike.app, ad-hoc signed (identity "-") unless "none"
cd /tmp/cm-velo-mac || exit 1
v=$1; id=${2:--}
d=pack/app$v/VeloSpike.app
rm -rf pack/app$v; mkdir -p $d/Contents/MacOS $d/Contents/Resources
cp bins/velo-spike-0.3.2-spike$v $d/Contents/MacOS/velo-spike
cat > $d/Contents/Info.plist <<PL
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>VeloSpike</string>
<key>CFBundleIdentifier</key><string>dev.capturemann.velospike</string>
<key>CFBundleExecutable</key><string>velo-spike</string>
<key>CFBundleVersion</key><string>0.3.2.$v</string>
<key>CFBundleShortVersionString</key><string>0.3.2</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
</dict></plist>
PL
echo "payload for version $v" > $d/Contents/Resources/note.txt
[ "$id" = none ] || codesign --force --deep --options runtime -s "$id" $d
