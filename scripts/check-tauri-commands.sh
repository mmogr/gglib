#!/usr/bin/env bash
#
# CI gate: Enforce "HTTP-first, OS-glue-only" Tauri command policy
#
# This script fails if it finds:
# 1. #[tauri::command] outside of {util,llama}.rs
# 2. Extra .rs files in src-tauri/src/commands/ (only {mod,util,llama}.rs allowed)
# 3. invoke_handler! referencing commands outside of {util,llama}
# 4. Deprecated get_gui_api_port anywhere in the codebase
#
# Run this in CI to prevent architectural regression.
# Policy: All product API is HTTP (Axum). Tauri commands are OS integration only.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== Checking Tauri command policy compliance ==="
echo ""

ERRORS=0

# =============================================================================
# Check 1: #[tauri::command] should only exist in util.rs and llama.rs
# =============================================================================

echo "Checking for #[tauri::command] in unauthorized files..."

# Find all #[tauri::command] attributes in src-tauri/
COMMAND_ATTRS=$(grep -rn "#\[tauri::command\]" src-tauri/src/ --include="*.rs" 2>/dev/null || true)

while IFS= read -r line; do
    [ -z "$line" ] && continue
    
    file=$(echo "$line" | cut -d: -f1)
    
    # Approved files: only util.rs and llama.rs
    case "$file" in
        src-tauri/src/commands/util.rs)
            echo -e "  ${GREEN}✓${NC} $file (OS integration: discovery, shell, menu)"
            ;;
        src-tauri/src/commands/llama.rs)
            echo -e "  ${GREEN}✓${NC} $file (OS integration: binary management)"
            ;;
        *)
            echo -e "  ${RED}✗${NC} UNAUTHORIZED COMMAND: $line"
            echo -e "      Commands must be in util.rs or llama.rs only"
            ERRORS=$((ERRORS + 1))
            ;;
    esac
done <<< "$COMMAND_ATTRS"

echo ""

# =============================================================================
# Check 2: commands/ directory should only contain {mod,util,llama}.rs
# =============================================================================

echo "Checking for extra files in commands/ directory..."

COMMANDS_DIR="src-tauri/src/commands"
if [ -d "$COMMANDS_DIR" ]; then
    # List all .rs files in commands/
    ALL_FILES=$(find "$COMMANDS_DIR" -maxdepth 1 -name "*.rs" -type f | sort)
    
    while IFS= read -r file; do
        [ -z "$file" ] && continue
        
        basename=$(basename "$file")
        
        case "$basename" in
            mod.rs|util.rs|llama.rs)
                echo -e "  ${GREEN}✓${NC} $basename (allowed)"
                ;;
            *)
                echo -e "  ${RED}✗${NC} EXTRA FILE: $file"
                echo -e "      Only mod.rs, util.rs, llama.rs are allowed in commands/"
                ERRORS=$((ERRORS + 1))
                ;;
        esac
    done <<< "$ALL_FILES"
else
    echo -e "  ${YELLOW}⚠${NC} commands/ directory not found at $COMMANDS_DIR"
fi

echo ""

# =============================================================================
# Check 3: invoke_handler! should only reference commands::{util,llama}::*
# =============================================================================

echo "Checking invoke_handler! registrations..."

MAIN_RS="src-tauri/src/main.rs"
if [ -f "$MAIN_RS" ]; then
    # Extract the generate_handler! block
    HANDLER_BLOCK=$(sed -n '/generate_handler!\[/,/\]/p' "$MAIN_RS" 2>/dev/null || true)
    
    if [ -n "$HANDLER_BLOCK" ]; then
        # Check each line for command references
        while IFS= read -r line; do
            # Skip empty lines and the macro invocation line
            [ -z "$line" ] && continue
            echo "$line" | grep -q "generate_handler" && continue
            echo "$line" | grep -q "^\s*\]\s*$" && continue
            
            # Check if line contains a command reference
            if echo "$line" | grep -q "commands::"; then
                # Validate it's one of the allowed patterns
                if echo "$line" | grep -qE "commands::(util|llama)::"; then
                    echo -e "  ${GREEN}✓${NC} $(echo "$line" | sed 's/^[[:space:]]*//')"
                else
                    echo -e "  ${RED}✗${NC} INVALID COMMAND REGISTRATION: $(echo "$line" | sed 's/^[[:space:]]*//')"
                    echo -e "      Only commands::util::* and commands::llama::* are allowed"
                    ERRORS=$((ERRORS + 1))
                fi
            fi
        done <<< "$HANDLER_BLOCK"
    else
        echo -e "  ${YELLOW}⚠${NC} Could not find generate_handler! block in $MAIN_RS"
    fi
else
    echo -e "  ${YELLOW}⚠${NC} $MAIN_RS not found"
fi

echo ""

# =============================================================================
# Check 4: Deprecated get_gui_api_port should not exist anywhere
# =============================================================================

echo "Checking for deprecated get_gui_api_port..."

# Search in src-tauri/ and src/ (backend and frontend)
DEPRECATED_HITS=$(grep -rn "get_gui_api_port" src-tauri/ src/ --include="*.rs" --include="*.ts" --include="*.tsx" 2>/dev/null || true)

# Filter out doc comments that might explain the deprecation
ACTUAL_USAGE=$(echo "$DEPRECATED_HITS" | grep -v "//.*deprecated" || true)

if [ -n "$ACTUAL_USAGE" ]; then
    echo -e "  ${RED}✗${NC} DEPRECATED COMMAND FOUND:"
    echo "$ACTUAL_USAGE" | while IFS= read -r line; do
        [ -z "$line" ] && continue
        echo -e "      $line"
    done
    echo -e "      Use get_embedded_api_info instead"
    ERRORS=$((ERRORS + 1))
else
    echo -e "  ${GREEN}✓${NC} No usage of deprecated get_gui_api_port found"
fi

echo ""

# =============================================================================
# Summary
# =============================================================================

if [ $ERRORS -gt 0 ]; then
    echo -e "${RED}=== FAILED: Found $ERRORS policy violation(s) ===${NC}"
    echo ""
    echo "Policy: HTTP-first, OS-glue-only for Tauri commands"
    echo ""
    echo "To fix:"
    echo "  1. Move business logic to HTTP API (gglib-axum crate)"
    echo "  2. Keep only OS integration commands in util.rs and llama.rs"
    echo "  3. Remove any command files besides mod.rs, util.rs, llama.rs"
    echo "  4. Update invoke_handler! to only register util and llama commands"
    echo "  5. Replace get_gui_api_port with get_embedded_api_info"
    echo ""
    echo "See Phase 3 (issue #10) for architecture rationale"
    exit 1
else
    echo -e "${GREEN}=== PASSED: All Tauri command policy checks passed ===${NC}"
    exit 0
fi
