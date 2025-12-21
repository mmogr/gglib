#!/bin/bash
# Update existing README files with badge tables (does NOT create new READMEs)
# Usage: ./scripts/generate_submodule_readmes.sh [--dry-run]
#
# This script:
# 1. Finds all existing README.md files in crates/
# 2. If a README is empty or minimal, fills in the template
# 3. If a README exists but lacks module-table, appends one
# 4. Never creates new README files - user controls where READMEs exist

set -e

CRATES_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/crates"
BADGE_BASE="https://raw.githubusercontent.com/mmogr/gglib/badges"
DRY_RUN=false

if [[ "$1" == "--dry-run" ]]; then
    DRY_RUN=true
    echo "=== DRY RUN MODE ==="
fi

# Function to get crate name from path
get_crate_name() {
    local dir="$1"
    echo "$dir" | sed -E 's|.*/crates/(gglib-[^/]+)/.*|\1|'
}

# Function to compute badge prefix from directory path
# e.g., crates/gglib-core/src/domain -> gglib-core-domain
get_badge_prefix() {
    local dir="$1"
    local crate_name=$(get_crate_name "$dir")
    
    # Get path relative to src/
    local rel_path=$(echo "$dir" | sed -E "s|.*/crates/$crate_name/src/||")
    
    # If we're at the crate root src/, rel_path will equal the full path
    if [[ "$rel_path" == "$dir" ]]; then
        echo "$crate_name"
        return
    fi
    
    # Replace / with - and construct prefix
    local module_path=$(echo "$rel_path" | tr '/' '-')
    
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
