#!/usr/bin/env bash
# Builds a signed APT repository from CaptureMann .deb files (amd64 and arm64). Needs no server: the output is a
# folder of static files that any web server, GitHub Pages, a release-asset URL or an object store can serve.
# Uses only apt-ftparchive (package apt-utils) and gpg.
#
#   packaging/linux/make-apt-repo.sh --out DIR [--layout pool|flat] [--url URL] INPUT...
#
# INPUT is a .deb file or a folder searched for capturemann_*.deb (a previous repository folder works, so older
# versions stay installable and a downgrade or a pinned version keeps working). Every package must be
# `capturemann` for amd64 or arm64; the same file name twice is taken once.
#
# Layouts:
#   pool (default)  dists/<suite>/main/binary-<arch>/Packages{,.gz,.xz}, dists/<suite>/{Release,InRelease,Release.gpg},
#                   pool/main/c/capturemann/*.deb.  Users write:  Suites: stable  Components: main
#   flat            all files in one folder (Packages, Release, InRelease, Release.gpg and the .deb files), for hosts
#                   that give a flat file list, such as the assets of one GitHub release.  Users write:  Suites: ./
#
# Signing (the PRIVATE key is never in the repository; this script only reads it from the environment):
#   CM_APT_GPG_PRIVATE_KEY       the ASCII-armored secret key (the text, for example from a CI secret), or
#   CM_APT_GPG_KEY_FILE          a file holding the secret key (armored or binary); one of the two is required
#   CM_APT_GPG_PASSPHRASE        its passphrase, when it has one (read from the environment, never put on a command line)
#   CM_APT_GPG_KEY_ID            which key when the import holds several (default: the first signing key)
# The key is imported into a throw-away GNUPGHOME that is deleted at the end; ~/.gnupg is never touched.
# Created next to the repository, all public:
#   capturemann-archive-keyring.gpg   the public key, binary: this is the file `Signed-By:` points to
#   capturemann-archive-keyring.asc   the same, ASCII-armored
#   capturemann.sources               the deb822 file users put in /etc/apt/sources.list.d/ (needs --url)
#
# Other environment: CM_APT_SUITE (default stable; pool layout), CM_APT_COMPONENT (default main),
# CM_APT_ORIGIN (CaptureMann), CM_APT_LABEL (CaptureMann), CM_APT_URL (same as --url).
#
# The script verifies what it made: every signature with the exported public key alone, the hashes in Release
# against the files, and the Filename and SHA256 of every package against the pool.
set -euo pipefail
die() { echo "make-apt-repo.sh: $*" >&2; exit 1; }

layout=pool; out=""; url="${CM_APT_URL:-}"; inputs=()
while [ $# -gt 0 ]; do
  case "$1" in
    --out) out="${2:-}"; shift 2 ;;
    --layout) layout="${2:-}"; shift 2 ;;
    --url) url="${2:-}"; shift 2 ;;
    -h|--help) sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown option: $1" ;;
    *) inputs+=("$1"); shift ;;
  esac
done
[ -n "$out" ] || die "--out DIR is required"
case "$layout" in pool|flat) ;; *) die "--layout must be pool or flat" ;; esac
[ "${#inputs[@]}" -gt 0 ] || die "give at least one .deb or a folder with .deb files"
for t in apt-ftparchive gpg gpgv dpkg-deb sha256sum gzip xz; do
  command -v "$t" >/dev/null 2>&1 || die "required tool not found: $t (sudo apt install apt-utils gnupg gpgv dpkg xz-utils)"
done
suite="${CM_APT_SUITE:-stable}"; comp="${CM_APT_COMPONENT:-main}"
origin="${CM_APT_ORIGIN:-CaptureMann}"; label="${CM_APT_LABEL:-CaptureMann}"
[ -n "${CM_APT_GPG_PRIVATE_KEY:-}" ] || [ -n "${CM_APT_GPG_KEY_FILE:-}" ] \
  || die "no signing key: set CM_APT_GPG_PRIVATE_KEY (armored text) or CM_APT_GPG_KEY_FILE. See docs/development/linux-apt.md"
[ -z "${CM_APT_GPG_KEY_FILE:-}" ] || [ -f "$CM_APT_GPG_KEY_FILE" ] || die "no such key file: $CM_APT_GPG_KEY_FILE"

