#!/bin/bash
# usage: packv.sh <n> -> packs 0.3.2-spike<n> from bins into feed
cd /tmp/cm-velo-mac || exit 1
v=$1
mkdir -p pack/s$v
cp bins/velo-spike-0.3.2-spike$v pack/s$v/velo-spike
./run.sh vpk pack --packId dev.capturemann.velospike --bundleId dev.capturemann.velospike --packVersion 0.3.2-spike$v --packDir pack/s$v --mainExe velo-spike --packTitle VeloSpike --runtime osx-arm64 --channel osx-arm64 --outputDir feed 2>&1 | grep -E "elta|ERR|WRN"
