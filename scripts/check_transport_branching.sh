#!/bin/bash
# check_transport_branching.sh
# 
# Enforcement gate for frontend transport unification.
# Ensures platform-specific code (isTauriApp) never appears in client modules.
#
# Usage: ./scripts/check_transport_branching.sh
#
# Exit codes:
#   0 - All checks pass
#   1 - Platform branching found in forbidden locations

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

echo "🔍 Checking for platform branching violations..."
echo ""

# Track if any violations found
VIOLATIONS=0

# ============================================================================
# Rule 1: No isTauriApp in src/services/clients/
# ============================================================================
echo "📋 Rule 1: No platform branching in client modules"

if [ -d "$PROJECT_ROOT/src/services/clients" ]; then
    CLIENTS_MATCHES=$(grep -rn "isTauriApp" "$PROJECT_ROOT/src/services/clients" 2>/dev/null || true)
    
    if [ -n "$CLIENTS_MATCHES" ]; then
        echo -e "${RED}❌ VIOLATION: isTauriApp found in src/services/clients/${NC}"
        echo "$CLIENTS_MATCHES"
        VIOLATIONS=$((VIOLATIONS + 1))
    else
        echo -e "${GREEN}✓ No platform branching in client modules${NC}"
    fi
else
    echo -e "${YELLOW}⚠ src/services/clients/ does not exist yet${NC}"
fi

echo ""

# ============================================================================
# Rule 2: No direct transport imports in clients (except getTransport)
# ============================================================================
echo "📋 Rule 2: Clients import only getTransport, not transport implementations"

if [ -d "$PROJECT_ROOT/src/services/clients" ]; then
    # Look for imports of TauriTransport or HttpTransport directly
    DIRECT_IMPORTS=$(grep -rn "import.*from.*transport/tauri\|import.*from.*transport/http\|TauriTransport\|HttpTransport" "$PROJECT_ROOT/src/services/clients" 2>/dev/null || true)
    
    if [ -n "$DIRECT_IMPORTS" ]; then
        echo -e "${RED}❌ VIOLATION: Direct transport implementation imports found${NC}"
        echo "$DIRECT_IMPORTS"
        VIOLATIONS=$((VIOLATIONS + 1))
    else
        echo -e "${GREEN}✓ No direct transport imports in client modules${NC}"
    fi
else
    echo -e "${YELLOW}⚠ src/services/clients/ does not exist yet${NC}"
fi

echo ""

# ============================================================================
# Rule 3: All isTauriApp usages must have TRANSPORT_EXCEPTION comment (warning only)
# ============================================================================
echo "📋 Rule 3: Remaining isTauriApp usages should be documented exceptions"

# Find all files with isTauriApp
ALL_TAURI_FILES=$(grep -rl "isTauriApp" "$PROJECT_ROOT/src" --include="*.ts" --include="*.tsx" 2>/dev/null | grep -v "node_modules" | grep -v "transport/" || true)

UNDOCUMENTED=0
if [ -n "$ALL_TAURI_FILES" ]; then
    echo "Files with isTauriApp:"
    for file in $ALL_TAURI_FILES; do
        # Check if file has TRANSPORT_EXCEPTION comment
        if grep -q "TRANSPORT_EXCEPTION:" "$file" 2>/dev/null; then
            echo -e "  ${GREEN}✓ $(basename "$file") (documented exception)${NC}"
        else
            echo -e "  ${YELLOW}⚠ $(basename "$file") (no TRANSPORT_EXCEPTION comment)${NC}"
            UNDOCUMENTED=$((UNDOCUMENTED + 1))
        fi
    done
else
    echo -e "${GREEN}✓ No isTauriApp usages found outside transport layer${NC}"
fi

echo ""

# ============================================================================
# Summary
# ============================================================================
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ $VIOLATIONS -gt 0 ]; then
    echo -e "${RED}❌ FAILED: $VIOLATIONS violation(s) found${NC}"
    exit 1
elif [ $UNDOCUMENTED -gt 0 ]; then
    echo -e "${YELLOW}⚠ PASSED with warnings: $UNDOCUMENTED undocumented exception(s)${NC}"
    echo "  Consider adding TRANSPORT_EXCEPTION: comments to explain platform-specific code"
    exit 0
else
    echo -e "${GREEN}✓ PASSED: All transport branching rules satisfied${NC}"
    exit 0
fi
