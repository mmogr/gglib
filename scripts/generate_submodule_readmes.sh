#!/bin/bash
# generate_submodule_readmes.sh — Manage README files across all crate src/ subdirs
#
# Default mode (no flags):
#   Updates existing README files — fills in minimal ones, appends module-table
#   to those that lack it. Does NOT create new files.
#
# --create mode:
#   Creates README stubs for every src/ subdir (Rust/TypeScript/tests) that
#   currently lacks one. Extracts //! doc comments from mod.rs verbatim into
#   the module-docs section, prepends #![doc = include_str!("README.md")] to
#   mod.rs, and leaves the original //! block with a migration comment.
#   Exits after creating stubs; does NOT run the existing-README scan.
#
# Usage:
#   ./scripts/generate_submodule_readmes.sh              # update existing
#   ./scripts/generate_submodule_readmes.sh --create     # create missing
#   ./scripts/generate_submodule_readmes.sh --dry-run    # preview updates
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

if $DRY_RUN && $CREATE; then
    echo "=== CREATE MODE (DRY RUN) ==="
elif $CREATE; then
    echo "=== CREATE MODE ==="
elif $DRY_RUN; then
    echo "=== DRY RUN MODE ==="
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

# Function to list files/directories in a module
list_module_entries() {
    local dir="$1"
    local entries=()
    
    # List .rs files (excluding mod.rs and lib.rs)
    for f in "$dir"/*.rs; do
        [[ -f "$f" ]] || continue
        local name=$(basename "$f")
        [[ "$name" == "mod.rs" || "$name" == "lib.rs" ]] && continue
        entries+=("$name")
    done
    
    # List subdirectories that have code
    for d in "$dir"/*/; do
        [[ -d "$d" ]] || continue
        local name=$(basename "$d")
        [[ "$name" == "target" ]] && continue
        # Check if directory has .rs files
        if ls "$d"/*.rs &>/dev/null; then
            entries+=("$name/")
        fi
    done
    
    printf '%s\n' "${entries[@]}" | sort
}

# Function to generate badge row for a file or directory
generate_badge_row() {
    local entry="$1"
    local badge_prefix="$2"
    local crate_name="$3"
    local is_dir=false
    
    if [[ "$entry" == */ ]]; then
        is_dir=true
        entry="${entry%/}"
    fi
    
    # Remove .rs extension for badge name
    local badge_name="${entry%.rs}"
    local full_prefix="${badge_prefix}-${badge_name}"
    
    # For directories, use simplified naming (crate-dirname) matching CI convention
    if $is_dir; then
        full_prefix="${crate_name}-${badge_name}"
    fi
    
    local link_text="$entry"
    # Strip .rs extension from link target for rustdoc compatibility
    local link_target="${entry%.rs}"
    if $is_dir; then
        link_text="$entry/"
        link_target="$entry/"
    fi
    
    # 4 columns: LOC, Complexity, Coverage (no Tests - matches generate_module_tables.sh)
    cat << EOF
