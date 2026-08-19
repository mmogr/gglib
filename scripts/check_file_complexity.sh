#!/usr/bin/env bash
# TypeScript/CSS file-size ratchet: nothing over the budget may grow, and
# nothing new may join.
#
# Usage: ./scripts/check_file_complexity.sh [--update]
#
# Why this changed shape. It was a hard 300-LOC threshold over `src/`, and it
# ran nowhere — not `make enforce`, not `make pre-commit`, not CI. It could
# not: 25 files under `src/` are already over the line, the largest at 755, so
# wiring it in would have failed every commit. Meanwhile CONTRIBUTING.md
# documented it as the rule, and `check_rust_complexity.sh` cited it as the
# precedent it was modelled on — a rule everything pointed at and nothing ran.
#
# So it is the same ratchet its Rust sibling is, for the same reason: a file
# already over budget is recorded at its current size and may shrink freely;
# growing it fails. A file not in the baseline may not cross the line at all.
# The budget can only ever be approached, never retreated from.
#
# `--update` rewrites the baseline. Use it when a file legitimately grew and
# the growth is the point; the diff then shows the number going up, which is a
# reviewable fact rather than an invisible one.

set -euo pipefail

THRESHOLD=300
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BASELINE="$ROOT_DIR/scripts/ts-complexity-baseline.txt"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

current_sizes() {
  # `types/generated/` is ts-rs output. The budget exists to prompt a split,
  # and there is nothing to split — the file is however long the Rust type is,
  # and the only way to shorten it is to change the wire. Rust's own ratchet
  # already covers the source these are generated from.
  find "$ROOT_DIR/src" \( -name "*.ts" -o -name "*.tsx" -o -name "*.css" \) \
    -not -path "*/node_modules/*" -not -path "*/types/generated/*" -exec wc -l {} + \
    | awk -v root="$ROOT_DIR/" '$2 != "total" && $1 > '"$THRESHOLD"' {
        path = $2; sub(root, "", path); print path" "$1
      }' \
    | sort
}

if [ "${1:-}" = "--update" ]; then
  current_sizes > "$BASELINE"
  echo -e "${GREEN}✅ baseline updated:${NC} $(wc -l < "$BASELINE" | tr -d ' ') files over ${THRESHOLD} LOC"
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo -e "${RED}❌ no baseline at ${BASELINE}${NC}"
  echo "   Run ./scripts/check_file_complexity.sh --update to create it."
  exit 1
fi

echo "Checking TypeScript/CSS file-size ratchet (budget ${THRESHOLD} LOC)..."
echo "================================================"

failed=false
scanned=0

while read -r path size; do
  [ -z "$path" ] && continue
  scanned=$((scanned + 1))
  recorded=$(awk -v p="$path" '$1 == p {print $2}' "$BASELINE")

  if [ -z "$recorded" ]; then
    echo -e "${RED}❌${NC} $path: $size LOC — new file over the ${THRESHOLD} LOC budget"
    failed=true
  elif [ "$size" -gt "$recorded" ]; then
    echo -e "${RED}❌${NC} $path: $size LOC — grew from $recorded, already over budget"
    failed=true
  elif [ "$size" -lt "$recorded" ]; then
    echo -e "${GREEN}✓${NC} $path: $size LOC — shrank from $recorded"
  fi
done < <(current_sizes)

echo ""
if $failed; then
  cat <<'MSG'
❌ TypeScript/CSS complexity ratchet failed.

Split the component, extract the hook, or move the types out — or run
./scripts/check_file_complexity.sh --update to record the growth deliberately,
which makes it a visible line in the diff rather than an invisible one.
MSG
  exit 1
fi

# Liveness. A ratchet that scans nothing reports exactly what a ratchet that
# found no growth reports.
if [ "$scanned" -eq 0 ]; then
  echo -e "${RED}❌ no files were scanned${NC}"
  echo "   Either every file under src/ is under ${THRESHOLD} LOC — in which case"
  echo "   turn this back into a hard threshold — or the scan is broken."
  exit 1
fi

echo -e "${GREEN}✅ no file over budget grew, and nothing new crossed it${NC} (${scanned} scanned)"
