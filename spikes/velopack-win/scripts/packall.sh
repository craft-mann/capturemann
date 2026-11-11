#!/bin/sh
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad
cd $S || exit 1
rm -rf feed
mkdir feed
i=1
for v in 0.3.2-spike1 0.3.2-spike2 0.3.2-spike3; do
  echo "=== pack $v from p$i"
  START=$(date +%s)
  sh $S/bin/vpkm.sh '[win]' pack -u CaptureMannSpike -v $v -p p$i -e velospike.exe -o feed -r win-x64 \
    --packTitle CaptureMann --packAuthors "CaptureMann contributors" -i p$i/app.ico --shortcuts StartMenuRoot 2>&1 | cut -c1-200 | grep -v "INF\] \(Starting\|Complete\)" | tail -12
  echo "pack seconds: $(( $(date +%s) - START ))"
  i=$((i+1))
done
ls -l feed
cat feed/releases.win.json
