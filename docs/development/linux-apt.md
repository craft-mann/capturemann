# CaptureMann APT repository (Linux, 0.3.0)

A signed APT repository for Ubuntu 24.04 / Debian 13 and later, `amd64` and `arm64`, so that
`sudo apt install capturemann` installs the app with its libraries and `sudo apt upgrade` updates it. It is
static files only: no server program. The tarball (minisign) and the single `.deb` downloads stay as they are.

Status: the tooling is built and proved in a clean Ubuntu 24.04 environment (reports in `verification/`, names
`L1-linux-0.3.0-packaging-apt-*`). **Nothing is published**: the repository key does not exist yet and the host is
not chosen (see "Decisions for the maintainer"). Everything below that shows a URL uses `https://apt.example.org/capturemann/`
as the placeholder.

## For users (to go on the website and the release page once the host is chosen)

Install (Ubuntu 24.04+, Debian 13+; `amd64` or `arm64`):

```sh
sudo install -d -m 0755 /usr/share/keyrings
sudo curl -fsSL https://apt.example.org/capturemann/capturemann-archive-keyring.gpg \
  -o /usr/share/keyrings/capturemann-archive-keyring.gpg
sudo curl -fsSL https://apt.example.org/capturemann/capturemann.sources \
  -o /etc/apt/sources.list.d/capturemann.sources
sudo apt update
sudo apt install capturemann
```

`capturemann.sources` is a deb822 file; this is its whole content for the pool layout:

```
Types: deb
URIs: https://apt.example.org/capturemann/
Suites: stable
Components: main
Architectures: amd64 arm64
Signed-By: /usr/share/keyrings/capturemann-archive-keyring.gpg
```

`Signed-By` limits the key to this one repository (it is not added to the global trusted keys, which `apt-key` did and
which is deprecated). Check the key before you trust it: the fingerprint is printed on the release page and the
website; compare with `gpg --show-keys /usr/share/keyrings/capturemann-archive-keyring.gpg`.

| Task | Command |
|---|---|
| Upgrade | `sudo apt update && sudo apt upgrade` (or `sudo apt install --only-upgrade capturemann`) |
| Remove the app | `sudo apt remove capturemann` (your library and settings are never touched) |
| Remove and forget the package state | `sudo apt purge capturemann` (still leaves your library and settings) |
| Stop using the repository | `sudo rm /etc/apt/sources.list.d/capturemann.sources /usr/share/keyrings/capturemann-archive-keyring.gpg` |
| A specific older version | `apt list -a capturemann`, then `sudo apt install capturemann=<version>` |

Pre-releases use `~` in the version (`0.3.0~rc.1` sorts before `0.3.0`), so the final release replaces them on upgrade.
`apt` installs `Depends:` libraries and, unless you pass `--no-install-recommends`, the recommended ones (secret
service, portal, PipeWire, Tesseract for Auto-redact).

## Layouts

`make-apt-repo.sh --layout pool` (default) is the usual layout: `dists/stable/{InRelease,Release,Release.gpg}`,
`dists/stable/main/binary-<arch>/Packages{,.gz,.xz}` and `pool/main/c/capturemann/*.deb`. Needs a host that serves a
folder tree.

`--layout flat` puts every file (the `.deb` files, `Packages*`, `Release`, `InRelease`, `Release.gpg`) in one folder, for a
host that only gives a flat list of files, such as the assets of one GitHub release. The user's source then has
`Suites: /` and no `Components:` (`Suites: ./` makes apt request `<URL>/./InRelease`, which GitHub release downloads answer with a 504; `/` was tested against the real `apt` release on Nov 9, 2026). `capturemann.sources` is written for the chosen layout.

## For the maintainer

### One-time: create the repository key (the private key never goes in the repo)

Do this on your own machine, ideally an offline or encrypted one. RSA 4096 is the choice with the widest apt
compatibility; the key is signing-only.

```sh
export GNUPGHOME=$(mktemp -d)               # a scratch keyring; ~/.gnupg is not used
gpg --batch --pinentry-mode loopback --passphrase 'CHOOSE-A-LONG-PASSPHRASE' \
    --quick-generate-key 'CaptureMann APT Repository <apt@YOUR-DOMAIN>' rsa4096 sign 5y
gpg --list-keys --with-fingerprint          # note the fingerprint: it goes on the release page and the website
gpg --armor --export-secret-keys > capturemann-apt-secret.asc   # the secret key: store it, see below
gpg --armor --export > capturemann-archive-keyring.asc          # the public key (also made by make-apt-repo.sh)
gpg --gen-revoke <fingerprint> > capturemann-apt-revoke.asc    # revocation certificate: store it with the secret key
```

Store the secret key
file and the passphrase in your password manager and keep one offline backup (an encrypted USB stick); delete the scratch
`GNUPGHOME`. Never commit the secret key. The public key is not committed either: `make-apt-repo.sh` exports it into
every repository it makes (`capturemann-archive-keyring.gpg`, plus the `.asc`), and the release page carries the fingerprint.
Rotating: make a new key, publish a new keyring file (users replace `/usr/share/keyrings/capturemann-archive-keyring.gpg`),
sign with both during a transition by publishing the new `.gpg` at the same URL.

