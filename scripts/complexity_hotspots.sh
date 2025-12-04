#!/usr/bin/env bash
# complexity_hotspots.sh - Generate a ranked list of high-complexity files
#
# Usage: ./scripts/complexity_hotspots.sh [threshold]
#   threshold: minimum complexity to report (default: 40)
#
# Requires: scc (https://github.com/boyter/scc)
#   Install: brew install scc

set -euo pipefail

THRESHOLD="${1:-40}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$REPO_ROOT"

# Check scc is installed
if ! command -v scc &> /dev/null; then
    echo "Error: scc not installed. Install with: brew install scc" >&2
    exit 1
fi

echo "=== Complexity Hotspots (threshold: ${THRESHOLD}) ==="
echo "Generated: $(date '+%Y-%m-%d %H:%M')"
echo ""

# Run scc on source directories, sorted by complexity
echo "### Rust Files (src/, src-tauri/src/)"
echo ""
printf "| %-50s | %5s | %5s | %10s |\n" "File" "Lines" "Code" "Complexity"
printf "| %-50s | %5s | %5s | %10s |\n" "$(printf '%.0s-' {1..50})" "-----" "-----" "----------"

scc --by-file --sort complexity src/ src-tauri/src/ 2>/dev/null \
    | grep -E "\.rs[[:space:]]" \
    | awk -v thresh="$THRESHOLD" '{
        complexity=$NF; 
        if(complexity+0 > thresh) {
            # Extract filename from truncated path
            file=$1;
            lines=$2;
            code=$5;
            printf "| %-50s | %5s | %5s | %10s |\n", file, lines, code, complexity
        }
    }'

echo ""
echo "### TypeScript/TSX Files (src/)"
echo ""
printf "| %-50s | %5s | %5s | %10s |\n" "File" "Lines" "Code" "Complexity"
printf "| %-50s | %5s | %5s | %10s |\n" "$(printf '%.0s-' {1..50})" "-----" "-----" "----------"

scc --by-file --sort complexity src/ 2>/dev/null \
    | grep -E "\.tsx?[[:space:]]" \
    | awk -v thresh="$THRESHOLD" '{
        complexity=$NF; 
        if(complexity+0 > thresh) {
            file=$1;
            lines=$2;
            code=$5;
            printf "| %-50s | %5s | %5s | %10s |\n", file, lines, code, complexity
        }
    }'

echo ""
echo "### Summary"
total_rust=$(scc --by-file --sort complexity src/ src-tauri/src/ 2>/dev/null | grep -E "\.rs[[:space:]]" | awk -v thresh="$THRESHOLD" '$NF+0 > thresh' | wc -l | tr -d ' ')
total_ts=$(scc --by-file --sort complexity src/ 2>/dev/null | grep -E "\.tsx?[[:space:]]" | awk -v thresh="$THRESHOLD" '$NF+0 > thresh' | wc -l | tr -d ' ')
echo "- Rust files above threshold: $total_rust"
echo "- TS/TSX files above threshold: $total_ts"
echo "- Total: $((total_rust + total_ts))"
