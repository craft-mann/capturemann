#!/bin/sh
# Builds three payload trees from the real 0.3.1 Windows payload plus the scratch exes.
S=/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad
W=/Users/trungtran/source/repos/capturemann/.claude/worktrees/agent-a1bec45ce3c42f688/spikes/velopack-win/target/x86_64-pc-windows-msvc/release
B=$S/payload031/capturemann-0.3.1-windows-x64
cd $S || exit 1
rm -rf p1 p2 p3
for n in p1 p2 p3; do
  cp -cR "$B" $n
  cp $W/velospike.exe $W/vcli.exe $W/vplain.exe $n/
done
# p2: "recompile" of cm-desktop.exe: 20000 scattered 4-byte edits (~80 KB of 94 MB changed in many places)
python3 -I - <<'EOF'
import random
S='/private/tmp/claude-502/-Users-trungtran-source-repos-capturemann/493a1988-1983-4365-b04d-8243b5085854/scratchpad'
random.seed(7)
def edit(path, n):
    b=bytearray(open(path,'rb').read())
    for _ in range(n):
        i=random.randrange(1024, len(b)-8)
        b[i:i+4]=random.randbytes(4)
    open(path,'wb').write(b)
edit(S+'/p2/cm-desktop.exe',20000)
# p3 = p2 + another small recompile of cm-daemon.exe
edit(S+'/p3/cm-desktop.exe',20000)
edit(S+'/p3/cm-daemon.exe',20000)
EOF
# p3 shares p2's cm-desktop edits? (no: independent edits, different seed state) -> p2 and p3 both differ from p1
ls -l p1/cm-desktop.exe p2/cm-desktop.exe p3/cm-desktop.exe
