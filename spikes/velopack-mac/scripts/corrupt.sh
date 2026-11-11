#!/bin/bash
cd /tmp/cm-velo-mac || exit 1
BIN=Apps/VeloSpike.app/Contents/MacOS/velo-spike
F=dev.capturemann.velospike-0.3.2-spike3-osx-arm64-full.nupkg
rm -rf fT fH fJ fS
for d in fT fH fJ fS; do mkdir $d; cp feed/$F $d/; python3 -I - $d <<'PY'
import json,sys
d=sys.argv[1]
j=json.load(open('feed/releases.osx-arm64.json'))
j['Assets']=[a for a in j['Assets'] if a['Type']=='Full' and a['Version']=='0.3.2-spike3']
json.dump(j,open(d+'/releases.osx-arm64.json','w'))
PY
done
# T: truncated file (size mismatch)
head -c 3000000 feed/$F > fT/$F
# H: same size, one byte flipped in the middle
python3 -I - <<'PY'
b=bytearray(open('feed/dev.capturemann.velospike-0.3.2-spike3-osx-arm64-full.nupkg','rb').read())
b[2000000]^=0xFF
open('fH/dev.capturemann.velospike-0.3.2-spike3-osx-arm64-full.nupkg','wb').write(b)
PY
# J: feed hashes wrong (SHA1+SHA256 changed), file intact
python3 -I - <<'PY'
import json
p='fJ/releases.osx-arm64.json'
j=json.load(open(p))
for a in j['Assets']:
    a['SHA256']='0'*64; a['SHA1']='0'*40
json.dump(j,open(p,'w'))
PY
# S: file corrupted (flip), SHA256 correct for original but SHA1 tampered to match?? -> here: only SHA1 changed, file intact => shows SHA1 ignored when SHA256 present
python3 -I - <<'PY'
import json
p='fS/releases.osx-arm64.json'
j=json.load(open(p))
for a in j['Assets']:
    a['SHA1']='0'*40
json.dump(j,open(p,'w'))
PY
port=8110
for d in fT fH fJ fS; do
  port=$((port+1))
  (python3 -I serve.py $port /tmp/cm-velo-mac/$d /tmp/cm-velo-mac/logs/server-$d.log > /dev/null 2>&1 &)
  sleep 1
  ./fresh.sh
  echo "=== $d (port $port)"
  VELO_LOG=/tmp/cm-velo-mac/logs/run-$d.log $BIN download-only http://127.0.0.1:$port 2>&1 | cut -c1-330 | grep -E "download took|check error|update available"
  echo "packages dir after:"; ls -la ~/Library/Caches/velopack/dev.capturemann.velospike/packages/ | grep -E "nupkg|partial"
done
pkill -f "serve.py 811"
