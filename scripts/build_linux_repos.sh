#!/usr/bin/env bash
#
# Build the signed apt, dnf and pacman repositories for one stable release.
#
#   scripts/build_linux_repos.sh <pkgs-dir> <arch-dir> <out-dir>
#
# <pkgs-dir> holds the release's .deb and .rpm files, <arch-dir> the
# .pkg.tar.zst files release.yml's `arch-packages` job built from
# packaging/aur.
#
# The signing key must already be in the caller's gpg keyring: release.yml
# imports it from the APT_GPG_PRIVATE_KEY secret. Set REPO_GPG_KEY_ID to pick
# one key when the keyring holds several, and REPO_GPG_PASSPHRASE when the
# key has a passphrase.
#
# The result is the root of the Pages site:
#
#   install.sh                       packaging/install.sh, which adds the
#                                    repository for the system's package
#                                    manager and installs gglib
#   gglib-archive-keyring.{gpg,asc}  the public key all three are signed with
#   apt/                             dists/stable, pool/
#   rpm/                             the rpms, repodata/ with repomd.xml.asc
#   arch/                            gglib.db, the packages and their .sig
#
# apt's Release and dnf's repomd.xml are signed and list every package's
# sha256, so the .deb and .rpm files stay byte for byte the ones the release
# attested. pacman wants a signature beside each package as well as the
# database, so the Arch packages each get a detached .sig.
#
# Each repository holds the packages it was given and nothing else. Pages
# replaces the whole site on each deploy, so the repositories carry only the
# release being published; a package manager only ever wants the newest
# version, and older ones stay on their GitHub release pages.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <pkgs-dir> <arch-dir> <out-dir>" >&2
  exit 2
fi

PKGS_DIR="$(cd "$1" && pwd)"
ARCH_DIR="$(cd "$2" && pwd)"
OUT_DIR="$3"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

