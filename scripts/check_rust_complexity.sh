#!/usr/bin/env bash
# Rust file-size ratchet: nothing over the budget may grow, and nothing new
# may join.
#
# Usage: ./scripts/check_rust_complexity.sh [--update]
#
# Why a ratchet and not a threshold. The repo's stated constraint is small,
# low-complexity files, and `check_file_complexity.sh` enforces 300 LOC — but
# only over `src/` (TypeScript and CSS). Rust was never checked at all, and
# 175 files are already over that line, the largest at 2574. A hard gate would
# fail on every commit and be switched off within a day, which is how a
# constraint becomes decorative.
#
# So this checks the derivative instead of the value. A file already over
# budget is recorded in the baseline at its current size and may shrink freely;
# growing it fails. A file not in the baseline may not cross the line at all.
# The effect is that the budget can only ever be approached, never retreated
# from — and the files this repo is actively working in (forward.rs, server.rs,
# the pipeline) stop quietly accumulating.
#
# `--update` rewrites the baseline. Use it when a file legitimately grew and
# the growth is the point; the diff then shows the number going up, which is a
# reviewable fact rather than an invisible one.

set -euo pipefail

THRESHOLD=300
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$ROOT_DIR/scripts/rust-complexity-baseline.txt"

# `src-tauri` is a workspace member with several thousand lines of Rust and was
# outside this scan until now — which is where the desktop refactor's growth
# landed. Its two over-budget files enter the baseline at their current size,
# like every other file did when this script was written.
current_sizes() {
  find "$ROOT_DIR/crates" "$ROOT_DIR/src-tauri" -name "*.rs" -not -path "*/target/*" -exec wc -l {} + \
    | awk -v root="$ROOT_DIR/" '$2 != "total" && $1 > '"$THRESHOLD"' {
        path = $2; sub(root, "", path); print path" "$1
      }' \
    | LC_ALL=C sort
}

if [ "${1:-}" = "--update" ]; then
  current_sizes > "$BASELINE"
  echo "✅ baseline updated: $(wc -l < "$BASELINE" | tr -d ' ') files over ${THRESHOLD} LOC"
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "❌ missing baseline: $BASELINE (run with --update to create it)"
  exit 1
fi

echo "Checking Rust file-size ratchet (budget ${THRESHOLD} LOC)..."
echo "================================================"

failed=false
while read -r path loc; do
  [ -z "$path" ] && continue
  baseline_loc=$(awk -v p="$path" '$1 == p {print $2}' "$BASELINE")
  if [ -z "$baseline_loc" ]; then
    echo "❌ $path: $loc LOC — new file over the ${THRESHOLD} LOC budget"
    failed=true
  elif [ "$loc" -gt "$baseline_loc" ]; then
    echo "❌ $path: $loc LOC — grew from $baseline_loc, already over budget"
    failed=true
  fi
done < <(current_sizes)

echo ""
if [ "$failed" = true ]; then
  echo "❌ Rust complexity ratchet failed."
  echo "   Split the file, or run ./scripts/check_rust_complexity.sh --update"
  echo "   to record the growth deliberately."
  exit 1
fi

echo "✅ no file over budget grew, and nothing new crossed it"
exit 0
