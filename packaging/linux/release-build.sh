#!/usr/bin/env bash
# One command for the release files of one Linux machine: tarball + .deb + checks + SHA256SUMS.txt, built from a
# given commit. Run it on each architecture (x86_64: the lab ThinkPad; aarch64: the VM), one at a time.
#
#   packaging/linux/release-build.sh --src ~/wt/main --commit <sha-or-ref> [--out DIR] [--jobs 3] [--target-dir DIR]
#                                    [--no-nice] [--keep-tree]
#
#   --src DIR       a git checkout (default ~/wt/main). It is only READ: the commit is exported with `git archive`
#                   into a clean tree, so uncommitted changes and untracked files of DIR never get into a release.
#   --commit REF    the commit to build (default: HEAD of DIR). The full SHA is recorded.
#   --out DIR       result folder (default ~/wt/release-out/<short sha>-<arch>).
#   --jobs N        cargo, FFmpeg, llama.cpp parallel jobs (default 3: the lab rule).
#   --target-dir D  cargo target dir (default ~/wt/release-target; kept between runs so a second run is incremental).
#   --no-nice       do not run at nice 19.
#   --keep-tree     keep the exported source tree (default: removed at the end, the outputs stay).
#
# Steps (each failing step stops the run): make sherpa (pinned libraries + speaker models), packaging/ffmpeg/build-linux.sh,
# packaging/llama/build-linux.sh, build-linux.sh with every CM_REQUIRE_* set, build-deb.sh, check-package.sh on both files,
# then SHA256SUMS.txt, BUILD-INFO.txt (commit, machine, tool versions, glibc) and the logs in <out>/logs/.
# Result (in OUT): capturemann-<v>-linux-<arch>.tar.gz(+.sha256), capturemann_<v>_<amd64|arm64>.deb(+.sha256),
# SHA256SUMS.txt, BUILD-INFO.txt, logs/, debug/ (unstripped programs for backtraces; not for the release page).
# Then: sign with sign-linux.sh (minisign) and make the APT repository with make-apt-repo.sh (docs/development/linux-apt.md).
# Takes about an hour on the ThinkPad at nice 19 when nothing is cached. Build tools: see packaging/linux/README.md.
set -euo pipefail
die() { echo "release-build.sh: $*" >&2; exit 1; }
[ "$(uname -s)" = Linux ] || die "Linux only"

src="$HOME/wt/main"; ref=""; out=""; jobs=3; tdir="$HOME/wt/release-target"; nice=1; keep=0
while [ $# -gt 0 ]; do
  case "$1" in
    --src) src="${2:-}"; shift 2 ;;
    --commit) ref="${2:-}"; shift 2 ;;
    --out) out="${2:-}"; shift 2 ;;
    --jobs) jobs="${2:-}"; shift 2 ;;
    --target-dir) tdir="${2:-}"; shift 2 ;;
    --no-nice) nice=0; shift ;;
    --keep-tree) keep=1; shift ;;
    -h|--help) sed -n '2,26p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option: $1" ;;
  esac
done
[ -d "$src" ] || die "no such source checkout: $src"
src="$(cd "$src" && pwd)"
sha="$(git -C "$src" rev-parse --verify "${ref:-HEAD}^{commit}")" || die "unknown commit ${ref:-HEAD} in $src"
short="${sha:0:8}"
case "$(uname -m)" in x86_64) arch=x86_64 ;; aarch64) arch=aarch64 ;; *) die "unsupported CPU $(uname -m)" ;; esac
out="${out:-$HOME/wt/release-out/$short-$arch}"
tree="$HOME/wt/release-src-$short"
mkdir -p "$out/logs"; out="$(cd "$out" && pwd)"
log() { echo "[$(date +%H:%M:%S)] $*" | tee -a "$out/logs/steps.log"; }
n=(); [ "$nice" = 1 ] && n=(nice -n 19)

rm -rf "$tree"; mkdir -p "$tree"
git -C "$src" archive --format=tar "$sha" | tar -x -C "$tree"
trap '[ "$keep" = 1 ] || rm -rf "$tree"' EXIT
log "building $sha ($arch) from a clean export of $src in $tree; output $out"
export CARGO_BUILD_JOBS="$jobs" CARGO_TARGET_DIR="$tdir" CM_FFMPEG_JOBS="$jobs" CM_LLAMA_JOBS="$jobs" CM_CARGO_JOBS="$jobs"
export PATH="$HOME/.cargo/bin:$PATH"
cd "$tree"
step() { local name="$1"; shift; log "== $name"; "$@" >"$out/logs/$name.log" 2>&1 || { tail -25 "$out/logs/$name.log" >&2; die "step '$name' failed (log: $out/logs/$name.log)"; }; }

step sherpa ${n[@]+"${n[@]}"} bash -c 'bash packaging/sherpa/fetch-linux.sh && bash packaging/sherpa/fetch-models.sh'
step ffmpeg ${n[@]+"${n[@]}"} bash packaging/ffmpeg/build-linux.sh
step llama ${n[@]+"${n[@]}"} bash packaging/llama/build-linux.sh
export CM_LINUX_OUT="$out" CM_REQUIRE_SHERPA=1 CM_REQUIRE_SPEAKER_MODELS=1 CM_REQUIRE_LLAMA=1 CM_REQUIRE_FFMPEG=1
export CM_FFMPEG_BUNDLE="$tree/packaging/ffmpeg/out-linux-$arch" CM_LLAMA_BUNDLE="$tree/packaging/llama/out"
step tarball ${n[@]+"${n[@]}"} bash packaging/linux/build-linux.sh
step deb bash packaging/linux/build-deb.sh
shopt -s nullglob
files=("$out"/capturemann-*-linux-"$arch".tar.gz "$out"/capturemann_*_*.deb)
[ "${#files[@]}" -eq 2 ] || die "expected one tarball and one .deb in $out, found ${#files[@]}"
for f in "${files[@]}"; do step "check-$(basename "$f" | tr -c 'A-Za-z0-9.\n' _)" bash packaging/linux/check-package.sh "$f"; done

log "== checksums"
( cd "$out" && sha256sum "$(basename "${files[0]}")" "$(basename "${files[1]}")" > SHA256SUMS.txt && sha256sum -c SHA256SUMS.txt )
{
  echo "commit:   $sha"
  echo "built:    $(date -u +%Y-%m-%dT%H:%M:%SZ) on $(hostname), $arch"
  echo "os:       $(. /etc/os-release; echo "$PRETTY_NAME"), glibc $(ldd --version | head -1 | awk '{print $NF}')"
  echo "rustc:    $(rustc --version)"
  echo "cmake:    $(cmake --version | head -1)   gcc: $(gcc --version | head -1)"
  echo "ffmpeg:   $(head -3 "$tree/packaging/ffmpeg/out-linux-$arch/BUILD-INFO.txt" | tr '\n' ' ')"
  echo "llama:    $(head -3 "$tree/packaging/llama/out/BUILD-INFO.txt" | tr '\n' ' ')"
  echo "files:"; sed 's/^/  /' "$out/SHA256SUMS.txt"
} > "$out/BUILD-INFO.txt"
log "== done: $out"; cat "$out/SHA256SUMS.txt"