shopt -s nullglob
debs=("$PKGS_DIR"/*.deb)
rpms=("$PKGS_DIR"/*.rpm)
pkgs=("$ARCH_DIR"/*.pkg.tar.zst)
require() {
  local ext="$1" dir="$2"
  shift 2
  [ "$#" -gt 0 ] || { echo "::error::no $ext files in $dir" >&2; exit 1; }
}
require .deb "$PKGS_DIR" "${debs[@]}"
require .rpm "$PKGS_DIR" "${rpms[@]}"
require .pkg.tar.zst "$ARCH_DIR" "${pkgs[@]}"

gpg_args=(--batch --yes)
if [ -n "${REPO_GPG_KEY_ID:-}" ]; then
  gpg_args+=(--local-user "$REPO_GPG_KEY_ID")
fi
if [ -n "${REPO_GPG_PASSPHRASE:-}" ]; then
  gpg_args+=(--pinentry-mode loopback --passphrase "$REPO_GPG_PASSPHRASE")
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
cd "$OUT_DIR"

# The public half, in both forms: binary for /usr/share/keyrings (what apt's
# signed-by expects), armored for pacman-key, dnf and anyone reading it.
KEY=gglib-archive-keyring
export_args=()
[ -n "${REPO_GPG_KEY_ID:-}" ] && export_args+=("$REPO_GPG_KEY_ID")
gpg --batch --export "${export_args[@]}" > "$KEY.gpg"
gpg --batch --armor --export "${export_args[@]}" > "$KEY.asc"
[ -s "$KEY.gpg" ] || { echo "::error::no public key exported" >&2; exit 1; }
KEYRING="$PWD/$KEY.gpg"

# Sign <file> into <file>.sig (or .asc with --armor), then prove the
# signature against the exported key alone before anything is published.
sign() {
  local file="$1" sig="$1.sig" form=()
  if [ "${2:-}" = --armor ]; then sig="$file.asc"; form=(--armor); fi
  gpg "${gpg_args[@]}" "${form[@]}" --detach-sign -o "$sig" "$file"
  gpgv --keyring "$KEYRING" "$sig" "$file" 2>&1
}

install -m 0644 "$ROOT/packaging/install.sh" install.sh

# --- apt ---------------------------------------------------------------------
SUITE="stable"
COMPONENT="main"
(
  mkdir apt
  cd apt

  # The pool is split by architecture so each index is one directory's
  # listing. apt-ftparchive's own --arch filter drops every package unless a
  # full generate config is supplied, which is more machinery than this needs.
  for deb in "${debs[@]}"; do
    arch="$(dpkg-deb --field "$deb" Architecture)"
    mkdir -p "pool/$COMPONENT/$arch"
    cp "$deb" "pool/$COMPONENT/$arch/"
  done
  mapfile -t archs < <(ls "pool/$COMPONENT")
  if printf '%s\n' "${archs[@]}" | grep -qx all; then
    echo "::error::Architecture: all packages are not supported here" >&2
    exit 1
  fi

  for arch in "${archs[@]}"; do
    dir="dists/$SUITE/$COMPONENT/binary-$arch"
    mkdir -p "$dir"
    apt-ftparchive packages "pool/$COMPONENT/$arch" > "$dir/Packages"
    [ -s "$dir/Packages" ] || { echo "::error::empty Packages index for $arch" >&2; exit 1; }
    gzip -9nk "$dir/Packages"
  done

  apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=gglib \
    -o APT::FTPArchive::Release::Label=gglib \
    -o APT::FTPArchive::Release::Suite="$SUITE" \
    -o APT::FTPArchive::Release::Codename="$SUITE" \
    -o APT::FTPArchive::Release::Components="$COMPONENT" \
    -o APT::FTPArchive::Release::Architectures="${archs[*]}" \
    release "dists/$SUITE" > "dists/$SUITE/Release.tmp"
  mv "dists/$SUITE/Release.tmp" "dists/$SUITE/Release"

  gpg "${gpg_args[@]}" --clearsign -o "dists/$SUITE/InRelease" "dists/$SUITE/Release"
  gpgv --keyring "$KEYRING" "dists/$SUITE/InRelease" 2>&1
  # Release.gpg is the detached form older apt reads.
  gpg "${gpg_args[@]}" --armor --detach-sign -o "dists/$SUITE/Release.gpg" "dists/$SUITE/Release"
)

# --- dnf ---------------------------------------------------------------------
# gz rather than createrepo_c's zstd default, which older dnf and zypper
# cannot read, and no sqlite copies of the metadata, which only yum read.
(
  mkdir rpm
  cp "${rpms[@]}" rpm/
  createrepo_c --no-database --general-compress-type gz rpm
  sign rpm/repodata/repomd.xml --armor
)

# --- pacman ------------------------------------------------------------------
# The package signatures go into the database (--include-sigs) as well as
# beside the packages, so pacman reads them from the one download. repo-add
# is pacman's, so off Arch it runs in an Arch container; it signs nothing
# itself, which keeps the key and its passphrase in this shell's gpg.
(
  mkdir arch
  cp "${pkgs[@]}" arch/
  cd arch
  for pkg in *.pkg.tar.zst; do
    sign "$pkg"
  done
  if command -v repo-add >/dev/null 2>&1; then
    repo-add --include-sigs gglib.db.tar.gz ./*.pkg.tar.zst
  else
    docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/repo" -w /repo archlinux:base-devel \
      sh -c 'repo-add --include-sigs gglib.db.tar.gz ./*.pkg.tar.zst'
  fi
  # pacman fetches gglib.db and gglib.db.sig; repo-add made the first a link
  # to the tarball, and the signature's link follows it. Pages serves the
  # links' targets: upload-pages-artifact dereferences them.
  for db in gglib.db gglib.files; do
    sign "$db.tar.gz"
    ln -sf "$db.tar.gz.sig" "$db.sig"
  done
)

fpr="$(gpg --batch --with-colons --import-options show-only --import "$KEYRING" | awk -F: '$1 == "fpr" { print $10; exit }')"
echo "Linux repositories built in $OUT_DIR, signed by $fpr:"
find . \( -type f -o -type l \) | LC_ALL=C sort
