#!/bin/bash
# Realistic-size test: scratch app whose Resources/payload holds the real 0.2.9 (v1) / 0.3.1 (v2) bundle contents (never executed).
cd /tmp/cm-velo-mac || exit 1
pkill -f "VeloSpike.app"; pkill -f "serve.py 8120"
rm -rf bigapps feedBig packbig ~/Library/Caches/velopack/dev.capturemann.velospike logs/run.log
mkdir -p packbig
for n in 1 2; do
  src=real29; [ $n = 2 ] && src=real31
  d=packbig/app$n/VeloSpike.app
  ./mkapp.sh $n none > /dev/null 2>&1
  mkdir -p packbig/app$n
  ditto pack/app$n/VeloSpike.app $d
  mkdir -p $d/Contents/Resources/payload
  ditto $src/CaptureMann.app/Contents/MacOS $d/Contents/Resources/payload/MacOS
  ditto $src/CaptureMann.app/Contents/Resources $d/Contents/Resources/payload/Resources
  codesign --force --deep --options runtime -s - $d 2>&1 | tail -1
  du -sh $d
  s=$(date +%s)
  ./run.sh vpk pack --packId dev.capturemann.velospike --packVersion 0.3.2-spike$n --packDir $d --mainExe velo-spike --runtime osx-arm64 --channel osx-arm64 --outputDir feedBig --signAppIdentity - 2>&1 | grep -E "elta|ERR|rror"
  echo "pack $n took $(( $(date +%s) - s )) s"
  if [ $n = 1 ]; then mkdir -p bigapps && (cd feedBig && unzip -q -o dev.capturemann.velospike-osx-arm64-Portable.zip -d ../bigunz) && ditto bigunz/dev.capturemann.velospike.app bigapps/VeloSpike.app; fi
done
ls -l feedBig
(python3 -I serve.py 8120 /tmp/cm-velo-mac/feedBig /tmp/cm-velo-mac/logs/server-big.log > /dev/null 2>&1 &)
sleep 1