| [\`${link_text}\`](${link_target}) | ![](https://img.shields.io/endpoint?url=${BADGE_BASE}/${full_prefix}-loc.json) | ![](https://img.shields.io/endpoint?url=${BADGE_BASE}/${full_prefix}-complexity.json) | ![](https://img.shields.io/endpoint?url=${BADGE_BASE}/${full_prefix}-coverage.json) |
EOF
}

# Function to generate full README content for empty/minimal READMEs
generate_readme_content() {
    local dir="$1"
    local module_name=$(get_module_name "$dir")
    local badge_prefix=$(get_badge_prefix "$dir")
    local crate_name=$(get_crate_name "$dir")
    
    # Header with module-docs markers (no top-level badges for submodules)
    cat << EOF
# ${module_name}

<!-- module-docs:start -->

Module documentation pending.

<!-- module-docs:end -->

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
EOF

    # Generate rows for each entry
    while IFS= read -r entry; do
        [[ -n "$entry" ]] && generate_badge_row "$entry" "$badge_prefix" "$crate_name"
    done < <(list_module_entries "$dir")
    
    cat << EOF
<!-- module-table:end -->

</details>
EOF
}

# Function to generate just the module table section
generate_table_section() {
    local dir="$1"
    local badge_prefix=$(get_badge_prefix "$dir")
    local crate_name=$(get_crate_name "$dir")
    
    cat << EOF

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
EOF
    
    while IFS= read -r entry; do
        [[ -n "$entry" ]] && generate_badge_row "$entry" "$badge_prefix" "$crate_name"
    done < <(list_module_entries "$dir")
    
    cat << EOF
<!-- module-table:end -->

</details>
EOF
}

# Check if README is empty or minimal (less than 50 bytes or just a title)
is_readme_minimal() {
    local readme="$1"
    local size=$(wc -c < "$readme" | tr -d ' ')
    
    # Less than 50 bytes is definitely minimal
    if [[ $size -lt 50 ]]; then
        return 0
    fi
    
    # Check if it's just a title line
    local line_count=$(wc -l < "$readme" | tr -d ' ')
    if [[ $line_count -le 2 ]]; then
        return 0
    fi
    
    return 1
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
    local crate_name
    crate_name=$(get_crate_name "$dir")
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

<details>
<summary><h2>Modules</h2></summary>

<!-- module-table:start -->
| Module | LOC | Complexity | Coverage |
|--------|-----|------------|----------|
EOF
    while IFS= read -r entry; do
        [[ -n "$entry" ]] && generate_badge_row "$entry" "$badge_prefix" "$crate_name"
    done < <(list_module_entries "$dir")
cat << EOF
<!-- module-table:end -->

</details>
EOF
}

# Generate a README stub for a TypeScript src/ subdir.
# Includes LOC/Complexity badges and module-docs markers; no module-table
# (TypeScript does not have the same module hierarchy as Rust).
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
    echo "Next steps:"
    echo "  Run: ./scripts/generate_module_tables.sh  (populate badge tables)"
    echo "  Run: ./scripts/check_readmes.sh           (verify coverage)"
}

# ── Dispatch ───────────────────────────────────────────────────────────────
if $CREATE; then
    create_missing_readmes
    exit 0
fi

# Main logic
echo "Scanning for existing READMEs to update..."
echo ""

UPDATED=0
FILLED=0
SKIPPED=0

# Find all existing README.md files in crates
while IFS= read -r readme; do
    dir=$(dirname "$readme")
    rel_path=$(echo "$readme" | sed "s|$CRATES_DIR/||")
    
    # Skip crate root READMEs (handled separately, they have different structure)
    if [[ "$dir" == */crates/gglib-* && ! "$dir" == */src* ]]; then
        continue
    fi
    
    # Skip the top-level crates/README.md
    if [[ "$readme" == "$CRATES_DIR/README.md" ]]; then
        continue
    fi
    
    # Check if README already has module-table
    if grep -q "module-table:start" "$readme"; then
        ((SKIPPED++))
        continue
    fi
    
    # Check if README is empty/minimal
    if is_readme_minimal "$readme"; then
        echo "Filling: $rel_path (empty/minimal)"
        if ! $DRY_RUN; then
            generate_readme_content "$dir" > "$readme"
        fi
        ((FILLED++))
    else
        # Has content but no module-table - append one
        echo "Updating: $rel_path (adding module table)"
        if ! $DRY_RUN; then
            generate_table_section "$dir" >> "$readme"
        fi
        ((UPDATED++))
    fi
done < <(find "$CRATES_DIR" -name "README.md" -type f | sort)

echo ""
echo "Summary:"
echo "  Filled (empty/minimal): $FILLED"
echo "  Updated (added table):  $UPDATED"
echo "  Skipped (already done): $SKIPPED"
echo ""
echo "Done!"