### Every release: make the repository

Sign on your machine, like the tarball signing (`sign-linux.sh`, minisign): download the two CI artifacts
(`capturemann-linux-x86_64`, `capturemann-linux-aarch64` from the **Linux package** workflow) and run

```sh
CM_APT_GPG_KEY_FILE=~/secure/capturemann-apt-secret.asc CM_APT_GPG_PASSPHRASE=... \
  packaging/linux/make-apt-repo.sh --out apt-repo --url https://apt.example.org/capturemann/ \
  path/to/downloads  path/to/previous/apt-repo      # new .deb files, and the old repository so old versions stay
```

Inputs are `.deb` files or folders (a previous repository folder keeps older versions installable). The script needs
`apt-utils`, `gnupg`, `gpgv`, `xz-utils`. The key is read only from `CM_APT_GPG_KEY_FILE` or from
`CM_APT_GPG_PRIVATE_KEY` (the armored text), the passphrase only from `CM_APT_GPG_PASSPHRASE`; it is imported into a
temporary keyring that is deleted afterwards. It writes `InRelease` (clear-signed) and `Release.gpg` (detached), the
public keyring and `capturemann.sources`, then verifies the signatures with the exported public key alone, the hashes
in `Release` and the hash of every package against the files.

Optional: the **Linux package** workflow builds the signed repository in CI too, but only when the repository secrets
`APT_SIGNING_KEY` (the armored secret key) and `APT_SIGNING_PASSPHRASE` exist and the variable `APT_REPO_URL` is set. That
puts the private key in GitHub's secret store; the project policy for minisign is "the secret key never goes to CI", so
the default recommendation is to sign locally and not set these secrets. Without them that step is skipped.

### What CI proves on every run of the workflow

Job `apt-repo` of `linux-package.yml` (both architectures, each in an `ubuntu:24.04` container of its own CPU type)
calls `packaging/linux/test-apt-repo.sh`, which builds the repository from the two freshly built `.deb` files with a
**throwaway key made in the job**, serves it over HTTP and, in the clean container: adds the key and the source exactly
as the user instructions above do, `apt update` (must be clean), `apt install capturemann`, runs `capturemann --version`,
publishes a newer version (the same file with `+aptcheck2` appended to the version), `apt upgrade`, checks that an index
modified after signing is refused, `apt remove`, `apt purge`. It runs once for the pool and once for the flat layout.
Locally, on a machine without Docker: `packaging/linux/test-apt-repo.sh --runner chroot=<ubuntu 24.04 root> pkg.deb...`
(`sudo debootstrap noble <dir>` makes the root).

### Decisions for the maintainer

1. **The host (open decision).** The URL is baked into every user's `.sources` file, so choose before the first publish.
   The `.deb` is about 105 MB (FFmpeg, llama-server and the speaker models inside), per architecture and version.

   | Option | How | For | Against |
   |---|---|---|---|
   | A. GitHub release assets, flat layout (recommended if there is no domain and no wish to run anything) | one release with a fixed tag (for example `apt`); `gh release upload apt apt-repo/* --clobber`; sources `URIs: https://github.com/craft-mann/capturemann/releases/download/apt/` `Suites: /` | free, no new account, assets up to 2 GB each, nothing to operate; apt follows the redirect | URL is tied to GitHub; the upload is not atomic (a user who runs `apt update` in the minute of an upload can see a hash mismatch and retry); old versions must be kept in the release by you; GitHub may change redirects |
   | B. Static file host with your domain (Cloudflare R2 or any object store plus CDN, or a small VPS with nginx) at `apt.<domain>` | `rclone sync apt-repo remote:bucket` (or `rsync`) after `make-apt-repo.sh`, pool layout | clean URL you control, can move later without changing users' files, atomic enough (upload pool first, index last), cheap | needs a domain and an account; something else to pay for and keep alive |
   | C. GitHub Pages | publish `apt-repo` from a `gh-pages` branch or a Pages workflow | familiar | the `.deb` files are above the 100 MB limit of a git file on a branch; the site limit is 1 GB (about four releases of two architectures), and the soft traffic limit is 100 GB/month. Only workable for the index with the `.deb` files elsewhere, which apt does not support. Not recommended |

   Recommendation: **B if you own a domain** (a one-time cost, the URL stays stable for years); otherwise **A** to start.
   The repository tooling supports both layouts, so moving from A to B later means users edit one line.
2. **The key.** Create it (section above), choose where the secret lives (recommended: your machine plus an offline backup,
   not CI), and publish the fingerprint on the release page and website.
3. **Retention.** Which older versions stay in the repository (the script keeps whatever you give it).
4. **Name of the suite.** `stable` is used (one suite, no per-Ubuntu-release suites: the same `.deb` serves Ubuntu 24.04
   and later; glibc 2.39 is the floor and `Depends:` says so, an older release refuses cleanly).

### Not done

No RPM, no `dnf` repository, no Flatpak or AppImage; no automatic publishing; no mirror; no `by-hash` index
(`Acquire-By-Hash`), which only matters when many clients update during an upload (option A or a CDN may want it later).
