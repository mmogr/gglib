#!/usr/bin/env bash
# File-size check: stops when a file is longer than the size recorded for it,
# or crosses the budget with no size recorded.
#
# The budget is a guide, not a limit. It is there so that a file which has
# taken on a second job is noticed. Stopping asks one question, whether the
# file is still one thing, and CONTRIBUTING's "File size" says what follows
# from each answer: split at the seam, or keep the file whole and raise its
# row. Nothing is to be shaped to fit the number.
#
# Usage: ./scripts/check_file_size.sh <rust|ts> <baseline> [--update]
#
#   rust  every *.rs under crates/ and src-tauri/
#   ts    every *.ts, *.tsx and *.css under src/, except src/types/generated/
#
# `make enforce` runs it once per language, each with its own baseline:
#
#   ./scripts/check_file_size.sh rust scripts/rust-complexity-baseline.txt
#   ./scripts/check_file_size.sh ts scripts/ts-complexity-baseline.txt
#
# A ratchet and not a threshold. Well over a hundred files are past the
# budget, and many of them are one thing. A gate on size would fail on every
# commit and be switched off within a day, which is how a check becomes
# decorative.
#
# So this checks the derivative instead of the value. The baseline holds one
# row per file over the budget, `<path> <lines>`. A file with a row may shrink
# freely and stops the check when it is longer than its row. A file without a
# row stops it as soon as it is over the budget.
#
# A file that is one thing and grew has its row raised, or added, by hand: one
# row, one decision, and the diff shows the number going up. `--update`
# rewrites every row to the tree's sizes instead, which on a tree that fails
# records every growth in it at once. On a tree that passes it can only lower
# a row or drop one, and the check says when there is something to lower.

set -euo pipefail

THRESHOLD=300
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  echo "usage: $0 <rust|ts> <baseline> [--update]" >&2
  exit 2
}

[ "$#" -ge 2 ] && [ "$#" -le 3 ] || usage
LANGUAGE="$1"
BASELINE="$2"
MODE="${3:-}"
[ -z "$MODE" ] || [ "$MODE" = "--update" ] || usage

case "$LANGUAGE" in
  rust) LABEL="Rust" ;;
  ts) LABEL="TypeScript/CSS" ;;
  *) usage ;;
esac

# `wc -l` over every file the language covers.
count_lines() {
  case "$LANGUAGE" in
    rust)
      find "$ROOT_DIR/crates" "$ROOT_DIR/src-tauri" -name "*.rs" \
        -not -path "*/target/*" -exec wc -l {} +
      ;;
    ts)
      # `types/generated/` is ts-rs output. The budget exists to prompt a
      # split, and there is nothing to split: the file is as long as the Rust
      # type is, and the Rust ratchet covers the source it is generated from.
      find "$ROOT_DIR/src" \( -name "*.ts" -o -name "*.tsx" -o -name "*.css" \) \
        -not -path "*/node_modules/*" -not -path "*/types/generated/*" -exec wc -l {} +
      ;;
  esac
}

# `<path> <lines>` for each of those files, the path relative to the repo root.
# `LC_ALL=C` so the order, which `--update` writes, is the same on every machine.
sizes() {
  count_lines \
    | awk -v root="$ROOT_DIR/" '$2 != "total" {
        path = $2
        if (index(path, root) == 1) path = substr(path, length(root) + 1)
        print path, $1
      }' \
    | LC_ALL=C sort
}

if [ "$MODE" = "--update" ]; then
  sizes | awk -v budget="$THRESHOLD" '$2 + 0 > budget + 0' > "$BASELINE"
  echo "✅ baseline updated: $(wc -l < "$BASELINE" | tr -d ' ') files over ${THRESHOLD} LOC"
  exit 0
fi

if [ ! -f "$BASELINE" ]; then
  echo "❌ no baseline at $BASELINE"
  echo "   Run $0 $LANGUAGE $BASELINE --update to create it."
  exit 1
fi

echo "Checking $LABEL file sizes against their baseline (budget ${THRESHOLD} LOC, a guide)..."
echo "================================================"

# awk exits 10 when a file grew or crossed, and 11 when it saw no file over the
# budget. Any other failure is the scan's own: `find`, or awk itself.
status=0
sizes | awk -v budget="$THRESHOLD" -v baseline="$BASELINE" '
  BEGIN {
    while ((getline row < baseline) > 0) {
      if (split(row, field, " ") >= 2) recorded[field[1]] = field[2] + 0
    }
  }
  { size[$1] = $2 + 0 }
  size[$1] > budget {
    over++
    if (!($1 in recorded)) {
      printf "❌ %s: %d LOC — crossed the %d LOC budget, and has no baseline row\n", $1, size[$1], budget
      failed = 1
    } else if (size[$1] > recorded[$1]) {
      printf "❌ %s: %d LOC — longer than its baseline row, %d\n", $1, size[$1], recorded[$1]
      failed = 1
    }
  }
  END {
    print ""
    if (failed) exit 10
    if (!over) exit 11
    # A row above its file: the file shrank, went back under the budget, or is
    # gone. It is room to grow back that nothing uses, so say so, and pass.
    for (path in recorded) {
      if (!(path in size) || size[path] <= budget || size[path] < recorded[path]) loose++
    }
    if (loose) {
      printf "ℹ️  %d baseline row(s) are above their file: it shrank, or is gone.\n", loose
      print "   This tree passes, so --update now lowers or drops them and raises nothing."
    }
    printf "✅ no file is longer than its baseline row, and none crossed the budget without one (%d over the budget)\n", over
  }
' || status=$?

case "$status" in
  0) ;;
  10)
    echo "❌ $LABEL file-size check stopped. The budget is a guide: is each file above still one thing?"
    echo "   It has taken on a second job: split it at that seam."
    echo "   It is one thing that grew: keep it whole, and raise its row in $BASELINE"
    echo "   by hand, or add one, so the diff shows the number going up."
    echo "   Never add a sibling file to get under the number. CONTRIBUTING.md, \"File size\"."
    exit 1
    ;;
  11)
    # Liveness. A ratchet that scans nothing reports exactly what a ratchet
    # that found no growth reports.
    echo "❌ no $LABEL file over ${THRESHOLD} LOC was found"
    echo "   Either every file is under the budget, in which case this check has"
    echo "   nothing left to compare, or the scan is broken."
    exit 1
    ;;
  *)
    echo "❌ the $LABEL file-size scan itself failed (exit $status)"
    exit 1
    ;;
esac
