#!/usr/bin/env bash
# Proves the APT repository end to end: builds a signed repository from the given .deb files with a
# THROWAWAY key, serves it over HTTP, and in a clean Ubuntu 24.04 environment runs
#   add the source and key -> apt install -> run -> publish a newer version -> apt upgrade -> apt remove -> apt purge
# and the negative checks (an index changed after signing is refused).
#
#   packaging/linux/test-apt-repo.sh --runner docker|chroot=DIR  [--layout pool|flat] [--keep] capturemann_<v>_<arch>.deb...
#
#   --runner docker      `docker run ubuntu:24.04` (CI). The container shares the host network.
#   --runner chroot=DIR  an existing Ubuntu 24.04 root file system in DIR (for example made by `sudo debootstrap noble DIR`),
#                        entered with sudo chroot. For a machine without Docker.
#   --layout             pool (default) or flat, as for make-apt-repo.sh.
#   --keep               keep the work folder (printed) instead of deleting it.
# The .deb files must include one for this machine's architecture. The "newer" package is the same file with its version
# raised to <version>+aptcheck2 (control file edited, nothing else). No real key is used and nothing is published.
set -euo pipefail
die() { echo "test-apt-repo.sh: $*" >&2; exit 1; }

runner=""; layout=pool; keep=0; debs=()
while [ $# -gt 0 ]; do
  case "$1" in
    --runner) runner="${2:-}"; shift 2 ;;
    --layout) layout="${2:-}"; shift 2 ;;
    --keep) keep=1; shift ;;
    -h|--help) sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    -*) die "unknown option: $1" ;;
    *) debs+=("$1"); shift ;;
  esac
done
[ -n "$runner" ] && [ "${#debs[@]}" -gt 0 ] || die "usage: test-apt-repo.sh --runner docker|chroot=DIR [--layout pool|flat] DEB..."
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
case "$(uname -m)" in x86_64) host_deb=amd64 ;; aarch64) host_deb=arm64 ;; *) die "unsupported machine" ;; esac
mine=""; for d in "${debs[@]}"; do [ "$(dpkg-deb -f "$d" Architecture)" = "$host_deb" ] && mine="$d"; done
[ -n "$mine" ] || die "no $host_deb package among the inputs"
v1="$(dpkg-deb -f "$mine" Version)"; v2="${v1}+aptcheck2"

work="$(mktemp -d "${TMPDIR:-/tmp}/cm-apt-test.XXXXXX")"
www="$work/www"; port=""; srv=""; cname=""
cleanup() {
  [ -z "$srv" ] || kill "$srv" 2>/dev/null || true
  [ -z "$cname" ] || docker rm -f "$cname" >/dev/null 2>&1 || true
  GNUPGHOME="$work/gh" gpgconf --kill gpg-agent >/dev/null 2>&1 || true
  if [ "$keep" = 1 ]; then echo "work folder kept: $work"; else rm -rf "$work"; fi
}
trap cleanup EXIT

# ---- a throwaway key (never a real one) ----
mkdir -m 700 "$work/gh"
GNUPGHOME="$work/gh" gpg --batch --passphrase '' --quick-generate-key 'CaptureMann APT TEST key <test@example.invalid>' rsa3072 sign 1d 2>/dev/null
export CM_APT_GPG_PRIVATE_KEY; CM_APT_GPG_PRIVATE_KEY="$(GNUPGHOME="$work/gh" gpg --batch --armor --export-secret-keys)"

# ---- serve it ----
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
url="http://127.0.0.1:$port/"
mkdir -p "$www"
make_repo() { # extra deb files...
  CM_APT_URL="$url" "$here/make-apt-repo.sh" --out "$www" --layout "$layout" "$@" >"$work/make.log" 2>&1 || { cat "$work/make.log" >&2; die "make-apt-repo.sh failed"; }
  grep -E '^(==> .*package|    signatures)' "$work/make.log" | sed 's/^/    /'
}
echo "== building repository 1 ($layout): $(basename "$mine") $v1"
make_repo "${debs[@]}"
(cd "$www" && exec python3 -m http.server "$port" --bind 127.0.0.1 >"$work/http.log" 2>&1) &
srv=$!
for _ in $(seq 1 50); do curl -fsS -o /dev/null "${url}capturemann-archive-keyring.gpg" 2>/dev/null && break; sleep 0.2; done
curl -fsS -o /dev/null "${url}capturemann-archive-keyring.gpg" || die "the test web server does not answer"

# ---- the clean environment ----
case "$runner" in
  docker)
    cname="cm-apt-test-$$"
    docker run -d --name "$cname" --network host ubuntu:24.04 sleep 3600 >/dev/null
    in_target() { docker exec -i "$cname" bash -ec "$1"; } ;;
  chroot=*)
    root="${runner#chroot=}"; [ -d "$root/etc" ] || die "no root file system in $root"
    in_target() { sudo chroot "$root" /usr/bin/env -i PATH=/usr/sbin:/usr/bin:/sbin:/bin HOME=/root LANG=C.UTF-8 bash -ec "$1"; } ;;
  *) die "unknown runner $runner" ;;
esac
export DEBIAN_FRONTEND=noninteractive
apt_env='export DEBIAN_FRONTEND=noninteractive; '
step() { echo "== $*"; }

