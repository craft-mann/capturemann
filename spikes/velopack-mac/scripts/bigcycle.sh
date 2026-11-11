#!/bin/bash
# usage: bigcycle.sh full|delta  -> reinstall v1 (190 MB payload), update to v2, print timings
cd /tmp/cm-velo-mac || exit 1
mode=$1
pkill -f "VeloSpike.app"
rm -rf bigapps ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
mkdir bigapps && ditto bigunz/dev.capturemann.velospike.app bigapps/VeloSpike.app
if [ "$mode" = delta ]; then
  mkdir -p ~/Library/Caches/velopack/dev.capturemann.velospike/packages
  cp feedBig/dev.capturemann.velospike-0.3.2-spike1-osx-arm64-full.nupkg ~/Library/Caches/velopack/dev.capturemann.velospike/packages/
fi
bigapps/VeloSpike.app/Contents/MacOS/velo-spike update http://127.0.0.1:8120 > /dev/null 2>&1
for i in $(seq 1 90); do
  grep -q "after-update:" logs/run.log && break
  sleep 1
done
a=$(grep "download took" logs/run.log | sed 's/.*download took \([0-9]*\) ms.*/\1/')
t0=$(grep "applying and restarting" logs/run.log | cut -d' ' -f1)
t1=$(grep "after-update:" logs/run.log | cut -d' ' -f1)
echo "mode=$mode download_ms=$a down_ms=$((t1 - t0))"