work="$(mktemp -d "${TMPDIR:-/tmp}/cm-apt.XXXXXX")"
export GNUPGHOME="$work/gnupg"; mkdir -m 700 "$GNUPGHOME"
cleanup() { gpgconf --kill gpg-agent >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT

# ---- the signing key (throw-away keyring) ----
if [ -n "${CM_APT_GPG_PRIVATE_KEY:-}" ]; then printf '%s\n' "$CM_APT_GPG_PRIVATE_KEY" > "$work/key.asc"; keysrc="$work/key.asc"
else keysrc="$CM_APT_GPG_KEY_FILE"; fi
printf 'allow-loopback-pinentry\n' > "$GNUPGHOME/gpg-agent.conf"
pass=()
if [ -n "${CM_APT_GPG_PASSPHRASE:-}" ]; then printf '%s' "$CM_APT_GPG_PASSPHRASE" > "$work/pass"; chmod 600 "$work/pass"; pass=(--pinentry-mode loopback --passphrase-file "$work/pass"); fi
gpg --batch --quiet "${pass[@]+"${pass[@]}"}" --import "$keysrc" 2>"$work/import.log" || { cat "$work/import.log" >&2; die "cannot import the signing key"; }
keyid="${CM_APT_GPG_KEY_ID:-}"
if [ -z "$keyid" ]; then
  keyid="$(gpg --batch --list-secret-keys --with-colons | awk -F: '$1=="sec"{print $5; exit}')"
fi
[ -n "$keyid" ] || die "the imported file holds no secret key"
fpr="$(gpg --batch --list-secret-keys --with-colons "$keyid" | awk -F: '$1=="fpr"{print $10; exit}')"
sign=(gpg --batch --yes "${pass[@]+"${pass[@]}"}" --local-user "$fpr" --digest-algo SHA512)
echo "==> signing key $fpr"

# ---- collect the packages ----
debs="$work/debs"; mkdir -p "$debs"
add_deb() {
  local f="$1" pkg arch
  pkg="$(dpkg-deb -f "$f" Package)"; arch="$(dpkg-deb -f "$f" Architecture)"
  [ "$pkg" = capturemann ] || die "$f is package '$pkg', not capturemann"
  case "$arch" in amd64|arm64) ;; *) die "$f is for '$arch'; only amd64 and arm64 are published" ;; esac
  if [ -e "$debs/$(basename "$f")" ]; then
    cmp -s "$f" "$debs/$(basename "$f")" || die "two different files named $(basename "$f")"
  else
    ln -s "$(cd "$(dirname "$f")" && pwd)/$(basename "$f")" "$debs/$(basename "$f")"
  fi
}
for i in "${inputs[@]}"; do
  if [ -d "$i" ]; then while IFS= read -r f; do add_deb "$f"; done < <(find "$i" -type f -name 'capturemann_*.deb' | sort)
  elif [ -f "$i" ]; then add_deb "$i"
  else die "no such file or folder: $i"; fi
done
n=$(find "$debs" -type l | wc -l); [ "$n" -gt 0 ] || die "no capturemann_*.deb found in the inputs"
archs="$(for f in "$debs"/*.deb; do dpkg-deb -f "$f" Architecture; done | sort -u | tr '\n' ' ')"
archs="${archs% }"
echo "==> $n package file(s), architectures: $archs"

mkdir -p "$out"; out="$(cd "$out" && pwd)"
# A repository is replaced as a whole: the old index files must not survive next to the new ones.
rm -rf "$out/dists" "$out/Packages" "$out/Packages.gz" "$out/Packages.xz" "$out/Release" "$out/InRelease" "$out/Release.gpg"

release_opts=(-o "APT::FTPArchive::Release::Origin=$origin" -o "APT::FTPArchive::Release::Label=$label"
              -o "APT::FTPArchive::Release::Description=CaptureMann APT repository"
              -o "APT::FTPArchive::Release::Architectures=$archs")
compress() { gzip -9nc "$1" > "$1.gz"; xz -9ec "$1" > "$1.xz"; }

