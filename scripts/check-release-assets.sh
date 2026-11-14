#!/usr/bin/env bash
# Checks a release's asset names (and, for a folder, their checksums) before upload or publish.
# The names are an interface: the Homebrew cask builds its URL from them and the README tells users
# the same names. The table in docs/development/releasing.md ("Asset names") is the one source of
# truth; this script reads it, so changing a name means changing that table first.
#
#   scripts/check-release-assets.sh <version> <folder>             a staging folder (names, .sha256, .minisig, SHA256SUMS.txt)
#   scripts/check-release-assets.sh <version> --names <file|->     one asset name per line (names only)
#   scripts/check-release-assets.sh <version> --github <owner/repo> the names of the release v<version> (needs `gh`)
#   options: --cask <Casks/capturemann.rb>   also check the cask's url pattern against the names
#            --releasing <file>               the table to read (default docs/development/releasing.md)
#            --require macos,windows,linux    systems that must be complete (default: every system that
#                                             has at least one file; a release of one system is normal)
#
# It fails (exit 1, every problem listed) when: a name is not in the table; a system that is present
# is incomplete; a `.sha256` is not `hash  name` or does not match its file; a Linux `.minisig` is
# missing; `SHA256SUMS.txt` lists a different set or different hashes; or the cask's url does not
# resolve to an asset of this release. Exit 2: bad usage.
set -euo pipefail

die() { echo "check-release-assets.sh: $*" >&2; exit 2; }
here="$(cd "$(dirname "$0")/.." && pwd)"

version="${1:-}"; [ -n "$version" ] || { sed -n '2,19p' "$0"; exit 2; }
version="${version#v}"; shift
src=""; srcarg=""; cask=""; releasing="$here/docs/development/releasing.md"; require=""
while [ $# -gt 0 ]; do
  case "$1" in
    --names) src=names; srcarg="${2:?--names needs a file or -}"; shift 2 ;;
    --github) src=github; srcarg="${2:?--github needs owner/repo}"; shift 2 ;;
    --cask) cask="${2:?--cask needs a file}"; shift 2 ;;
    --releasing) releasing="${2:?--releasing needs a file}"; shift 2 ;;
    --require) require="${2:?--require needs a list}"; shift 2 ;;
    -*) die "unknown option $1" ;;
    *) src=folder; srcarg="$1"; shift ;;
  esac
done
[ -n "$src" ] || die "give a folder, --names or --github"
[ -f "$releasing" ] || die "no $releasing"

sha_of() { if command -v sha256sum >/dev/null; then sha256sum "$1" | awk '{print $1}'; else shasum -a 256 "$1" | awk '{print $1}'; fi; }

# ---- the table ------------------------------------------------------------------------------
# table_names <System>: the backticked names in that row of the table that contain <v>, for this version.
table_names() {
  awk -F'|' -v s="$1" '{ c=$2; gsub(/^ +| +$/, "", c) } c == s { print $3 }' "$releasing" \
    | grep -o '`[^`]*`' | tr -d '`' | grep '<v>' | sed "s/<v>/$version/g" || true
}
mac="$(table_names macOS)"; win="$(table_names Windows)"; lin="$(table_names Linux)"
[ -n "$mac" ] && [ -n "$win" ] && [ -n "$lin" ] || die "could not read the macOS, Windows and Linux rows of the table in $releasing"

# expected_of <system>: every file name of that system, with its sidecars.
expected_of() {
  local payload="$1" n
  case "$2" in
    macos|windows) for n in $payload; do echo "$n"; echo "$n.sha256"; done ;;
    linux) for n in $payload; do echo "$n"; echo "$n.sha256"; echo "$n.minisig"; echo "$n.sha256.minisig"; done ;;
  esac
}
exp_mac="$(expected_of "$mac" macos)"; exp_win="$(expected_of "$win" windows)"; exp_lin="$(expected_of "$lin" linux)"
all_names="SHA256SUMS.txt"
all_extra="SHA256SUMS.txt.minisig"

# ---- the names ------------------------------------------------------------------------------
tmp="$(mktemp -d)"; trap 'rm -rf "$tmp"' EXIT
case "$src" in
  folder) [ -d "$srcarg" ] || die "not a folder: $srcarg"
          (cd "$srcarg" && find . -maxdepth 1 -type f | sed 's|^\./||' | sort) > "$tmp/names" ;;
  names)  if [ "$srcarg" = "-" ]; then sort; else sort "$srcarg"; fi > "$tmp/names" ;;
  github) command -v gh >/dev/null || die "the GitHub CLI (gh) is needed for --github"
          gh release view "v$version" --repo "$srcarg" --json assets -q '.assets[].name' | sort > "$tmp/names" ;;
