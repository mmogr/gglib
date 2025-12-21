#!/bin/bash
# generate_module_tables.sh - Regenerate module badge tables in READMEs
#
# This script finds all README.md files with <!-- module-table:start/end --> markers
# and regenerates the badge tables based on the actual .rs files and subdirectories.
#
# Usage:
#   ./scripts/generate_module_tables.sh           # Update all READMEs
#   ./scripts/generate_module_tables.sh --check   # Check if tables need updating (CI mode)
#   ./scripts/generate_module_tables.sh --dry-run # Show what would change

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CRATES_DIR="$PROJECT_ROOT/crates"

MODE="update"
CHANGED=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --check)
            MODE="check"
            shift
            ;;
        --dry-run)
            MODE="dry-run"
            shift
            ;;
        --help)
            echo "Usage: $0 [--check|--dry-run]"
            echo ""
            echo "Modes:"
            echo "  (default)   Update all README tables in-place"
            echo "  --check     Exit with error if tables need updating"
            echo "  --dry-run   Show what would change without writing"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

# Badge URL base
BADGE_BASE="https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/mmogr/gglib/badges"

# Generate a table row for a module
generate_row() {
    local MODULE_NAME="$1"      # e.g., "domain.rs" or "handlers/"
    local BADGE_PREFIX="$2"     # e.g., "gglib-core-domain"
    local LINK_PATH="$3"        # e.g., "src/domain.rs" or "src/handlers/"
    
    # Strip .rs extension from link target for rustdoc compatibility
    # Keep the extension in the display name for clarity
    # This makes rustdoc generate module links (add/index.html) instead of file links (add.rs)
    local LINK_TARGET="${LINK_PATH%.rs}"
    
    # Display name with link
    local DISPLAY="[\`$MODULE_NAME\`]($LINK_TARGET)"
    
    # Badge URLs (Tests excluded - only generated per-module, not per-file)
    local LOC_BADGE="![]($BADGE_BASE/$BADGE_PREFIX-loc.json)"
    local COMPLEXITY_BADGE="![]($BADGE_BASE/$BADGE_PREFIX-complexity.json)"
    local COVERAGE_BADGE="![]($BADGE_BASE/$BADGE_PREFIX-coverage.json)"
    
    echo "| $DISPLAY | $LOC_BADGE | $COMPLEXITY_BADGE | $COVERAGE_BADGE |"
}

# Generate table content for a directory
generate_table_for_dir() {
    local DIR="$1"
    local BADGE_PREFIX_BASE="$2"
    local CRATE_NAME="$3"
    local LINK_PREFIX="$4"      # e.g., "src/" for top-level READMEs, "" for subdir READMEs
    
    echo "| Module | LOC | Complexity | Coverage |"
    echo "|--------|-----|------------|----------|"
    
    # Find .rs files (excluding mod.rs, lib.rs, main.rs)
    for RS_FILE in "$DIR"/*.rs; do
        if [[ -f "$RS_FILE" ]]; then
            local BASENAME="$(basename "$RS_FILE")"
            local MODNAME="${BASENAME%.rs}"
            if [[ "$MODNAME" != "mod" && "$MODNAME" != "lib" && "$MODNAME" != "main" ]]; then
                generate_row "$BASENAME" "$BADGE_PREFIX_BASE-$MODNAME" "$LINK_PREFIX$BASENAME"
            fi
        fi
    done
    
    # Find subdirectories with mod.rs
    # For subdirectories, use simplified naming: just {crate}-{subdir-name}
    # This matches the CI badge naming convention for directory aggregates
    for SUBDIR in "$DIR"/*/; do
        if [[ -d "$SUBDIR" ]]; then
            local SUBNAME="$(basename "$SUBDIR")"
            if [[ -f "$SUBDIR/mod.rs" ]] || [[ -f "$SUBDIR/lib.rs" ]]; then
                generate_row "$SUBNAME/" "$CRATE_NAME-$SUBNAME" "$LINK_PREFIX$SUBNAME/"
            fi
        fi
    done
}

# Process a single README file
process_readme() {
    local README="$1"
    local README_DIR="$(dirname "$README")"
    
    # Determine badge prefix from path
    local REL_PATH="${README_DIR#$CRATES_DIR/}"
    local CRATE_NAME="${REL_PATH%%/*}"
    
    # Figure out the src directory and badge prefix
    local SRC_DIR=""
    local BADGE_PREFIX=""
    local LINK_PREFIX=""
    
    if [[ "$README_DIR" == "$CRATES_DIR/$CRATE_NAME" ]]; then
        # Top-level crate README - modules are in src/
        SRC_DIR="$README_DIR/src"
        BADGE_PREFIX="$CRATE_NAME"
        LINK_PREFIX="src/"
    else
        # Subdirectory README - modules are in same directory
        SRC_DIR="$README_DIR"
        # Extract submodule path for badge prefix
        local SUBPATH="${README_DIR#$CRATES_DIR/$CRATE_NAME/src/}"
        # Convert path to badge prefix (e.g., handlers/download -> gglib-cli-download)
        local LAST_PART="${SUBPATH##*/}"
        BADGE_PREFIX="$CRATE_NAME-$LAST_PART"
        LINK_PREFIX=""
    fi
    
    if [[ ! -d "$SRC_DIR" ]]; then
        return
    fi
    
    # Check if README has module-table markers
    if ! grep -q "<!-- module-table:start -->" "$README"; then
        return
    fi
    
    # Generate new table content
    local NEW_TABLE
    NEW_TABLE=$(generate_table_for_dir "$SRC_DIR" "$BADGE_PREFIX" "$CRATE_NAME" "$LINK_PREFIX")
    
    # Extract current table content
    local CURRENT_TABLE
    CURRENT_TABLE=$(sed -n '/<!-- module-table:start -->/,/<!-- module-table:end -->/p' "$README" | \
                    sed '1d;$d')
    
    # Compare tables
    if [[ "$NEW_TABLE" != "$CURRENT_TABLE" ]]; then
        CHANGED=1
        
        case "$MODE" in
            check)
                echo "NEEDS UPDATE: $README"
                ;;
            dry-run)
                echo "Would update: $README"
                echo "---"
                echo "$NEW_TABLE"
                echo "---"
                ;;
            update)
                echo "Updating: $README"
                
                # Create temp file with updated content
                local TEMP_FILE
                TEMP_FILE=$(mktemp)
                local TABLE_FILE
                TABLE_FILE=$(mktemp)
                
                # Write new table to temp file
                echo "$NEW_TABLE" > "$TABLE_FILE"
                
                # Use awk to replace content between markers
                awk -v table_file="$TABLE_FILE" '
                    /<!-- module-table:start -->/ {
                        print
                        while ((getline line < table_file) > 0) {
                            print line
                        }
                        close(table_file)
                        in_table = 1
                        next
                    }
                    /<!-- module-table:end -->/ {
                        in_table = 0
                    }
                    !in_table { print }
                ' "$README" > "$TEMP_FILE"
                
                mv "$TEMP_FILE" "$README"
                rm -f "$TABLE_FILE"
                ;;
        esac
    fi
}

# Find all READMEs with module-table markers
echo "Scanning for READMEs with module-table markers..."
find "$CRATES_DIR" -name "README.md" -type f | while read -r README; do
    process_readme "$README"
done

if [[ "$MODE" == "check" && "$CHANGED" -eq 1 ]]; then
    echo ""
    echo "ERROR: Some module tables need updating. Run:"
    echo "  ./scripts/generate_module_tables.sh"
    exit 1
elif [[ "$MODE" == "check" ]]; then
    echo "All module tables are up to date."
fi
