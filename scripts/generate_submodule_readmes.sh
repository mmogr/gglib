#!/bin/bash
# generate_submodule_readmes.sh — Create missing README stubs
#
# --create:
#   Creates README stubs for every src/ subdir (Rust/TypeScript/tests) that
#   currently lacks one. Extracts //! doc comments from mod.rs verbatim into
#   the module-docs section, prepends #![doc = include_str!("README.md")] to
#   mod.rs, and leaves the original //! block with a migration comment.
#   A README that already exists is never touched.
#
# Usage:
#   ./scripts/generate_submodule_readmes.sh --create     # create missing
#   ./scripts/generate_submodule_readmes.sh --create --dry-run

set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATES_DIR="$ROOT_DIR/crates"
TS_SRC_DIR="$ROOT_DIR/src"
TESTS_DIR="$ROOT_DIR/tests"
BADGE_BASE="https://raw.githubusercontent.com/mmogr/gglib/badges"
DRY_RUN=false
CREATE=false

for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=true ;;
        --create)  CREATE=true  ;;
    esac
done

if ! $CREATE; then
    echo "Usage: $0 --create [--dry-run]" >&2
    exit 1
fi

if $DRY_RUN; then
    echo "=== CREATE MODE (DRY RUN) ==="
else
    echo "=== CREATE MODE ==="
fi

# Function to get crate name from path
get_crate_name() {
    local dir="$1"
    if [[ "$dir" =~ .*/crates/(gglib-[^/]+)/.* ]]; then
        echo "${BASH_REMATCH[1]}"
    elif [[ "$dir" =~ .*/src-tauri/.* ]]; then
        echo "src-tauri"
    else
        echo "$dir" | sed -E 's|.*/crates/(gglib-[^/]+)/.*|\1|'
    fi
}

# Function to compute badge prefix from directory path
# e.g., crates/gglib-core/src/domain -> gglib-core-domain
# e.g., src-tauri/src/gui_backend  -> src-tauri-gui_backend
get_badge_prefix() {
    local dir="$1"
    local crate_name
    crate_name=$(get_crate_name "$dir")

    # Get path relative to src/ — handles both crates/ and src-tauri/
    local rel_path
    if [[ "$dir" =~ .*/src-tauri/src/(.+) ]]; then
        rel_path="${BASH_REMATCH[1]}"
    else
        rel_path=$(echo "$dir" | sed -E "s|.*/crates/$crate_name/src/||")
    fi

    # If we're at the crate root src/, rel_path will equal the full path
    if [[ "$rel_path" == "$dir" ]]; then
        echo "$crate_name"
        return
    fi

    # Replace / with - and construct prefix
    local module_path
    module_path=$(echo "$rel_path" | tr '/' '-')

    echo "${crate_name}-${module_path}"
}

# Function to get module name (directory name)
get_module_name() {
    basename "$1"
}

# ── TypeScript badge prefix ────────────────────────────────────────────────
# Computes badge slug for a TypeScript src/ subdir.
# e.g., src/services/transport -> ts-services-transport
get_ts_badge_prefix() {
    local dir="$1"
    local rel="${dir#"$TS_SRC_DIR"/}"
    local prefix
    prefix=$(echo "$rel" | tr '/' '-')
    echo "ts-${prefix}"
}

