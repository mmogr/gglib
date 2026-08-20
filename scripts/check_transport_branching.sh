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
#   1 - Any of: platform branching in a client module (Rule 1); a client
#       importing a transport domain API (Rule 2); the Rule 2 self-test failing;
#       or zero client modules scanned

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
# Rule 2: Clients reach the transport only through its low-level primitive
# ============================================================================
#
# What this rule is FOR. `src/services/clients/` holds the modules that own real
# request logic of their own — hand-parsed streaming, or a server that is not the
# app's own backend. They are allowed the low-level primitive (`transport/api/client`,
# which supplies the base URL and auth headers) and they are allowed types. They
# must not reach into a *domain* API module, because that is the layer whose job
# they are deliberately doing themselves; an import from there means the client
# should not have existed.
#
# The previous version of this rule grepped for `transport/tauri`,
# `transport/http`, `TauriTransport` and `HttpTransport`. None of the four appears
# anywhere in the tree, so the rule could not fail. It also did not describe the
# directory it guarded: `clients/benchmark.ts` imports `transport/api/client`
# directly, and neither client module imports `getTransport`.
#
# So the invariant is restated as the one that is actually true and actually worth
# holding, and it self-tests against known-bad and known-good fixtures before
# scanning anything real — a rule that has never been watched to fail is not a rule.
echo "📋 Rule 2: Clients import only the transport's low-level client, not domain APIs"

# A transport import is permitted from a client module iff it names either
# `transport/api/client` (the primitive) or something under `transport/types/`
# (declarations only — no runtime behaviour to bypass).
transport_import_violations() {
    # $1 = directory to scan
    # Filter on the extracted specifier, not the whole line: an unrelated mention
    # of an allowed path anywhere on a line must not suppress a real violation.
    grep -rnE "(from|import|require)[[:space:]]*\(?[[:space:]]*['\"\`][^'\"\`]*transport(/|['\"\`])" "$1" \
        --include='*.ts' --include='*.tsx' 2>/dev/null \
    | while IFS= read -r hit; do
        spec=$(printf '%s' "$hit" | sed -E "s/.*(from|import|require)[[:space:]]*\(?[[:space:]]*['\"\`]([^'\"\`]+)['\"\`].*/\2/")
        case "$spec" in
            */transport/api/client) continue ;;
            */transport/types|*/transport/types/*) continue ;;
        esac
        printf '%s\n' "$hit"
    done
}

# --- self-test: the detector must catch a domain import and clear the allowed ones
SELFTEST_DIR="$(mktemp -d)"
trap 'rm -rf "$SELFTEST_DIR"' EXIT
cat > "$SELFTEST_DIR/known_bad.ts" <<'EOF'
import { listModels } from '../transport/api/models/local';
import { getTransport } from '../transport';
const { streamSse } = await import('../transport/api/sse');
const { get } = await import(`../transport/api/models/local`);
EOF
cat > "$SELFTEST_DIR/known_good.ts" <<'EOF'
import { get, getAuthenticatedFetchConfig } from '../transport/api/client';
import type { DashboardSnapshot } from '../transport/types/dashboard';
import type { Transport } from '../transport/types';
import { appLogger } from '../platform';
EOF
SELFTEST_BAD=$(transport_import_violations "$SELFTEST_DIR" | grep -c "known_bad" || true)
SELFTEST_GOOD=$(transport_import_violations "$SELFTEST_DIR" | grep -c "known_good" || true)
rm -rf "$SELFTEST_DIR"; trap - EXIT

if [ "$SELFTEST_BAD" -ne 4 ]; then
    echo -e "${RED}❌ self-test: the detector missed a domain-API import (caught $SELFTEST_BAD/4)${NC}"
    VIOLATIONS=$((VIOLATIONS + 1))
elif [ "$SELFTEST_GOOD" -ne 0 ]; then
    echo -e "${RED}❌ self-test: the detector flagged an allowed import (flagged $SELFTEST_GOOD)${NC}"
    VIOLATIONS=$((VIOLATIONS + 1))
else
    echo -e "  ${GREEN}✓${NC} self-test: domain imports caught, primitive and type imports cleared"
fi

if [ -d "$PROJECT_ROOT/src/services/clients" ]; then
    CLIENT_FILES=$(find "$PROJECT_ROOT/src/services/clients" -name '*.ts' -o -name '*.tsx' 2>/dev/null | wc -l | tr -d ' ')

    if [ "$CLIENT_FILES" -eq 0 ]; then
        # Liveness guard. An empty scan is indistinguishable from a clean one, and
        # that is precisely how the previous version of this rule stayed green.
        echo -e "${RED}❌ no client modules were scanned — the rule cannot have passed${NC}"
        VIOLATIONS=$((VIOLATIONS + 1))
    else
        DIRECT_IMPORTS=$(transport_import_violations "$PROJECT_ROOT/src/services/clients")

        if [ -n "$DIRECT_IMPORTS" ]; then
            echo -e "${RED}❌ VIOLATION: client module imports a transport domain API${NC}"
            echo "$DIRECT_IMPORTS"
            echo "  Allowed: transport/api/client (the primitive) and transport/types/*."
            echo "  A client needing a domain API should not be a client — move it out."
            VIOLATIONS=$((VIOLATIONS + 1))
        else
            echo -e "${GREEN}✓ No domain-API imports in client modules${NC} ($CLIENT_FILES scanned)"
        fi
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
