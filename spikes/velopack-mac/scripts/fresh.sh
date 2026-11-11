#!/bin/bash
# fresh install of spike1 into Apps/, clean cache and run log
cd /tmp/cm-velo-mac || exit 1
pkill -f "VeloSpike.app"; pkill -f "sleep 600"
chmod -R u+w Apps 2>/dev/null
rm -rf Apps ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
mkdir -p Apps && ditto inst1/dev.capturemann.velospike.app Apps/VeloSpike.app