# ── mod.rs migration ───────────────────────────────────────────────────────
# Prepend #![doc = include_str!("README.md")] to the very top of mod.rs.
# Uses the inner attribute form (#![...]) which is required — the outer form
# (#[doc...]) belongs on the parent module declaration, not in the file.
# If //! doc comments are present, inserts a migration note above them so
# developers know to remove the //! block once the README is reviewed.
update_modrs_for_migration() {
    local modrs="$1"
    local tmp
    tmp=$(mktemp)

    # Inner doc attribute at the very top of the file
    printf '#![doc = include_str!("README.md")]\n\n' > "$tmp"

    local migration_inserted=false
    while IFS= read -r line || [[ -n "$line" ]]; do
        if ! $migration_inserted && [[ "$line" =~ ^//! ]]; then
            printf '// MIGRATION: content extracted to README.md — remove this //! block after review\n' >> "$tmp"
            migration_inserted=true
        fi
        printf '%s\n' "$line" >> "$tmp"
    done < "$modrs"

    cp "$tmp" "$modrs"
    rm -f "$tmp"
}

# ── Stub generators ────────────────────────────────────────────────────────

# Generate a full README stub for a new Rust crate src/ subdir.
# If mod.rs contains //! doc comments they are extracted verbatim into the
# module-docs section; otherwise a TODO placeholder is used.
generate_rust_stub() {
    local dir="$1"
    local module_name
    module_name=$(get_module_name "$dir")
    local badge_prefix
    badge_prefix=$(get_badge_prefix "$dir")
    local modrs="$dir/mod.rs"

    # Extract //! doc content for the module-docs section
    local doc_content=""
    if [[ -f "$modrs" ]] && grep -q '^//!' "$modrs" 2>/dev/null; then
        doc_content=$(grep '^//!' "$modrs" | sed -E 's|^//! ?||')
    fi

cat << EOF
# ${module_name}

![LOC](https://img.shields.io/endpoint?url=${BADGE_BASE}/${badge_prefix}-loc.json)
![Complexity](https://img.shields.io/endpoint?url=${BADGE_BASE}/${badge_prefix}-complexity.json)

<!-- module-docs:start -->

EOF
    if [[ -n "$doc_content" ]]; then
        printf '%s\n' "$doc_content"
    else
        printf 'TODO: Describe the purpose and responsibilities of this module.\n'
    fi
cat << EOF

<!-- module-docs:end -->
EOF
}

# Generate a README stub for a TypeScript src/ subdir.
# Includes LOC/Complexity badges and module-docs markers.
generate_ts_stub() {
    local dir="$1"
    local module_name
    module_name=$(basename "$dir")
    local badge_prefix
    badge_prefix=$(get_ts_badge_prefix "$dir")

cat << EOF
# ${module_name}

![LOC](https://img.shields.io/endpoint?url=${BADGE_BASE}/${badge_prefix}-loc.json)
![Complexity](https://img.shields.io/endpoint?url=${BADGE_BASE}/${badge_prefix}-complexity.json)

<!-- module-docs:start -->

TODO: Describe the purpose and responsibilities of this module.

<!-- module-docs:end -->
EOF
}

# Generate a minimal README stub for a tests/ subdir.
# No badges or module markers — test directories are documentation, not modules.
generate_tests_stub() {
    local dir="$1"
    local dir_name
    dir_name=$(basename "$dir")

cat << EOF
# ${dir_name}

TODO: Describe what this test suite covers.
EOF
}

# ── CREATE mode: generate stubs for directories missing READMEs ────────────
create_missing_readmes() {
    local CREATED_RUST=0
    local CREATED_TS=0
    local CREATED_TESTS=0
    local MODRS_UPDATED=0

    # ── Rust crate src/ subdirs (crates/*/src/**/ + src-tauri/src/**/)
    echo "Rust crate src/ subdirs..."

    local -a src_roots=()
    while IFS= read -r d; do
        src_roots+=("$d")
    done < <(find "$CRATES_DIR" -maxdepth 2 -name "src" -type d | sort)
    [[ -d "$ROOT_DIR/src-tauri/src" ]] && src_roots+=("$ROOT_DIR/src-tauri/src")

    for src_root in "${src_roots[@]}"; do
        while IFS= read -r dir; do
            local readme="$dir/README.md"
            [[ -f "$readme" ]] && continue

            local rel="${dir#"$ROOT_DIR"/}"
            local modrs="$dir/mod.rs"

            if $DRY_RUN; then
                echo "  [create] $rel/README.md"
                [[ -f "$modrs" ]] && echo "  [update] $rel/mod.rs"
                continue
            fi

            echo "  Creating: $rel/README.md"
            generate_rust_stub "$dir" > "$readme"
            (( CREATED_RUST++ )) || true

            if [[ -f "$modrs" ]] && ! grep -q '#!\[doc = include_str!("README.md")]' "$modrs" 2>/dev/null; then
                echo "  Updating: $rel/mod.rs"
                update_modrs_for_migration "$modrs"
                (( MODRS_UPDATED++ )) || true
            fi
        done < <(find "$src_root" -mindepth 1 -type d | sort)
    done

    # ── TypeScript src/ subdirs
    echo ""
    echo "TypeScript src/ subdirs..."

    if [[ -d "$TS_SRC_DIR" ]]; then
        while IFS= read -r dir; do
            local readme="$dir/README.md"
            [[ -f "$readme" ]] && continue

            local rel="${dir#"$ROOT_DIR"/}"

            if $DRY_RUN; then
                echo "  [create] $rel/README.md"
                continue
            fi

            echo "  Creating: $rel/README.md"
            generate_ts_stub "$dir" > "$readme"
            (( CREATED_TS++ )) || true
        done < <(find "$TS_SRC_DIR" -mindepth 1 -type d | grep -v "node_modules" | sort)
    else
        echo "  (src/ not found — skipping)"
    fi

    # ── tests/ subdirs (root + all nested)
    echo ""
    echo "tests/ subdirs..."

    if [[ -d "$TESTS_DIR" ]]; then
        local -a test_dirs=("$TESTS_DIR")
        while IFS= read -r d; do
            test_dirs+=("$d")
        done < <(find "$TESTS_DIR" -mindepth 1 -type d | sort)

        for dir in "${test_dirs[@]}"; do
            local readme="$dir/README.md"
            [[ -f "$readme" ]] && continue

            local rel="${dir#"$ROOT_DIR"/}"

            if $DRY_RUN; then
                echo "  [create] $rel/README.md"
                continue
            fi

            echo "  Creating: $rel/README.md"
            generate_tests_stub "$dir" > "$readme"
            (( CREATED_TESTS++ )) || true
        done
    else
        echo "  (tests/ not found — skipping)"
    fi

    # ── Summary
    echo ""
    echo "Summary:"
    if $DRY_RUN; then
        echo "  (dry run — no files written)"
    else
        echo "  Rust subdir READMEs created:  $CREATED_RUST"
        echo "  mod.rs files updated:         $MODRS_UPDATED"
        echo "  TypeScript READMEs created:   $CREATED_TS"
        echo "  tests/ READMEs created:       $CREATED_TESTS"
        local total=$(( CREATED_RUST + CREATED_TS + CREATED_TESTS ))
        echo "  Total READMEs created:        $total"
    fi
    echo ""
    echo "Next step:"
    echo "  Run: ./scripts/check_readmes.sh           (verify coverage)"
}

create_missing_readmes
