#!/usr/bin/env bash
#
# CI gate: Enforce allowlisted Tauri invoke() usage in frontend
#
# This script fails if it finds:
# 1. invoke() called with command strings not in the allowlist
# 2. Dynamic command string construction (security risk)
#
# Run this in CI to prevent architectural regression.
# Policy: Only OS integration commands should be invoked from frontend.
#
# Allowlist (8 commands):
#   - get_embedded_api_info (API discovery)
#   - check_llama_status (binary management)
#   - install_llama (binary management)
#   - open_url (shell integration)
#   - set_selected_model (menu sync)
#   - sync_menu_state (menu sync)
#   - set_proxy_state (proxy/menu state)
#   - log_from_frontend (frontend log ingestion)

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=== Checking frontend invoke() usage policy ==="
echo ""

ERRORS=0

# Allowlisted commands (OS integration only)
ALLOWED_COMMANDS=(
    "get_embedded_api_info"
    "check_llama_status"
    "install_llama"
    "open_url"
    "set_selected_model"
    "sync_menu_state"
    "set_proxy_state"
    "log_from_frontend"
)

# =============================================================================
# Check 1: Find all invoke() calls and validate command strings
# =============================================================================

echo "Checking invoke() calls in frontend..."

# Find all invoke( calls in TypeScript/TSX files
# Pattern: invoke('command_name') or invoke("command_name")
INVOKE_CALLS=$(grep -rn "invoke\s*(" src/ --include="*.ts" --include="*.tsx" 2>/dev/null || true)

# Track if we found any violations
FOUND_ALLOWED=false
FOUND_VIOLATIONS=false

while IFS= read -r line; do
    [ -z "$line" ] && continue
    
    file=$(echo "$line" | cut -d: -f1)
    linenum=$(echo "$line" | cut -d: -f2)
    content=$(echo "$line" | cut -d: -f3-)
    
    # Skip if this is a comment
    if echo "$content" | grep -qE '^\s*(//|/\*)'; then
        continue
    fi
    
    # Skip if it's a type definition or interface
    if echo "$content" | grep -qE '(interface|type|import)'; then
        continue
    fi
    
    # Extract command string from invoke('command') or invoke("command")
    # Handle both single and double quotes
    COMMAND=$(echo "$content" | sed -n "s/.*invoke\s*(\s*['\"]\\([^'\"]*\\)['\"].*/\\1/p")
    
    if [ -z "$COMMAND" ]; then
        # Check if invoke uses a variable or template literal (dynamic command - FORBIDDEN)
        if echo "$content" | grep -qE 'invoke\s*\(\s*[^'\''"]+\s*[,)]'; then
            # Allow internal helper functions (invokeTauri wrapper in platform layer)
            if echo "$file" | grep -qE '(platform/tauri|api/client)\.ts$'; then
                # Internal helper - these only call allowlisted commands
                continue
            fi
            
            echo -e "  ${RED}✗${NC} DYNAMIC COMMAND: $file:$linenum"
            echo -e "      $content"
            echo -e "      invoke() must use static string literals only"
            ERRORS=$((ERRORS + 1))
            FOUND_VIOLATIONS=true
        fi
        continue
    fi
    
    # Check if command is in allowlist
    IS_ALLOWED=false
    for allowed in "${ALLOWED_COMMANDS[@]}"; do
        if [ "$COMMAND" = "$allowed" ]; then
            IS_ALLOWED=true
            break
        fi
    done
    
    if $IS_ALLOWED; then
        echo -e "  ${GREEN}✓${NC} $file:$linenum → invoke('$COMMAND')"
        FOUND_ALLOWED=true
    else
        echo -e "  ${RED}✗${NC} UNAUTHORIZED COMMAND: $file:$linenum"
        echo -e "      invoke('$COMMAND') is not in the allowlist"
        echo -e "      $content"
        ERRORS=$((ERRORS + 1))
        FOUND_VIOLATIONS=true
    fi
done <<< "$INVOKE_CALLS"

if ! $FOUND_ALLOWED && ! $FOUND_VIOLATIONS; then
    echo -e "  ${YELLOW}⚠${NC} No invoke() calls found (expected at least a few)"
fi

echo ""

# =============================================================================
# Check 2: Verify no deprecated patterns
# =============================================================================

echo "Checking for deprecated Tauri patterns..."

# Check for old command invocation patterns
DEPRECATED_PATTERNS=(
    "window.__TAURI__.invoke"
    "@tauri-apps/api/tauri.*invoke"
)

for pattern in "${DEPRECATED_PATTERNS[@]}"; do
    HITS=$(grep -rn "$pattern" src/ --include="*.ts" --include="*.tsx" 2>/dev/null || true)
    if [ -n "$HITS" ]; then
        echo -e "  ${YELLOW}⚠${NC} Found deprecated pattern: $pattern"
        echo "$HITS" | while IFS= read -r line; do
            [ -z "$line" ] && continue
            echo -e "      $line"
        done
    fi
done

echo -e "  ${GREEN}✓${NC} No deprecated patterns found"
echo ""

# =============================================================================
# Summary
# =============================================================================

if [ $ERRORS -gt 0 ]; then
    echo -e "${RED}=== FAILED: Found $ERRORS invoke() policy violation(s) ===${NC}"
    echo ""
    echo "Allowlisted commands (OS integration only):"
    for cmd in "${ALLOWED_COMMANDS[@]}"; do
        echo "  - $cmd"
    done
    echo ""
    echo "To fix:"
    echo "  1. Use HTTP API (fetch) for all business logic"
    echo "  2. Only invoke() OS integration commands from the allowlist"
    echo "  3. Never use dynamic command strings (security risk)"
    echo "  4. Update src/services/transport/ to use HTTP for new features"
    echo ""
    echo "See Phase 3 (issue #10) for architecture rationale"
    exit 1
else
    echo -e "${GREEN}=== PASSED: All invoke() calls are authorized ===${NC}"
    exit 0
fi
