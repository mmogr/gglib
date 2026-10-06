#!/usr/bin/env bash
# File-size ratchet: no file over the budget may grow, and no file under it may
# cross.
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
# A ratchet and not a threshold. The repo's constraint is small files, and well
# over a hundred are already past the budget. A hard gate would fail on every
# commit and be switched off within a day, which is how a constraint becomes
# decorative.
#
# So this checks the derivative instead of the value. The baseline holds one
# row per file over the budget, `<path> <lines>`. A file with a row may shrink
# freely and fails when it is longer than its row. A file without a row fails
# as soon as it is over the budget.
#
# `--update` rewrites the baseline to the tree's sizes. On a tree that fails,
# it records the growth: use it when the growth is the point, and the diff then
# shows the number going up. On a tree that passes it can only lower a row or
# drop one, and the check says when there is something to lower.

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

echo "Checking $LABEL file-size ratchet (budget ${THRESHOLD} LOC)..."
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
      printf "❌ %s: %d LOC — new file over the %d LOC budget\n", $1, size[$1], budget
      failed = 1
    } else if (size[$1] > recorded[$1]) {
      printf "❌ %s: %d LOC — grew from %d, already over budget\n", $1, size[$1], recorded[$1]
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
    printf "✅ no file over budget grew, and nothing new crossed it (%d over budget)\n", over
  }
' || status=$?

case "$status" in
  0) ;;
  10)
    echo "❌ $LABEL file-size ratchet failed."
    echo "   Split the file, or run $0 $LANGUAGE $BASELINE --update"
    echo "   to record the growth deliberately: the diff then shows the number going up."
    exit 1
    ;;
  11)
    # Liveness. A ratchet that scans nothing reports exactly what a ratchet
    # that found no growth reports.
    echo "❌ no $LABEL file over ${THRESHOLD} LOC was found"
    echo "   Either every file is under the budget, in which case make this a hard"
    echo "   threshold, or the scan is broken."
    exit 1
    ;;
  *)
    echo "❌ the $LABEL file-size scan itself failed (exit $status)"
    exit 1
    ;;
esac
