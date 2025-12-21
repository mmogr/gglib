#!/bin/bash
# generate_badges_for_crate.sh - Generate badge JSONs for all modules in a crate
#
# This script discovers modules in a crate and generates badge JSON files
# for LOC, complexity, coverage, and tests metrics.
#
# Usage (in CI workflow):
#   ./scripts/generate_badges_for_crate.sh gglib-core loc
#   ./scripts/generate_badges_for_crate.sh gglib-core complexity  
#   ./scripts/generate_badges_for_crate.sh gglib-core coverage --lcov-file path/to/lcov.info
#   ./scripts/generate_badges_for_crate.sh gglib-core tests --test-file path/to/test-output.txt

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CRATES_DIR="$PROJECT_ROOT/crates"

usage() {
    echo "Usage: $0 <crate-name> <metric> [options]"
    echo ""
    echo "Metrics:"
    echo "  loc         - Lines of code"
    echo "  complexity  - Cyclomatic complexity"
    echo "  coverage    - Code coverage (requires --lcov-file)"
    echo "  tests       - Test results (requires --test-file)"
    echo ""
    echo "Options:"
    echo "  --lcov-file <path>   Path to lcov.info file (for coverage)"
    echo "  --test-file <path>   Path to test output file (for tests)"
    echo "  --output-dir <path>  Output directory for badge JSONs (default: ./badges)"
    exit 1
}

if [[ $# -lt 2 ]]; then
    usage
fi

CRATE_NAME="$1"
METRIC="$2"
shift 2

LCOV_FILE=""
TEST_FILE=""
OUTPUT_DIR="./badges"

# Parse remaining arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --lcov-file)
            LCOV_FILE="$2"
            shift 2
            ;;
        --test-file)
            TEST_FILE="$2"
            shift 2
            ;;
        --output-dir)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Ensure output directory exists
mkdir -p "$OUTPUT_DIR"

# Get crate source directory
CRATE_SRC="$CRATES_DIR/$CRATE_NAME/src"
if [[ ! -d "$CRATE_SRC" ]]; then
    echo "Error: Crate source directory not found: $CRATE_SRC" >&2
    exit 1
fi

# Helper to determine badge color
get_color() {
    local VALUE=$1
    local TYPE=${2:-percent}  # percent or count
    
    if [[ "$TYPE" == "percent" ]]; then
        if [[ $VALUE -gt 90 ]]; then echo "brightgreen";
        elif [[ $VALUE -gt 80 ]]; then echo "green";
        elif [[ $VALUE -gt 70 ]]; then echo "yellowgreen";
        elif [[ $VALUE -gt 60 ]]; then echo "yellow";
        else echo "red"; fi
    else
        # For counts (LOC, complexity)
        if [[ $VALUE -le 100 ]]; then echo "brightgreen";
        elif [[ $VALUE -le 300 ]]; then echo "green";
        elif [[ $VALUE -le 500 ]]; then echo "yellowgreen";
        elif [[ $VALUE -le 1000 ]]; then echo "yellow";
        else echo "orange"; fi
    fi
}

# Generate LOC badge for a module
generate_loc_badge() {
    local MODULE_PATH="$1"
    local BADGE_PREFIX="$2"
    
    local LOC=0
    if [[ -d "$MODULE_PATH" ]]; then
        LOC=$(find "$MODULE_PATH" -name "*.rs" -exec cat {} \; 2>/dev/null | wc -l | tr -d ' ')
    elif [[ -f "$MODULE_PATH" ]]; then
        LOC=$(wc -l < "$MODULE_PATH" | tr -d ' ')
    fi
    
    local COLOR=$(get_color $LOC count)
    echo "{\"schemaVersion\":1,\"label\":\"LOC\",\"message\":\"$LOC\",\"color\":\"$COLOR\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-loc.json"
    echo "  $BADGE_PREFIX: $LOC lines"
}

# Generate complexity badge (approximation based on control flow keywords)
generate_complexity_badge() {
    local MODULE_PATH="$1"
    local BADGE_PREFIX="$2"
    
    local COMPLEXITY=0
    if [[ -d "$MODULE_PATH" ]]; then
        COMPLEXITY=$(find "$MODULE_PATH" -name "*.rs" -exec grep -c -E '(if |match |for |while |loop |&&|\|\||fn )' {} \; 2>/dev/null | awk '{s+=$1} END {print s+0}')
    elif [[ -f "$MODULE_PATH" ]]; then
        COMPLEXITY=$(grep -c -E '(if |match |for |while |loop |&&|\|\||fn )' "$MODULE_PATH" 2>/dev/null || echo "0")
    fi
    
    local COLOR=$(get_color $COMPLEXITY count)
    echo "{\"schemaVersion\":1,\"label\":\"complexity\",\"message\":\"$COMPLEXITY\",\"color\":\"$COLOR\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-complexity.json"
    echo "  $BADGE_PREFIX: $COMPLEXITY"
}

