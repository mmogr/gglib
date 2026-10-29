#!/usr/bin/env bash
# Print llama.cpp's pin: the tag PINNED_LLAMA_RELEASE names in
# crates/gglib-core/src/domain/engine_pins.rs, as `b<n>`.
#
#   llama_pin.sh
#
# It reads the constant's one line with the sed the Engine pins workflow
# (.github/workflows/engine-pins.yml) reads every pin with, so a script or
# workflow that needs the pin calls this rather than parsing Rust of its own.
# It runs from any directory, and fails when no line, or more than one,
# declares the constant.

set -euo pipefail

[ $# -eq 0 ] || { sed -n '5p' "$0" | sed 's/^# *//' >&2; exit 2; }
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FILE=crates/gglib-core/src/domain/engine_pins.rs

pin=$(sed -nE 's/.*PINNED_LLAMA_RELEASE: &str = "([^"]+)".*/\1/p' "$ROOT_DIR/$FILE")
[[ -n $pin && $pin != *$'\n'* ]] \
  || { echo "PINNED_LLAMA_RELEASE is not declared on exactly one line of $FILE" >&2; exit 1; }
printf '%s\n' "$pin"
