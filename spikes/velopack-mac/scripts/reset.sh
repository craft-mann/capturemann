#!/bin/bash
# full rebuild of the 3 versions, fresh feed, fresh install of spike1 in Apps/
cd /tmp/cm-velo-mac || exit 1
pkill -f "VeloSpike.app" ; pkill -f "sleep 600"
rm -rf Apps inst1 feed feed12 pack ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
for v in 1 2 3; do ./build.sh 0.3.2-spike$v > /dev/null; done
./packv.sh 1 > /dev/null
mkdir -p inst1 Apps && (cd inst1 && unzip -q -o ../feed/dev.capturemann.velospike-osx-arm64-Portable.zip) && cp -R inst1/VeloSpike.app Apps/
./packv.sh 2 > /dev/null
cp -R feed feed12
./packv.sh 3 > /dev/null
ls -l feed Apps/VeloSpike.app/Contents/MacOS
