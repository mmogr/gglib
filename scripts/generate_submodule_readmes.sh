#!/bin/bash
# generate_submodule_readmes.sh — Create missing README stubs
#
# --create:
#   Creates README stubs for every src/ subdir (Rust/TypeScript) that
#   currently lacks one, except src/types/generated/ and below (ts-rs output,
#   which check_readmes.sh also skips). Extracts //! doc comments from mod.rs
#   verbatim into the module-docs section, prepends
#   #![doc = include_str!("README.md")] to mod.rs, and leaves the original //!
#   block with a migration comment.
#   A README that already exists is never touched.
#
# Usage:
#   ./scripts/generate_submodule_readmes.sh --create     # create missing
#   ./scripts/generate_submodule_readmes.sh --create --dry-run

set -e

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CRATES_DIR="$ROOT_DIR/crates"
TS_SRC_DIR="$ROOT_DIR/src"
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

# Whether --create would prepend the include line to this mod.rs: it exists
# and does not carry the line yet. The dry run asks the same question, so it
# reports only the edits a real run would make.
modrs_needs_include() {
    [[ -f "$1" ]] && ! grep -q '#!\[doc = include_str!("README.md")]' "$1" 2>/dev/null
}

# ── Stub generators ────────────────────────────────────────────────────────

# Generate a full README stub for a new Rust crate src/ subdir.
# If mod.rs contains //! doc comments they are extracted verbatim into the
# module-docs section; otherwise a TODO placeholder is used.
generate_rust_stub() {
    local dir="$1"
    local module_name
    module_name=$(basename "$dir")
    local modrs="$dir/mod.rs"

    # Extract //! doc content for the module-docs section
    local doc_content=""
    if [[ -f "$modrs" ]] && grep -q '^//!' "$modrs" 2>/dev/null; then
        doc_content=$(grep '^//!' "$modrs" | sed -E 's|^//! ?||')
    fi

cat << EOF
# ${module_name}

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
generate_ts_stub() {
    local dir="$1"
    local module_name
    module_name=$(basename "$dir")

cat << EOF
# ${module_name}

<!-- module-docs:start -->

TODO: Describe the purpose and responsibilities of this module.

<!-- module-docs:end -->
EOF
}

# ── CREATE mode: generate stubs for directories missing READMEs ────────────
create_missing_readmes() {
    local CREATED_RUST=0
    local CREATED_TS=0
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
                if modrs_needs_include "$modrs"; then
                    echo "  [update] $rel/mod.rs"
                fi
                continue
            fi

            echo "  Creating: $rel/README.md"
            generate_rust_stub "$dir" > "$readme"
            (( CREATED_RUST++ )) || true

            if modrs_needs_include "$modrs"; then
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
        # `types/generated/` is ts-rs output, which check_readmes.sh skips for
        # the reason given there; a stub here would be a README it ignores.
        done < <(find "$TS_SRC_DIR" -mindepth 1 -type d | grep -v "node_modules" | grep -v "types/generated" | sort)
    else
        echo "  (src/ not found — skipping)"
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
        local total=$(( CREATED_RUST + CREATED_TS ))
        echo "  Total READMEs created:        $total"
    fi
    echo ""
    echo "Next step:"
    echo "  Run: ./scripts/check_readmes.sh           (verify coverage)"
}

create_missing_readmes