if [ "$layout" = pool ]; then
  pooldir="$out/pool/$comp/c/capturemann"
  mkdir -p "$pooldir"
  for f in "$debs"/*.deb; do cp -L "$f" "$pooldir/"; done
  d="$out/dists/$suite"
  for a in $archs; do
    mkdir -p "$d/$comp/binary-$a"
    (cd "$out" && apt-ftparchive --arch "$a" packages "pool/$comp") > "$d/$comp/binary-$a/Packages"
    compress "$d/$comp/binary-$a/Packages"
  done
  (cd "$d" && apt-ftparchive "${release_opts[@]}" -o "APT::FTPArchive::Release::Suite=$suite" -o "APT::FTPArchive::Release::Codename=$suite" \
      -o "APT::FTPArchive::Release::Components=$comp" release .) > "$work/Release"
  mv "$work/Release" "$d/Release"
  "${sign[@]}" --clearsign -o "$d/InRelease" "$d/Release"
  "${sign[@]}" --detach-sign --armor -o "$d/Release.gpg" "$d/Release"
  index_dir="$d"
else
  find "$out" -maxdepth 1 -type f -name 'capturemann_*.deb' -delete
  for f in "$debs"/*.deb; do cp -L "$f" "$out/"; done
  (cd "$out" && apt-ftparchive packages . ) > "$out/Packages"
  compress "$out/Packages"
  # Release lists the index files only (not the 100 MB packages): hash a folder that holds just those.
  mkdir "$work/idx"; cp "$out/Packages" "$out/Packages.gz" "$out/Packages.xz" "$work/idx/"
  (cd "$work/idx" && apt-ftparchive "${release_opts[@]}" -o "APT::FTPArchive::Release::Suite=$suite" release .) > "$out/Release"
  "${sign[@]}" --clearsign -o "$out/InRelease" "$out/Release"
  "${sign[@]}" --detach-sign --armor -o "$out/Release.gpg" "$out/Release"
  index_dir="$out"
fi

# ---- the public key and the sources file ----
gpg --batch --export "$fpr" > "$out/capturemann-archive-keyring.gpg"
gpg --batch --armor --export "$fpr" > "$out/capturemann-archive-keyring.asc"
[ -s "$out/capturemann-archive-keyring.gpg" ] || die "the public key could not be exported"
if [ -n "$url" ]; then
  u="${url%/}/"
  if [ "$layout" = pool ]; then suites_line="Suites: $suite
Components: $comp"; else suites_line="Suites: ./"; fi
  cat > "$out/capturemann.sources" <<EOF
Types: deb
URIs: $u
$suites_line
Architectures: $archs
Signed-By: /usr/share/keyrings/capturemann-archive-keyring.gpg
EOF
fi

# ---- verify what was made, using only the exported public key (as apt does) ----
echo "==> verifying"
gpgv --keyring "$out/capturemann-archive-keyring.gpg" "$index_dir/InRelease" >/dev/null 2>&1 \
  || die "InRelease does not verify with the exported key"
gpgv --keyring "$out/capturemann-archive-keyring.gpg" "$index_dir/Release.gpg" "$index_dir/Release" >/dev/null 2>&1 \
  || die "Release.gpg does not verify with the exported key"
# InRelease and Release must say the same.
gpg --batch --decrypt "$index_dir/InRelease" 2>/dev/null | diff -q - "$index_dir/Release" >/dev/null || die "InRelease and Release differ"
check_hashes() { # base dir for the names in Release
  local base="$1" name hash
  awk '/^SHA256:/{f=1;next} /^[A-Za-z0-9-]+:/{f=0} f{print $1, $3}' "$index_dir/Release" | while read -r hash name; do
    [ "$(sha256sum "$base/$name" | cut -d' ' -f1)" = "$hash" ] || { echo "hash mismatch: $name" >&2; exit 1; }
  done
}
if [ "$layout" = pool ]; then check_hashes "$index_dir"; else check_hashes "$out"; fi
pk=0
for pf in $( [ "$layout" = pool ] && find "$out/dists" -name Packages || echo "$out/Packages" ); do
  while IFS= read -r line; do
    case "$line" in
      Filename:*) fn="${line#Filename: }" ;;
      SHA256:*) sh="${line#SHA256: }"
        [ "$(sha256sum "$out/$fn" | cut -d' ' -f1)" = "$sh" ] || die "$fn does not match its SHA256 in $pf"; pk=$((pk + 1)) ;;
    esac
  done < "$pf"
done
echo "    signatures ok, Release hashes ok, $pk package entr$( [ "$pk" = 1 ] && echo y || echo ies ) match the files"
echo "==> $layout repository in $out"
echo "    key fingerprint: $fpr"
ls -l "$out" | sed 's/^/    /'
