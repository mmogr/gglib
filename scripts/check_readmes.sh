#!/bin/bash
# check_readmes.sh — Validate README coverage and quality across the workspace
#
# Checks performed:
#   [always]  Rust crate src/ subdirs have README.md
#   [always]  Rust subdir READMEs have <!-- module-docs:start/end --> markers
#   [always]  Rust subdir READMEs have <!-- module-table:start/end --> markers
#   [always]  Crate-level READMEs have ## Architecture + ## Internal Structure headings
#   [always]  TypeScript src/ subdirs have README.md with <!-- module-docs --> markers
#   [always]  tests/ and all its subdirs have README.md
#   [--strict] module-docs block does not contain "TODO:" placeholder text
#   [--strict] mod.rs with a sibling README.md uses #![doc = include_str!("README.md")]
#
# Usage: ./scripts/check_readmes.sh [--strict] [--json] [--verbose]
#
# Options:
#   --strict   Also fail on TODO: placeholders and missing include_str! parity.
#              Called with --strict by default from check_boundaries.sh in CI.
#   --json     Write readme-status.txt (NDJSON: one JSON entry per line) so
#              check_boundaries.sh can merge the results into boundary-status.json.
#   --verbose  Print each passing check in addition to failures.
#
# Failure fix hints (printed on detection):
#   Missing README.md:  ./scripts/generate_submodule_readmes.sh --create
#   Stale tables:       ./scripts/generate_module_tables.sh
#
# Exit codes:
#   0  All README checks pass
#   1  One or more violations found

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
CRATES_DIR="$ROOT_DIR/crates"
TS_SRC_DIR="$ROOT_DIR/src"
TESTS_DIR="$ROOT_DIR/tests"
JSON_OUTPUT_FILE="readme-status.txt"

# ─── Flags ────────────────────────────────────────────────────────────────────
STRICT=false
JSON_MODE=false
VERBOSE=false

for arg in "$@"; do
    case "$arg" in
        --strict)  STRICT=true  ;;
        --json)    JSON_MODE=true ;;
        --verbose) VERBOSE=true  ;;
    esac
done

# ─── Colors ───────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
NC='\033[0m'

# ─── Counters ─────────────────────────────────────────────────────────────────
HARD_FAIL=0
STRICT_FAIL=0
declare -a RESULTS=()

log()   { echo -e "$1"; }
log_v() { $VERBOSE && echo -e "$1" || true; }

# Escape backslashes and double-quotes for safe embedding in a JSON string value.
json_escape() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    printf '%s' "$s"
}

# Append one boundary-status-compatible entry to the RESULTS array.
# Usage: add_result <key> <status> [escaped-violation ...]
add_result() {
    local key="$1" status="$2"
    shift 2
    local v_json=""
    for v in "$@"; do
        v_json+="\"$v\","
    done
    v_json="${v_json%,}"
    if [[ -n "$v_json" ]]; then
        RESULTS+=("{\"crate\": \"$key\", \"status\": \"$status\", \"violations\": [$v_json]}")
    else
        RESULTS+=("{\"crate\": \"$key\", \"status\": \"$status\", \"violations\": []}")
    fi
}

# Returns 0 (true) when the <!-- module-docs --> block in FILE contains "TODO:".
readme_has_todo() {
    awk '/<!-- module-docs:start -->/,/<!-- module-docs:end -->/' "$1" | \
        grep -q "TODO:" 2>/dev/null
}

