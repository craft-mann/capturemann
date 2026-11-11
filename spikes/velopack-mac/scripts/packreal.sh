#!/bin/bash
cd /tmp/cm-velo-mac || exit 1
rm -rf feedR
for v in 29 31; do
  ver=0.2.9; [ $v = 31 ] && ver=0.3.1
  s=$(date +%s)
  ./run.sh vpk pack --packId dev.capturemann.velorealspike --bundleId dev.capturemann.app --packVersion $ver --packDir real$v/CaptureMann.app --mainExe cm-desktop --runtime osx-arm64 --channel osx-arm64 --outputDir feedR 2>&1 | grep -E "elta|ERR|rror|Finished"
  echo "pack $ver took $(( $(date +%s) - s )) s"
done
ls -l feedR
