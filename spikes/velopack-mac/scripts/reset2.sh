#!/bin/bash
cd /tmp/cm-velo-mac || exit 1
pkill -f "VeloSpike.app"; pkill -f "dev.capturemann.velospike.app"; pkill -f "sleep 600"
rm -rf Apps inst1 feed feed12 pack ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
for v in 1 2 3; do ./build.sh 0.3.2-spike$v > /dev/null; ./mkapp.sh $v > /dev/null 2>&1; done
pk() { ./run.sh vpk pack --packId dev.capturemann.velospike --packVersion 0.3.2-spike$1 --packDir pack/app$1/VeloSpike.app --mainExe velo-spike --runtime osx-arm64 --channel osx-arm64 --outputDir feed --signAppIdentity - 2>&1 | grep -E "elta|ERR|rror"; }
pk 1
mkdir -p inst1 Apps && (cd inst1 && unzip -q -o ../feed/dev.capturemann.velospike-osx-arm64-Portable.zip) && ditto inst1/dev.capturemann.velospike.app Apps/VeloSpike.app
pk 2
cp -R feed feed12
pk 3
ls -l feed