# ─────────────────────────────────────────────────────────────────────────────
# CHECK 1: Rust crate src/ subdir READMEs
# Covers: crates/gglib-*/src/**/ and src-tauri/src/**/
# ─────────────────────────────────────────────────────────────────────────────
check_rust_subdir_readmes() {
    log "${CYAN}📂 Rust crate subdir READMEs${NC}"

    if [[ ! -d "$CRATES_DIR" ]]; then
        log "  ${YELLOW}SKIP${NC}: crates/ directory not found"
        log ""
        add_result "readme-rust-subdirs" "pass"
        $STRICT && add_result "readme-cargo-docs-parity" "pass" || true
        return
    fi

    local -a violations=()
    local -a parity_violations=()
    local any_fail=0

    # Collect all Rust src/ roots: crates/*/src + src-tauri/src (if present)
    local -a src_roots=()
    while IFS= read -r d; do
        src_roots+=("$d")
    done < <(find "$CRATES_DIR" -maxdepth 2 -name "src" -type d | sort)
    [[ -d "$ROOT_DIR/src-tauri/src" ]] && src_roots+=("$ROOT_DIR/src-tauri/src")

    for src_root in "${src_roots[@]}"; do
        while IFS= read -r dir; do
            local rel="${dir#"$ROOT_DIR"/}"
            local readme="$dir/README.md"
            local modrs="$dir/mod.rs"
            local dir_ok=1

            # Hard check: README must exist
            if [[ ! -f "$readme" ]]; then
                log "  ${RED}MISSING${NC}    $rel/"
                violations+=("$(json_escape "$rel/: README.md missing")")
                (( HARD_FAIL++ )) || true
                any_fail=1
                continue
            fi

            # Hard check: <!-- module-docs:start/end --> markers must be present
            if ! grep -q "module-docs:start" "$readme"; then
                log "  ${RED}INCOMPLETE${NC} $rel/  — no <!-- module-docs --> markers"
                violations+=("$(json_escape "$rel/: missing <!-- module-docs:start --> markers")")
                (( HARD_FAIL++ )) || true
                dir_ok=0
                any_fail=1
            fi

            # Hard check: <!-- module-table:start/end --> markers must be present
            if ! grep -q "module-table:start" "$readme"; then
                log "  ${RED}INCOMPLETE${NC} $rel/  — no <!-- module-table --> markers"
                violations+=("$(json_escape "$rel/: missing <!-- module-table:start --> markers")")
                (( HARD_FAIL++ )) || true
                dir_ok=0
                any_fail=1
            fi

            if $STRICT; then
                # Strict check: module-docs block must not contain "TODO:" placeholder
                if readme_has_todo "$readme"; then
                    log "  ${YELLOW}TODO${NC}       $rel/  — 'TODO:' placeholder not yet replaced"
                    violations+=("$(json_escape "$rel/: TODO: placeholder not replaced in module-docs")")
                    (( STRICT_FAIL++ )) || true
                    dir_ok=0
                    any_fail=1
                fi

                # Strict check: mod.rs with sibling README.md must use inner doc attribute
                # for Cargo docs parity with GitHub browsing. Using inner attribute syntax
                # (#![doc = ...]) is required — the outer form (#[doc = ...]) belongs on
                # the parent module declaration, not in the file itself.
                if [[ -f "$modrs" ]] && ! grep -qF '#![doc = include_str!("README.md")]' "$modrs"; then
                    log "  ${YELLOW}PARITY${NC}     $rel/  — mod.rs missing #![doc = include_str!(\"README.md\")]"
                    parity_violations+=("$(json_escape "$rel/: mod.rs missing #![doc = include_str!(\"README.md\")]")")
                    (( STRICT_FAIL++ )) || true
                fi
            fi

            [[ $dir_ok -eq 1 ]] && log_v "  ${GREEN}OK${NC}         $rel/"
        done < <(find "$src_root" -mindepth 1 -type d | sort)
    done

    if [[ $any_fail -eq 0 ]]; then
        log "  ${GREEN}PASS${NC}: all Rust subdir READMEs present and complete"
    else
        log ""
        log "  → Fix missing: ./scripts/generate_submodule_readmes.sh --create"
    fi
    log ""

    if [[ ${#violations[@]} -gt 0 ]]; then
        add_result "readme-rust-subdirs" "fail" "${violations[@]}"
    else
        add_result "readme-rust-subdirs" "pass"
    fi

    if $STRICT; then
        if [[ ${#parity_violations[@]} -gt 0 ]]; then
            add_result "readme-cargo-docs-parity" "fail" "${parity_violations[@]}"
        else
            add_result "readme-cargo-docs-parity" "pass"
        fi
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# CHECK 2: Crate-level README structural headings
# Every crate-level README must have both ## Architecture and ## Internal Structure.
# ─────────────────────────────────────────────────────────────────────────────
check_rust_crate_readmes() {
    log "${CYAN}📦 Crate-level README structure${NC}"

    local -a violations=()
    local any_fail=0

    # Collect crate-level READMEs: crates/gglib-*/README.md + src-tauri/README.md
    local -a crate_readmes=()
    while IFS= read -r f; do
        crate_readmes+=("$f")
    done < <(find "$CRATES_DIR" -mindepth 2 -maxdepth 2 -name "README.md" | sort)
    [[ -f "$ROOT_DIR/src-tauri/README.md" ]] && crate_readmes+=("$ROOT_DIR/src-tauri/README.md")

    for readme in "${crate_readmes[@]}"; do
        local crate_ok=1
        local crate
        crate=$(basename "$(dirname "$readme")")

        if ! grep -q "^## Architecture" "$readme"; then
            log "  ${RED}FAIL${NC} $crate — missing '## Architecture' heading"
            violations+=("$(json_escape "$crate: missing ## Architecture heading")")
            (( HARD_FAIL++ )) || true
            crate_ok=0
            any_fail=1
        fi

        if ! grep -q "^## Internal Structure" "$readme"; then
            log "  ${RED}FAIL${NC} $crate — missing '## Internal Structure' heading"
            violations+=("$(json_escape "$crate: missing ## Internal Structure heading")")
            (( HARD_FAIL++ )) || true
            crate_ok=0
            any_fail=1
        fi

        [[ $crate_ok -eq 1 ]] && log_v "  ${GREEN}OK${NC}   $crate"
    done

    if [[ $any_fail -eq 0 ]]; then
        log "  ${GREEN}PASS${NC}: all crate-level READMEs have required headings"
    fi
    log ""

    if [[ ${#violations[@]} -gt 0 ]]; then
        add_result "readme-crate-structure" "fail" "${violations[@]}"
    else
        add_result "readme-crate-structure" "pass"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# CHECK 3: TypeScript src/ subdir READMEs
# Checks: existence + <!-- module-docs --> markers (no module-table required for TS).
# ─────────────────────────────────────────────────────────────────────────────
check_ts_subdir_readmes() {
    log "${CYAN}📘 TypeScript src/ subdir READMEs${NC}"

    if [[ ! -d "$TS_SRC_DIR" ]]; then
        log "  ${YELLOW}SKIP${NC}: src/ directory not found at $TS_SRC_DIR"
        log ""
        add_result "readme-ts-subdirs" "pass"
        return
    fi

    local -a violations=()
    local any_fail=0

    while IFS= read -r dir; do
        local rel="${dir#"$ROOT_DIR"/}"
        local readme="$dir/README.md"
        local dir_ok=1

        if [[ ! -f "$readme" ]]; then
            log "  ${RED}MISSING${NC}    $rel/"
            violations+=("$(json_escape "$rel/: README.md missing")")
            (( HARD_FAIL++ )) || true
            any_fail=1
            continue
        fi

        if ! grep -q "module-docs:start" "$readme"; then
            log "  ${RED}INCOMPLETE${NC} $rel/  — no <!-- module-docs --> markers"
            violations+=("$(json_escape "$rel/: missing <!-- module-docs:start --> markers")")
            (( HARD_FAIL++ )) || true
            dir_ok=0
            any_fail=1
        fi

        if $STRICT && readme_has_todo "$readme"; then
            log "  ${YELLOW}TODO${NC}       $rel/  — 'TODO:' placeholder not yet replaced"
            violations+=("$(json_escape "$rel/: TODO: placeholder not replaced in module-docs")")
            (( STRICT_FAIL++ )) || true
            dir_ok=0
            any_fail=1
        fi

        [[ $dir_ok -eq 1 ]] && log_v "  ${GREEN}OK${NC}         $rel/"
    done < <(find "$TS_SRC_DIR" -mindepth 1 -type d | grep -v "node_modules" | sort)

    if [[ $any_fail -eq 0 ]]; then
        log "  ${GREEN}PASS${NC}: all TypeScript src/ subdir READMEs present and complete"
    else
        log ""
        log "  → Fix missing: ./scripts/generate_submodule_readmes.sh --create"
    fi
    log ""

    if [[ ${#violations[@]} -gt 0 ]]; then
        add_result "readme-ts-subdirs" "fail" "${violations[@]}"
    else
        add_result "readme-ts-subdirs" "pass"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# CHECK 4: tests/ subdir READMEs — existence only
# No marker requirements: test directories are documentation, not modules.
# ─────────────────────────────────────────────────────────────────────────────
check_tests_readmes() {
    log "${CYAN}🧪 tests/ subdir READMEs${NC}"

    if [[ ! -d "$TESTS_DIR" ]]; then
        log "  ${YELLOW}SKIP${NC}: tests/ directory not found at $TESTS_DIR"
        log ""
        add_result "readme-tests" "pass"
        return
    fi

    local -a violations=()
    local any_fail=0

    # Check tests/ root itself and all nested subdirs
    local -a check_dirs=("$TESTS_DIR")
    while IFS= read -r d; do
        check_dirs+=("$d")
    done < <(find "$TESTS_DIR" -mindepth 1 -type d | sort)

    for dir in "${check_dirs[@]}"; do
        local rel="${dir#"$ROOT_DIR"/}"
        if [[ ! -f "$dir/README.md" ]]; then
            log "  ${RED}MISSING${NC}    $rel/"
            violations+=("$(json_escape "$rel/: README.md missing")")
            (( HARD_FAIL++ )) || true
            any_fail=1
        else
            log_v "  ${GREEN}OK${NC}         $rel/"
        fi
    done

    if [[ $any_fail -eq 0 ]]; then
        log "  ${GREEN}PASS${NC}: all tests/ subdirs have READMEs"
    else
        log ""
        log "  → Fix missing: ./scripts/generate_submodule_readmes.sh --create"
    fi
    log ""

    if [[ ${#violations[@]} -gt 0 ]]; then
        add_result "readme-tests" "fail" "${violations[@]}"
    else
        add_result "readme-tests" "pass"
    fi
}

# ─────────────────────────────────────────────────────────────────────────────
# main
# ─────────────────────────────────────────────────────────────────────────────
main() {
    log "🔍 Checking README coverage and quality..."
    $STRICT && log "   (strict mode: TODO + include_str! parity checks enabled)" || true
    log ""

    check_rust_subdir_readmes
    check_rust_crate_readmes
    check_ts_subdir_readmes
    check_tests_readmes

    if $JSON_MODE; then
        printf '%s\n' "${RESULTS[@]}" > "$JSON_OUTPUT_FILE"
    fi

    local total_fail=$(( HARD_FAIL + STRICT_FAIL ))
    if [[ $total_fail -eq 0 ]]; then
        log "✅ ${GREEN}All README checks passed${NC}"
        $JSON_MODE && log "   Results written to: $JSON_OUTPUT_FILE" || true
        exit 0
    else
        log "❌ ${RED}README violations detected${NC}"
        [[ $HARD_FAIL   -gt 0 ]] && log "   Hard failures:   $HARD_FAIL"
        [[ $STRICT_FAIL -gt 0 ]] && log "   Strict failures: $STRICT_FAIL"
        $JSON_MODE && log "   Results written to: $JSON_OUTPUT_FILE" || true
        exit 1
    fi
}

main
