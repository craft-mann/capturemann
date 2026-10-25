#!/usr/bin/env bash
# Runs a command (usually `cargo test ...` or `capturemann ...`) with CM_FFMPEG pointing at the
# FFmpeg that `make ffmpeg` built, so the exports it makes are done by the build that ships.
#
#   packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-media --locked
#   packaging/ffmpeg/with-bundled-ffmpeg.sh cargo test -p cm-desktop --test parity -- --nocapture
#
# The shipped build is deliberately small: it has no libx264 and no `lavfi` test sources. The test
# fixtures need both, and the parity test pulls single frames out of an export with the `select`
# filter. So this script puts two small wrapper programs first in line: calls that make fixtures
# (lavfi sources, libx264), extract test frames (`select=`) or decode a WebM the export wrote (the
# product only encodes VP9 and Opus; the shipped build has no decoder for them) go to the FFmpeg on PATH (Homebrew's),
# every other call (every export, every decode the product does, every ffprobe) goes to the bundled
# build. Every call is logged, and a count per binary is printed at the end, so a run shows which
# build did the work.
#
# Environment: CM_FFMPEG_OUT (default packaging/ffmpeg/out), CM_FIXTURE_FFMPEG (the full FFmpeg for
# fixtures; default: ffmpeg on PATH).
set -euo pipefail

die() { echo "with-bundled-ffmpeg.sh: $*" >&2; exit 1; }
[ "$#" -gt 0 ] || die "usage: $0 <command> [args...]"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out="${CM_FFMPEG_OUT:-$root/packaging/ffmpeg/out}"
[ -x "$out/ffmpeg" ] && [ -x "$out/ffprobe" ] || die "no bundled FFmpeg in $out (run: make ffmpeg)"
full="${CM_FIXTURE_FFMPEG:-$(command -v ffmpeg || true)}"
[ -n "$full" ] || die "no full FFmpeg on PATH for the test fixtures (brew install ffmpeg), or set CM_FIXTURE_FFMPEG"

dir="$(mktemp -d "${TMPDIR:-/tmp}/cm-bundled-ffmpeg.XXXXXX")"
log="$dir/calls.log"
: >"$log"
trap 'echo; echo "ffmpeg/ffprobe calls by binary (bundled = ours):"; awk "{print \$1\" \"\$2}" "$log" | sort | uniq -c | sed "s/^/  /"; rm -rf "$dir"' EXIT

cat >"$dir/ffmpeg" <<EOF
#!/bin/bash
for a in "\$@"; do
  case "\$a" in
    lavfi|libx264|libx265|testsrc*|sine*|anullsrc*|color=*|nullsrc*|select=*|*.webm)
      echo "full   ffmpeg" >>"$log"; exec "$full" "\$@";;
  esac
done
echo "bundled ffmpeg" >>"$log"
exec "$out/ffmpeg" "\$@"
EOF
cat >"$dir/ffprobe" <<EOF
#!/bin/bash
echo "bundled ffprobe" >>"$log"
exec "$out/ffprobe" "\$@"
EOF
chmod +x "$dir/ffmpeg" "$dir/ffprobe"

CM_FFMPEG="$dir" "$@"
