#!/usr/bin/env bash
#
# Package a built `gglib` CLI binary as a Debian package.
#
#   scripts/build_cli_deb.sh <binary> <version> <out-dir>
#
# Writes <out-dir>/gglib-<version>-x86_64-unknown-linux-gnu.deb, named like
# every other release asset so the checksum and attestation steps take it with
# no special case. The apt repository (build_apt_repo.sh) cares only about the
# control fields, never the filename.
#
# Depends is computed by dpkg-shlibdeps from the libraries the binary actually
# links, not written by hand, so it cannot drift from the build: on the
# ubuntu-24.04 image that is libc6 (>= 2.39), libasound2t64 and libgcc-s1,
# which is also the honest statement of where this binary runs.
#
# The Debian version spells a prerelease with `~`, so 0.26.0~rc.1 sorts before
# 0.26.0. With `-` dpkg would read "rc.1" as the Debian revision and order it
# after the release.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <binary> <version> <out-dir>" >&2
  exit 2
fi

BINARY="$1"
VERSION="$2"
OUT_DIR="$3"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

[ -x "$BINARY" ] || { echo "::error::$BINARY is not an executable file" >&2; exit 1; }

DEB_VERSION="${VERSION/-/\~}"
ARCH="$(dpkg --print-architecture)"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
PKG="$WORK/pkg"

install -Dm755 "$BINARY" "$PKG/usr/bin/gglib"
install -Dm644 "$ROOT/LICENSE" "$PKG/usr/share/doc/gglib/copyright"

# dpkg-shlibdeps insists on a debian/control naming the package it is
# resolving for, and reads nothing else from it.
mkdir -p "$WORK/debian"
printf 'Source: gglib\n\nPackage: gglib\nArchitecture: any\n' > "$WORK/debian/control"
DEPENDS="$(cd "$WORK" && dpkg-shlibdeps -O "pkg/usr/bin/gglib" | sed -n 's/^shlibs:Depends=//p')"
[ -n "$DEPENDS" ] || { echo "::error::dpkg-shlibdeps produced no Depends for $BINARY" >&2; exit 1; }

INSTALLED_KB="$(du -sk "$PKG/usr" | cut -f1)"

mkdir -p "$PKG/DEBIAN"
cat > "$PKG/DEBIAN/control" <<EOF
Package: gglib
Version: $DEB_VERSION
Architecture: $ARCH
Maintainer: mmogr <noreply@github.com>
Installed-Size: $INSTALLED_KB
Depends: $DEPENDS
Section: utils
Priority: optional
Homepage: https://github.com/mmogr/gglib
Description: Local model runtime and OpenAI-compatible proxy for llama.cpp
 gglib downloads GGUF models, installs and runs llama.cpp for them, and serves
 every installed model behind one OpenAI-compatible endpoint. This package is
 the gglib command-line tool; the desktop app is packaged separately.
EOF

mkdir -p "$OUT_DIR"
OUT="$OUT_DIR/gglib-${VERSION}-x86_64-unknown-linux-gnu.deb"
dpkg-deb --root-owner-group --build "$PKG" "$OUT" >/dev/null
echo "built $(basename "$OUT")"
dpkg-deb --field "$OUT" Package Version Architecture Depends
