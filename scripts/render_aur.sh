#!/usr/bin/env bash
#
# Render the AUR packages for one stable release.
#
#   scripts/render_aur.sh <version> <SHA256SUMS> <out-dir>
#
# Writes <out-dir>/<pkgname>/PKGBUILD for each template under packaging/aur,
# with pkgver and every @SHA256_*@ placeholder filled from the release's own
# SHA256SUMS, so the AUR checksums are the ones the release was attested
# with. .SRCINFO is not written here: it is makepkg's output, and release.yml
# runs makepkg --printsrcinfo on Arch for it, which also proves the PKGBUILD
# parses.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <version> <SHA256SUMS> <out-dir>" >&2
  exit 2
fi

VERSION="$1"
SUMS="$2"
OUT_DIR="$3"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

case "$VERSION" in
  *-*) echo "::error::$VERSION is a prerelease; the AUR carries stable releases only" >&2; exit 1 ;;
esac

sum_of() {
  local hash
  hash="$(awk -v f="$1" '$2 == f { print $1 }' "$SUMS")"
  [ -n "$hash" ] || { echo "::error::$1 is not in $SUMS" >&2; exit 1; }
  printf '%s' "$hash"
}

CLI_TARBALL="$(sum_of "gglib-${VERSION}-x86_64-unknown-linux-gnu.tar.gz")"
GUI_DEB="$(sum_of "gglib-gui-${VERSION}-x86_64-unknown-linux-gnu.deb")"

for template in "$ROOT"/packaging/aur/*/PKGBUILD; do
  pkg="$(basename "$(dirname "$template")")"
  mkdir -p "$OUT_DIR/$pkg"
  sed -e "s/@VERSION@/$VERSION/g" \
      -e "s/@SHA256_CLI_TARBALL@/$CLI_TARBALL/g" \
      -e "s/@SHA256_GUI_DEB@/$GUI_DEB/g" \
      "$template" > "$OUT_DIR/$pkg/PKGBUILD"
  if grep -n '@[A-Z0-9_]*@' "$OUT_DIR/$pkg/PKGBUILD"; then
    echo "::error::$pkg/PKGBUILD has a placeholder render_aur.sh does not fill" >&2
    exit 1
  fi
  echo "rendered $pkg $VERSION"
done