esac
[ -s "$tmp/names" ] || die "no asset names found"

problems=0
bad() { echo "PROBLEM: $*" >&2; problems=$((problems + 1)); }
has() { grep -qxF -- "$1" "$tmp/names"; }

# 1. every name is in the table
allowed="$(printf '%s\n' "$exp_mac" "$exp_win" "$exp_lin" "$all_names" "$all_extra")"
while IFS= read -r n; do
  printf '%s\n' "$allowed" | grep -qxF -- "$n" || bad "'$n' is not a name in the table of $(basename "$releasing") (version $version)"
done < "$tmp/names"

# 2. a system that is present is complete
present() {
  local n
  while IFS= read -r n; do
    case "$n" in *.sha256 | *.minisig) continue ;; esac
    has "$n" && return 0
  done <<< "$1"
  return 1
}
for sys in macos windows linux; do
  case "$sys" in macos) exp="$exp_mac" ;; windows) exp="$exp_win" ;; linux) exp="$exp_lin" ;; esac
  need=0
  present "$exp" && need=1
  case ",$require," in *",$sys,"*) need=1 ;; esac
  if [ "$need" = 1 ]; then
    while IFS= read -r n; do has "$n" || bad "$sys: '$n' is missing"; done <<< "$exp"
  fi
done
has SHA256SUMS.txt || bad "SHA256SUMS.txt is missing"

# 3. the cask's url pattern resolves to assets of this release
if [ -n "$cask" ]; then
  [ -f "$cask" ] || die "no cask file $cask"
  url="$(sed -n 's/^ *url "\(.*\)".*/\1/p' "$cask" | head -1)"
  arches="$(sed -n 's/^ *arch \(.*\)/\1/p' "$cask" | head -1 | grep -o '"[^"]*"' | tr -d '"')"
  [ -n "$url" ] && [ -n "$arches" ] || bad "cask: could not read the url or the arch line of $cask"
  for a in $arches; do
    asset="$(printf '%s' "$url" | sed -e "s/#{version}/$version/g" -e "s/#{arch}/$a/g" -e 's|.*/||')"
    has "$asset" || bad "cask: its url builds '$asset' for $a, which is not an asset of v$version"
  done
fi

# 4. folder only: checksums and signatures
if [ "$src" = folder ]; then
  cd "$srcarg"
  while IFS= read -r f; do
    case "$f" in
      *.sha256)
        base="${f%.sha256}"
        [ -f "$base" ] || { bad "$f has no file '$base' next to it"; continue; }
        line="$(cat "$f")"
        want="$(printf '%s' "$line" | awk '{print $1}')"; name="$(printf '%s' "$line" | awk '{print $2}')"
        [ "$(printf '%s\n' "$line" | wc -l | tr -d ' ')" = 1 ] || bad "$f is not one line"
        case "$line" in "$want  $name") ;; *) bad "$f is not 'hash  name' (two spaces)" ;; esac
        [ "$name" = "$base" ] || bad "$f names '$name', not '$base'"
        [ "$(sha_of "$base")" = "$want" ] || bad "$f does not match $base"
        ;;
    esac
  done < "$tmp/names"
  if [ -f SHA256SUMS.txt ]; then
    listed="$(awk '{print $2}' SHA256SUMS.txt | sort)"
    payload="$(grep -vE '\.(sha256|minisig)$|^SHA256SUMS.txt$' "$tmp/names" | sort)"
    [ "$listed" = "$payload" ] || bad "SHA256SUMS.txt lists a different set than the files: $(diff <(printf '%s\n' "$listed") <(printf '%s\n' "$payload") | tr '\n' ' ')"
    while read -r h n; do
      [ -f "$n" ] || continue
      [ "$(sha_of "$n")" = "$h" ] || bad "SHA256SUMS.txt has a wrong hash for $n"
    done < SHA256SUMS.txt
  fi
fi

if [ "$problems" -gt 0 ]; then
  echo "check-release-assets.sh: $problems problem(s) for v$version" >&2
  exit 1
fi
echo "check-release-assets.sh: v$version OK ($(wc -l < "$tmp/names" | tr -d ' ') names, source: $src)"