# Generate coverage badge from lcov file
generate_coverage_badge() {
    local MODULE_PATH="$1"
    local BADGE_PREFIX="$2"
    
    if [[ -z "$LCOV_FILE" || ! -f "$LCOV_FILE" ]]; then
        echo "{\"schemaVersion\":1,\"label\":\"cov\",\"message\":\"N/A\",\"color\":\"lightgrey\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-coverage.json"
        return
    fi
    
    # Extract module name for lcov matching
    local MODULE_NAME="${MODULE_PATH##*/}"
    MODULE_NAME="${MODULE_NAME%.rs}"
    
    # Parse lcov for this module
    local TOTAL_LINES=$(awk -v path="$MODULE_NAME" '
        /^SF:/ { in_module = ($0 ~ path) }
        in_module && /^LF:/ { split($0, a, ":"); total += a[2] }
        END { print total+0 }
    ' "$LCOV_FILE")
    
    local COVERED_LINES=$(awk -v path="$MODULE_NAME" '
        /^SF:/ { in_module = ($0 ~ path) }
        in_module && /^LH:/ { split($0, a, ":"); covered += a[2] }
        END { print covered+0 }
    ' "$LCOV_FILE")
    
    if [[ $TOTAL_LINES -gt 0 ]]; then
        local COV=$((COVERED_LINES * 100 / TOTAL_LINES))
        local COLOR=$(get_color $COV percent)
        echo "{\"schemaVersion\":1,\"label\":\"cov\",\"message\":\"${COV}%\",\"color\":\"$COLOR\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-coverage.json"
        echo "  $BADGE_PREFIX: ${COV}%"
    else
        echo "{\"schemaVersion\":1,\"label\":\"cov\",\"message\":\"0%\",\"color\":\"lightgrey\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-coverage.json"
    fi
}

# Generate tests badge from test output
generate_tests_badge() {
    local MODULE_PATH="$1"
    local BADGE_PREFIX="$2"
    
    if [[ -z "$TEST_FILE" || ! -f "$TEST_FILE" ]]; then
        echo "{\"schemaVersion\":1,\"label\":\"tests\",\"message\":\"N/A\",\"color\":\"lightgrey\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-tests.json"
        return
    fi
    
    # Extract module name for test matching
    local MODULE_NAME="${MODULE_PATH##*/}"
    MODULE_NAME="${MODULE_NAME%.rs}"
    
    # Count tests for this module
    local PASSED=$(grep -E "^test ${MODULE_NAME}::" "$TEST_FILE" 2>/dev/null | grep -c " \.\.\. ok$" || echo "0")
    local FAILED=$(grep -E "^test ${MODULE_NAME}::" "$TEST_FILE" 2>/dev/null | grep -c " \.\.\. FAILED$" || echo "0")
    local TOTAL=$((PASSED + FAILED))
    
    if [[ $TOTAL -gt 0 ]]; then
        local PCT=$((PASSED * 100 / TOTAL))
        local COLOR=$(get_color $PCT percent)
        echo "{\"schemaVersion\":1,\"label\":\"tests\",\"message\":\"${PASSED}/${TOTAL}\",\"color\":\"$COLOR\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-tests.json"
        echo "  $BADGE_PREFIX: ${PASSED}/${TOTAL}"
    else
        echo "{\"schemaVersion\":1,\"label\":\"tests\",\"message\":\"0\",\"color\":\"lightgrey\"}" > "$OUTPUT_DIR/${BADGE_PREFIX}-tests.json"
    fi
}

# Process all modules in the crate
echo "Generating $METRIC badges for $CRATE_NAME..."

# Get all modules using discover script
while IFS=: read -r CRATE MODULE_PATH BADGE_PREFIX; do
    # Determine full path
    FULL_PATH=""
    if [[ "$MODULE_PATH" == *"/"* ]]; then
        # Nested module - path is relative to src
        FULL_PATH="$CRATE_SRC/$MODULE_PATH"
    elif [[ -d "$CRATE_SRC/$MODULE_PATH" ]]; then
        # Directory module
        FULL_PATH="$CRATE_SRC/$MODULE_PATH"
    else
        # File module
        FULL_PATH="$CRATE_SRC/${MODULE_PATH}.rs"
    fi
    
    case "$METRIC" in
        loc)
            generate_loc_badge "$FULL_PATH" "$BADGE_PREFIX"
            ;;
        complexity)
            generate_complexity_badge "$FULL_PATH" "$BADGE_PREFIX"
            ;;
        coverage)
            generate_coverage_badge "$FULL_PATH" "$BADGE_PREFIX"
            ;;
        tests)
            generate_tests_badge "$FULL_PATH" "$BADGE_PREFIX"
            ;;
        *)
            echo "Unknown metric: $METRIC"
            usage
            ;;
    esac
done < <("$SCRIPT_DIR/discover_modules.sh" "$CRATE_NAME")

echo "Done generating $METRIC badges for $CRATE_NAME"
