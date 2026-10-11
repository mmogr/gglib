#!/usr/bin/env bash
#
# Build a signed apt repository from a directory of .deb files.
#
#   scripts/build_apt_repo.sh <debs-dir> <out-dir>
#
# The signing key must already be in the caller's gpg keyring: release.yml
# imports it from the APT_GPG_PRIVATE_KEY secret. Set APT_GPG_KEY_ID to pick
# one key when the keyring holds several, and APT_GPG_PASSPHRASE when the key
# has a passphrase.
#
# The result is published under the Pages site at /apt, and users add it with:
#
#   curl -fsSL https://mmogr.github.io/gglib/apt/gglib-archive-keyring.gpg \
#     | sudo tee /usr/share/keyrings/gglib-archive-keyring.gpg >/dev/null
#   echo "deb [signed-by=/usr/share/keyrings/gglib-archive-keyring.gpg] https://mmogr.github.io/gglib/apt stable main" \
#     | sudo tee /etc/apt/sources.list.d/gglib.list
#
# It holds the debs it was given and nothing else. Pages replaces the whole
# site on each deploy, so the repository carries only the release being
# published; apt only ever wants the newest version, and older ones stay on
# their GitHub release pages.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <debs-dir> <out-dir>" >&2
  exit 2
fi

DEBS_DIR="$(cd "$1" && pwd)"
OUT_DIR="$2"
SUITE="stable"
COMPONENT="main"

shopt -s nullglob
debs=("$DEBS_DIR"/*.deb)
if [ "${#debs[@]}" -eq 0 ]; then
  echo "::error::no .deb files in $DEBS_DIR" >&2
  exit 1
fi

gpg_args=(--batch --yes)
if [ -n "${APT_GPG_KEY_ID:-}" ]; then
  gpg_args+=(--local-user "$APT_GPG_KEY_ID")
fi
if [ -n "${APT_GPG_PASSPHRASE:-}" ]; then
  gpg_args+=(--pinentry-mode loopback --passphrase "$APT_GPG_PASSPHRASE")
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"
cd "$OUT_DIR"

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
gpg "${gpg_args[@]}" --armor --detach-sign -o "dists/$SUITE/Release.gpg" "dists/$SUITE/Release"

# The public half, in both forms: binary for /usr/share/keyrings (what
# signed-by expects), armored for anyone reading it.
export_args=()
[ -n "${APT_GPG_KEY_ID:-}" ] && export_args+=("$APT_GPG_KEY_ID")
gpg --batch --export "${export_args[@]}" > gglib-archive-keyring.gpg
gpg --batch --armor --export "${export_args[@]}" > gglib-archive-keyring.asc
[ -s gglib-archive-keyring.gpg ] || { echo "::error::no public key exported" >&2; exit 1; }

# Prove the signature before publishing it, against the exported key only.
gpgv --keyring "$PWD/gglib-archive-keyring.gpg" "dists/$SUITE/InRelease" 2>&1

echo "apt repository built in $OUT_DIR:"
find . -type f | LC_ALL=C sort