# A reused root (chroot) is reset first: no package, source or key left from an earlier run.
case "$runner" in chroot=*) in_target "apt-get purge -y -qq capturemann >/dev/null 2>&1 || true; rm -f /etc/apt/sources.list.d/capturemann.sources /usr/share/keyrings/capturemann-archive-keyring.gpg" ;; esac
step "the clean system has no gpg, no capturemann, no source"
in_target ". /etc/os-release; echo \$PRETTY_NAME; test \"\$VERSION_ID\" = 24.04; if command -v gpg; then exit 1; fi; if dpkg -s capturemann >/dev/null 2>&1; then exit 1; fi; apt-get update -qq >/dev/null; apt-get install -y -qq --no-install-recommends curl ca-certificates >/dev/null"

step "add the key and the source exactly as docs/development/linux-apt.md says"
in_target "
install -d -m 0755 /usr/share/keyrings
curl -fsSL ${url}capturemann-archive-keyring.gpg -o /usr/share/keyrings/capturemann-archive-keyring.gpg
curl -fsSL ${url}capturemann.sources -o /etc/apt/sources.list.d/capturemann.sources
cat /etc/apt/sources.list.d/capturemann.sources
apt-get update --error-on=any 2>&1 | tee /tmp/update.log | grep -i capturemann || true
grep -qi -E 'warning|error|not signed|NO_PUBKEY' /tmp/update.log && { echo 'apt-get update was not clean:'; cat /tmp/update.log; exit 1; } || true
apt-cache policy capturemann | tee /tmp/policy.txt
grep -q 'Candidate: $v1' /tmp/policy.txt"

step "install (with its dependencies; recommends left out as in a minimal container)"
in_target "$apt_env apt-get install -y -qq --no-install-recommends capturemann >/tmp/install.log 2>&1 || { tail -30 /tmp/install.log; exit 1; }
dpkg -s capturemann | grep -E '^(Status|Version|Architecture):'
test \"\$(dpkg-query -W -f='\${Version}' capturemann)\" = '$v1'
capturemann --version
rc=0; cm-mcp --bogus >/dev/null 2>&1 || rc=\$?; test \$rc -eq 2
test -x /opt/capturemann/lib/capturemann/cm-desktop
test -s /usr/share/doc/capturemann/LICENSE && test -s /usr/share/doc/capturemann/copyright
test -f /usr/share/applications/dev.capturemann.app.desktop"

step "a newer version appears in the repository, apt upgrade installs it"
# The same package with only its Version changed, in a new pool entry next to the old one.
mkdir "$work/v2"; dpkg-deb -R "$mine" "$work/v2/x"
sed -i "s/^Version: .*/Version: $v2/" "$work/v2/x/DEBIAN/control"
newdeb="$work/v2/capturemann_${v2}_${host_deb}.deb"
dpkg-deb --root-owner-group -Zgzip -z1 --build "$work/v2/x" "$newdeb" >/dev/null
rm -rf "$work/v2/x"
make_repo "${debs[@]}" "$newdeb"
in_target "$apt_env apt-get update -qq --error-on=any 2>&1 | tee /tmp/update2.log
if grep -qi -E 'warning|error|not signed' /tmp/update2.log; then exit 1; fi
apt-get --simulate upgrade | grep -q 'capturemann'
apt-get install -y -qq --no-install-recommends --only-upgrade capturemann >/tmp/upgrade.log 2>&1 || { tail -30 /tmp/upgrade.log; exit 1; }
test \"\$(dpkg-query -W -f='\${Version}' capturemann)\" = '$v2'
capturemann --version
n=\$(apt-get -s upgrade | grep -c '^Inst capturemann' || true); test \$n -eq 0"

step "a tampered index is refused"
inrel="$www/dists/stable/InRelease"; [ "$layout" = pool ] || inrel="$www/InRelease"
cp "$inrel" "$work/InRelease.good"
sed -i 's/^Label: .*/Label: Tampered/' "$inrel"
in_target "if apt-get update --error-on=any >/tmp/bad.log 2>&1; then echo 'apt accepted a tampered InRelease'; cat /tmp/bad.log; exit 1; fi
grep -i -E 'signatures were invalid|BADSIG|not signed|could not be verified' /tmp/bad.log >/dev/null || { cat /tmp/bad.log; exit 1; }"
cp "$work/InRelease.good" "$inrel"

step "remove (settings and the source stay), then purge"
in_target "$apt_env apt-get remove -y -qq capturemann >/dev/null
test ! -e /opt/capturemann && test ! -e /usr/bin/capturemann && test ! -e /usr/share/applications/dev.capturemann.app.desktop
if dpkg -s capturemann 2>/dev/null | grep -q '^Status: install ok installed'; then exit 1; fi
apt-get purge -y -qq capturemann >/dev/null 2>&1 || true
test -f /etc/apt/sources.list.d/capturemann.sources
apt-get update -qq >/dev/null 2>&1
apt-cache policy capturemann | grep -q 'Candidate: $v2'"

echo
echo "PASS: install, upgrade ($v1 -> $v2), remove and purge through the $layout repository on $(uname -m)"
