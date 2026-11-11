#!/bin/bash
cd /tmp/cm-velo-mac || exit 1
rm -rf dmgsrc test.dmg dmgmnt dmgout
mkdir -p dmgsrc dmgmnt "dmgout/my apps"
ditto inst1/dev.capturemann.velospike.app dmgsrc/VeloSpike.app
hdiutil create -volname VeloSpike -srcfolder dmgsrc -format UDZO -quiet test.dmg
hdiutil attach -readonly -nobrowse -mountpoint /tmp/cm-velo-mac/dmgmnt test.dmg -quiet
ditto dmgmnt/VeloSpike.app "dmgout/my apps/CaptureMann Spike.app"
hdiutil detach dmgmnt -quiet
pkill -f "VeloSpike.app"
rm -rf ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
"dmgout/my apps/CaptureMann Spike.app/Contents/MacOS/velo-spike" update http://127.0.0.1:8099 > /dev/null 2>&1
sleep 3
grep -E "update available|after-update:|installed" logs/run.log | cut -c1-200
ls "dmgout/my apps"
ls -l test.dmg | awk '{print $5}'
codesign --verify --deep --strict "dmgout/my apps/CaptureMann Spike.app" && echo "signature valid after update in renamed/moved app"
defaults read "$PWD/dmgout/my apps/CaptureMann Spike.app/Contents/Info" CFBundleVersion
