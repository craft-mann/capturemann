#!/bin/sh
# Realistic delta: real 0.2.9 Windows payload -> real 0.3.1 Windows payload (both public release assets).
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad
cd $S || exit 1
mkdir -p dl029
cd dl029 || exit 1
B=https://github.com/craft-mann/capturemann/releases/download/v0.2.9
cd $S || exit 1
rm -rf p029
mkdir p029
cd p029 || exit 1
unzip -q ../dl029/capturemann-0.2.9-windows-x64.zip
cd $S || exit 1
ls p029
rm -rf feed-real
mkdir feed-real
for pair in "0.2.9:p029/capturemann-0.2.9-windows-x64" "0.3.1:payload031/capturemann-0.3.1-windows-x64"; do
  v=${pair%%:*}; d=${pair#*:}
  sh $S/bin/vpkm.sh '[win]' pack -u CaptureMannReal -v $v -p $d -e cm-desktop.exe -o feed-real -r win-x64 --packTitle CaptureMann --noInst 2>&1 | grep -i "delta\|FTL" | cut -c1-200
done
ls -l feed-real
