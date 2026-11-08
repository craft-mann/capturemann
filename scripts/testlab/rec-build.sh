#!/usr/bin/env bash
# Runs on the lab laptop: cargo in ~/wt/rec with its own target folder.  rec-build.sh <cargo args...>
export CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$HOME/wt/rec-target" PATH="$HOME/.cargo/bin:$PATH"
cd "$HOME/wt/rec" || exit 1
cargo "$@"
