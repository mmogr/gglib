#!/bin/bash
# check_boundaries.sh - Validate workspace crate dependency boundaries
#
# This script enforces the Phase 5 architecture rules:
# - gglib-core: Pure domain types, no adapter/infra deps
# - gglib-db: Core + sqlx only, no adapter deps
# - Adapters (cli, axum, tauri): Core + db + their local deps only
#
# Usage: ./scripts/check_boundaries.sh [--verbose]
# Output: boundary-status.json with pass/fail per crate
#
# Exit codes:
#   0 - All boundaries pass
#   1 - One or more boundaries violated

set -euo pipefail

VERBOSE=${1:-""}
OUTPUT_FILE="boundary-status.json"
FAILED=0

# Colors for terminal output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m' # No Color

# JSON results array
declare -a RESULTS=()

log() {
    echo -e "$1"
}

log_verbose() {
    if [[ "$VERBOSE" == "--verbose" ]]; then
        echo -e "$1"
    fi
}

check_crate_deps() {
    local crate=$1
    shift
    local forbidden=("$@")
    
    log_verbose "${YELLOW}Checking $crate...${NC}"
    
    # Get direct dependencies (depth 1)
    local deps
    deps=$(cargo tree -p "$crate" --depth 1 --prefix none 2>/dev/null | tail -n +2 | awk '{print $1}')
    
    local violations=()
    for dep in $deps; do
        for forbidden_dep in "${forbidden[@]}"; do
            if [[ "$dep" == "$forbidden_dep" ]]; then
                violations+=("$dep")
            fi
        done
    done
    
    if [[ ${#violations[@]} -gt 0 ]]; then
        log "${RED}FAIL${NC}: $crate"
        log "  Forbidden dependencies found: ${violations[*]}"
        local violations_json=$(printf '"%s",' "${violations[@]}" | sed 's/,$//')
        RESULTS+=("{\"crate\": \"$crate\", \"status\": \"fail\", \"violations\": [$violations_json]}")
        return 1
    else
        log "${GREEN}PASS${NC}: $crate"
        RESULTS+=("{\"crate\": \"$crate\", \"status\": \"pass\", \"violations\": []}")
        return 0
    fi
}

main() {
    log "🔍 Checking workspace crate boundaries..."
    log ""
    
    # Adapter/infra dependencies that should NOT appear in core
    ADAPTER_DEPS=(axum tower tower-http clap tauri sqlx hyper)
    
    # gglib-core: Pure domain, no adapter or infra deps
    log "📦 gglib-core (pure domain - no adapters, no sqlx)"
    if ! check_crate_deps "gglib-core" "${ADAPTER_DEPS[@]}"; then
        FAILED=1
    fi
    log ""
    
    # gglib-db: Core + sqlx only, no web/cli/gui adapters
    DB_FORBIDDEN=(axum tower tower-http clap tauri hyper)
    log "📦 gglib-db (core + sqlx - no adapters)"
    if ! check_crate_deps "gglib-db" "${DB_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    # gglib-cli: Should not have web/gui deps
    CLI_FORBIDDEN=(axum tower tower-http tauri hyper)
    log "📦 gglib-cli (core + db + clap - no web/gui)"
    if ! check_crate_deps "gglib-cli" "${CLI_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    # gglib-axum: Should not have cli/gui deps
    AXUM_FORBIDDEN=(clap tauri)
    log "📦 gglib-axum (core + db + axum - no cli/gui)"
    if ! check_crate_deps "gglib-axum" "${AXUM_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    # gglib-tauri: Should not have cli/web deps (once tauri is added)
    TAURI_FORBIDDEN=(clap axum tower-http hyper)
    log "📦 gglib-tauri (core + db + tauri - no cli/web)"
    if ! check_crate_deps "gglib-tauri" "${TAURI_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    # Domain/service layer crates - no UI adapters
    DOMAIN_FORBIDDEN=(axum tower tower-http clap tauri hyper sqlx)
    
    log "📦 gglib-download (domain/adapter - no UI adapters)"
    if ! check_crate_deps "gglib-download" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    log "📦 gglib-gguf (parser - no adapters)"
    if ! check_crate_deps "gglib-gguf" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    log "📦 gglib-gui (service facade - no UI adapters)"
    if ! check_crate_deps "gglib-gui" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    log "📦 gglib-hf (HTTP adapter - no UI adapters)"
    if ! check_crate_deps "gglib-hf" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    log "📦 gglib-mcp (domain service - no adapters)"
    if ! check_crate_deps "gglib-mcp" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    log "📦 gglib-runtime (process runner - no UI adapters)"
    if ! check_crate_deps "gglib-runtime" "${DOMAIN_FORBIDDEN[@]}"; then
        FAILED=1
    fi
    log ""
    
    # Build JSON output
    local timestamp=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
    local overall_status="pass"
    if [[ $FAILED -eq 1 ]]; then
        overall_status="fail"
    fi
    
    # Join results array
    local results_json=$(IFS=,; echo "${RESULTS[*]}")
    
    cat > "$OUTPUT_FILE" << EOF
{
  "timestamp": "$timestamp",
  "overall": "$overall_status",
  "crates": [
    $results_json
  ]
}
EOF
    
    # Summary
    if [[ $FAILED -eq 0 ]]; then
        log "✅ ${GREEN}All boundary checks passed${NC}"
        log ""
        log "Results written to: $OUTPUT_FILE"
        exit 0
    else
        log "❌ ${RED}Boundary violations detected${NC}"
        log ""
        log "Results written to: $OUTPUT_FILE"
        exit 1
    fi
}

main
