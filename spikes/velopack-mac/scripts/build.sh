#!/bin/bash
# usage: build.sh <version>  -> builds velo-spike release with SPIKE_VER and copies to $WORK/bins/velo-spike-<ver>
export CARGO_BUILD_JOBS=4
cd /Users/trungtran/source/repos/capturemann/.claude/worktrees/agent-a5b37077b3611f8ef/spikes/velopack-mac || exit 1
mkdir -p /tmp/cm-velo-mac/bins
start=$(date +%s)
SPIKE_VER="$1" cargo build --release --bin velo-spike 2>&1 | tail -3
echo "build seconds: $(( $(date +%s) - start ))"
cp target/release/velo-spike /tmp/cm-velo-mac/bins/velo-spike-$1
ls -l target/release/hello-plain target/release/velo-spike
