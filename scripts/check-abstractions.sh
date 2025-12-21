#!/usr/bin/env bash
#
# CI gate: Check for leaky database abstractions
#
# This script fails if it finds:
# 1. setup_database() calls outside of approved entry points
# 2. Raw SQL (sqlx::query) outside of src/services/database/
#
# Run this in CI to prevent architectural regression.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== Checking for leaky database abstractions ==="
echo ""

ERRORS=0

# =============================================================================
# Check 1: setup_database() should only be called from approved entry points
# =============================================================================

echo "Checking for unauthorized setup_database() calls..."

# Approved locations (one per line):
# - src/main.rs (CLI entry point)
# - src/services/gui_backend.rs (GUI entry point)
# - tests/ (test files are allowed)
# - doc comments (/// or //!) don't count

# Find all setup_database().await calls in .rs files under src/
SETUP_DB_CALLS=$(grep -rn "setup_database().await" src/ --include="*.rs" 2>/dev/null || true)

# Filter to only actual code (not doc comments)
ACTUAL_CALLS=$(echo "$SETUP_DB_CALLS" | grep -v "^[^:]*:[0-9]*:[ ]*//[/!]" || true)

# Check each call
while IFS= read -r line; do
    [ -z "$line" ] && continue
    
    file=$(echo "$line" | cut -d: -f1)
    
    # Approved files
    case "$file" in
        src/main.rs)
            echo -e "  ${GREEN}✓${NC} $file (CLI entry point)"
            ;;
        src/services/gui_backend.rs)
            echo -e "  ${GREEN}✓${NC} $file (GUI entry point)"
            ;;
        *)
            echo -e "  ${RED}✗${NC} UNAUTHORIZED: $line"
            ERRORS=$((ERRORS + 1))
            ;;
    esac
done <<< "$ACTUAL_CALLS"

echo ""

# =============================================================================
# Check 2: Raw SQL (sqlx::query) should only be in approved database modules
# =============================================================================

echo "Checking for raw SQL outside database layer..."

# Find sqlx::query calls
RAW_SQL_CALLS=$(grep -rn "sqlx::query" src/ --include="*.rs" 2>/dev/null || true)

# Filter out the allowed directory and doc comments
while IFS= read -r line; do
    [ -z "$line" ] && continue
    
    file=$(echo "$line" | cut -d: -f1)
    
    # Skip doc comments
    if echo "$line" | grep -q "^[^:]*:[0-9]*:[ ]*//[/!]"; then
        continue
    fi
    
    # Check if in allowed locations:
    # - src/services/database/* (main database layer)
    # - src/services/mcp/database.rs (MCP server database)
    # - src/services/chat_history.rs (chat history database)
    # - src/services/settings.rs (settings database)
    case "$file" in
        src/services/database/*)
            echo -e "  ${GREEN}✓${NC} $file (database layer)"
            ;;
        src/services/mcp/database.rs)
            echo -e "  ${GREEN}✓${NC} $file (MCP database module)"
            ;;
        src/services/chat_history.rs)
            echo -e "  ${GREEN}✓${NC} $file (chat history module)"
            ;;
        src/services/settings.rs)
            echo -e "  ${GREEN}✓${NC} $file (settings module)"
            ;;
        *)
            echo -e "  ${RED}✗${NC} RAW SQL LEAK: $line"
            ERRORS=$((ERRORS + 1))
            ;;
    esac
done <<< "$RAW_SQL_CALLS"

echo ""

# =============================================================================
# Check 3: SqlitePool should not appear in command handlers
# =============================================================================

echo "Checking for SqlitePool in command handlers..."

# Command handlers should use Arc<AppCore>, not SqlitePool directly
POOL_IN_COMMANDS=$(grep -rn "SqlitePool" src/commands/ --include="*.rs" 2>/dev/null || true)

while IFS= read -r line; do
    [ -z "$line" ] && continue
    
    # Skip doc comments and use statements (imports are fine for type annotations)
    if echo "$line" | grep -qE "^[^:]*:[0-9]*:[ ]*(//|use )"; then
        continue
    fi
    
    # Skip if it's in a type context that's just for compatibility
    if echo "$line" | grep -qE "//.*SqlitePool"; then
        continue
    fi
    
    echo -e "  ${YELLOW}⚠${NC} REVIEW: $line"
done <<< "$POOL_IN_COMMANDS"

echo ""

# =============================================================================
# Check 4: GgufParser impl should only be used in bootstrap files
# =============================================================================

echo "Checking for GgufParser outside of bootstrap..."

# GgufParser (the impl) should only be imported/used in bootstrap.rs or main.rs
# Handlers should use the GgufParserPort trait via injected context
GGUF_IMPL_ALLOWLIST='bootstrap\.rs:|main\.rs:'
GGUF_IMPL_HITS=$(rg "gglib_gguf::GgufParser" crates/gglib-{cli,tauri,axum}/src -S 2>/dev/null || true)

if [[ -n "$GGUF_IMPL_HITS" ]]; then
    # Check if any hits are outside the allowlist
    VIOLATIONS=$(echo "$GGUF_IMPL_HITS" | grep -vE "$GGUF_IMPL_ALLOWLIST" || true)
    if [[ -n "$VIOLATIONS" ]]; then
        echo -e "  ${RED}✗${NC} PARSER IMPL LEAK: GgufParser used outside bootstrap/main"
        echo "$VIOLATIONS" | while IFS= read -r line; do
            echo -e "      $line"
        done
        ERRORS=$((ERRORS + 1))
    else
        echo -e "  ${GREEN}✓${NC} GgufParser only used in bootstrap files"
    fi
else
    echo -e "  ${GREEN}✓${NC} No GgufParser imports found in adapter crates"
fi

echo ""

# =============================================================================
# Summary
# =============================================================================

if [ $ERRORS -gt 0 ]; then
    echo -e "${RED}=== FAILED: Found $ERRORS abstraction leak(s) ===${NC}"
    echo ""
    echo "To fix:"
    echo "  1. Use Arc<AppCore> instead of calling setup_database() directly"
    echo "  2. Use ModelService/AppCore methods instead of raw sqlx::query"
    echo "  3. See src/services/core/README.md for the proper architecture"
    exit 1
else
    echo -e "${GREEN}=== PASSED: No abstraction leaks found ===${NC}"
    exit 0
fi
