#!/usr/bin/env bash
# Check file complexity - flag large files that should be decomposed
# Usage: ./scripts/check_file_complexity.sh [threshold_loc]

set -e

THRESHOLD=${1:-300}
FOUND_LARGE_FILES=false

echo "Checking for files exceeding ${THRESHOLD} LOC..."
echo "================================================"

# Check TypeScript/TSX files
while IFS= read -r file; do
  if [ -f "$file" ]; then
    LOC=$(wc -l < "$file" | tr -d ' ')
    if [ "$LOC" -gt "$THRESHOLD" ]; then
      echo "⚠️  $file: $LOC LOC (exceeds $THRESHOLD)"
      FOUND_LARGE_FILES=true
    fi
  fi
done < <(find src -type f \( -name "*.tsx" -o -name "*.ts" \) -not -path "*/node_modules/*")

# Check CSS modules
while IFS= read -r file; do
  if [ -f "$file" ]; then
    LOC=$(wc -l < "$file" | tr -d ' ')
    if [ "$LOC" -gt "$THRESHOLD" ]; then
      echo "⚠️  $file: $LOC LOC (exceeds $THRESHOLD)"
      FOUND_LARGE_FILES=true
    fi
  fi
done < <(find src -type f \( -name "*.css" -o -name "*.module.css" \) -not -path "*/node_modules/*")

echo ""
if [ "$FOUND_LARGE_FILES" = true ]; then
  echo "❌ Found files exceeding ${THRESHOLD} LOC complexity budget"
  echo "   Consider breaking them into smaller, focused components."
  exit 1
else
  echo "✅ All files within ${THRESHOLD} LOC complexity budget"
  exit 0
fi
