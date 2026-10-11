#!/usr/bin/env bash
#
# Render the Homebrew formula and cask for one stable release.
#
#   scripts/render_homebrew.sh <version> <SHA256SUMS> <out-dir>
#
# Writes <out-dir>/Formula/gglib.rb and <out-dir>/Casks/gglib-gui.rb from the
# templates under packaging/homebrew, laid out as the mmogr/homebrew-tap tap
# expects, with every checksum taken from the release's own SHA256SUMS.
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
  *-*) echo "::error::$VERSION is a prerelease; the tap carries stable releases only" >&2; exit 1 ;;
esac

sum_of() {
  local hash
  hash="$(awk -v f="$1" '$2 == f { print $1 }' "$SUMS")"
  [ -n "$hash" ] || { echo "::error::$1 is not in $SUMS" >&2; exit 1; }
  printf '%s' "$hash"
}

CLI_MACOS="$(sum_of "gglib-${VERSION}-aarch64-apple-darwin.tar.gz")"
CLI_LINUX="$(sum_of "gglib-${VERSION}-x86_64-unknown-linux-gnu.tar.gz")"
GUI_DMG="$(sum_of "gglib-gui-${VERSION}-aarch64-apple-darwin.dmg")"

for template in "$ROOT"/packaging/homebrew/Formula/*.rb "$ROOT"/packaging/homebrew/Casks/*.rb; do
  rel="${template#"$ROOT"/packaging/homebrew/}"
  mkdir -p "$OUT_DIR/$(dirname "$rel")"
  # The template header addresses whoever edits it; the tap's copy keeps it,
  # since it says where the real source is.
  sed -e "s/@VERSION@/$VERSION/g" \
      -e "s/@SHA256_CLI_MACOS@/$CLI_MACOS/g" \
      -e "s/@SHA256_CLI_LINUX@/$CLI_LINUX/g" \
      -e "s/@SHA256_GUI_DMG@/$GUI_DMG/g" \
      "$template" > "$OUT_DIR/$rel"
  if grep -n '@[A-Z0-9_]*@' "$OUT_DIR/$rel"; then
    echo "::error::$rel has a placeholder render_homebrew.sh does not fill" >&2
    exit 1
  fi
  echo "rendered $rel $VERSION"
done
