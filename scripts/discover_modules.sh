#!/bin/bash
# discover_modules.sh - Auto-discover modules in crate src/ directories
#
# This script dynamically discovers .rs files and subdirectories within crate
# src/ directories, outputting a format suitable for badge generation.
#
# Usage:
#   ./scripts/discover_modules.sh                         # Discover all crates  
#   ./scripts/discover_modules.sh gglib-core              # Discover specific crate
#   ./scripts/discover_modules.sh --modules-only gglib-core  # Just module names
#   ./scripts/discover_modules.sh --format=json           # Output as JSON
#
# Output format (default):
#   CRATE:MODULE:BADGE_PREFIX
#   e.g., gglib-core:domain:gglib-core-domain
#
# With --modules-only:
#   domain ports services events paths download utils
#   (space-separated module names for shell iteration)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CRATES_DIR="$PROJECT_ROOT/crates"

FORMAT="plain"
SPECIFIC_CRATE=""
MODULES_ONLY=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --format=*)
            FORMAT="${1#*=}"
            shift
            ;;
        --modules-only)
            MODULES_ONLY=true
            shift
            ;;
        --help)
            echo "Usage: $0 [--format=plain|json] [--modules-only] [crate-name]"
            exit 0
            ;;
        *)
            SPECIFIC_CRATE="$1"
            shift
            ;;
    esac
done

# Function to output module in correct format
output_module() {
    local CRATE_NAME="$1"
    local MODULE_PATH="$2"
    local BADGE_PREFIX="$3"
    
    if [[ "$MODULES_ONLY" == true ]]; then
        # Extract just the module name (last part of path)
        local MODULE_NAME="${MODULE_PATH##*/}"
        echo -n "$MODULE_NAME "
    else
        echo "$CRATE_NAME:$MODULE_PATH:$BADGE_PREFIX"
    fi
}

# Function to discover modules in a crate's src directory
discover_crate_modules() {
    local CRATE_DIR="$1"
    local CRATE_NAME="$(basename "$CRATE_DIR")"
    local SRC_DIR="$CRATE_DIR/src"
    
    if [[ ! -d "$SRC_DIR" ]]; then
        return
    fi
    
    # Find .rs files (excluding mod.rs, lib.rs, main.rs)
    for RS_FILE in "$SRC_DIR"/*.rs; do
        if [[ -f "$RS_FILE" ]]; then
            local BASENAME="$(basename "$RS_FILE" .rs)"
            # Skip mod.rs, lib.rs, main.rs as they're entry points
            if [[ "$BASENAME" != "mod" && "$BASENAME" != "lib" && "$BASENAME" != "main" ]]; then
                output_module "$CRATE_NAME" "$BASENAME" "$CRATE_NAME-$BASENAME"
            fi
        fi
    done
    
    # Find subdirectories with mod.rs (indicating modules)
    for SUBDIR in "$SRC_DIR"/*/; do
        if [[ -d "$SUBDIR" ]]; then
            local SUBNAME="$(basename "$SUBDIR")"
            # Check if it's a proper module (has mod.rs or is declared in lib.rs)
            if [[ -f "$SUBDIR/mod.rs" ]] || [[ -f "$SUBDIR/lib.rs" ]]; then
                output_module "$CRATE_NAME" "$SUBNAME" "$CRATE_NAME-$SUBNAME"
                
                # Recursively discover nested modules (for gglib-cli handlers subfolders)
                discover_nested_modules "$SUBDIR" "$CRATE_NAME" "$SUBNAME"
            fi
        fi
    done
    
    # If modules-only mode, add newline at end
    if [[ "$MODULES_ONLY" == true ]]; then
        echo ""
    fi
}

# Function to discover nested modules (e.g., handlers/download/)
discover_nested_modules() {
    local PARENT_DIR="$1"
    local CRATE_NAME="$2"
    local PARENT_MODULE="$3"
    
    # Find subdirectories with mod.rs
    for SUBDIR in "$PARENT_DIR"/*/; do
        if [[ -d "$SUBDIR" ]]; then
            local SUBNAME="$(basename "$SUBDIR")"
            if [[ -f "$SUBDIR/mod.rs" ]]; then
                # Use underscore for badge name (gglib-cli-check_deps)
                output_module "$CRATE_NAME" "$PARENT_MODULE/$SUBNAME" "$CRATE_NAME-$SUBNAME"
                
                # Go one more level deep (e.g., handlers/check_deps/instructions/)
                for NESTED in "$SUBDIR"/*/; do
                    if [[ -d "$NESTED" ]]; then
                        local NESTED_NAME="$(basename "$NESTED")"
                        if [[ -f "$NESTED/mod.rs" ]]; then
                            output_module "$CRATE_NAME" "$PARENT_MODULE/$SUBNAME/$NESTED_NAME" "$CRATE_NAME-$NESTED_NAME"
                        fi
                    fi
                done
            fi
        fi
    done
}

# Main discovery
if [[ -n "$SPECIFIC_CRATE" ]]; then
    CRATE_DIR="$CRATES_DIR/$SPECIFIC_CRATE"
    if [[ -d "$CRATE_DIR" ]]; then
        discover_crate_modules "$CRATE_DIR"
    else
        echo "Error: Crate '$SPECIFIC_CRATE' not found in $CRATES_DIR" >&2
        exit 1
    fi
else
    for CRATE_DIR in "$CRATES_DIR"/gglib-*/; do
        if [[ -d "$CRATE_DIR" ]]; then
            discover_crate_modules "$CRATE_DIR"
        fi
    done
fi
