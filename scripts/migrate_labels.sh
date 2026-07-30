#!/usr/bin/env bash
# migrate_labels.sh - Consolidate legacy/duplicate labels into the component:/priority:/
# size:/type: taxonomy, then remove the legacy ones.
#
# Uses `env bash`, not `/bin/bash` like most scripts here: this needs associative arrays
# (bash 4+), and macOS ships /bin/bash pinned at 3.2 for licensing reasons. Under 3.2,
# `declare -A` silently falls back to a regular indexed array, and unquoted bareword
# subscripts like [bug] get arithmetic-evaluated as a variable reference — which fails
# outright under `set -u`. Don't "fix" this back to /bin/bash to match convention.
#
# Migrate-then-delete, never blind-delete: every issue/PR carrying a legacy label gets the
# canonical replacement applied FIRST. Deleting a label strips it from every historical
# item that carries it, so nothing is silently lost — the semantic information just moves
# to the canonical label before the old one goes away.
#
# Legacy labels NOT touched by this script (kept deliberately):
#   component: gui, component: voice  - heavily used (54, 44 uses); only their PATH
#     mapping in label-check.yml was stale, not the labels themselves.
#   ux, tech-debt, cleanup            - orthogonal descriptors, not duplicates of
#     anything in the component:/priority:/size:/type: taxonomy.
#   epic:orchestrator, phase:a..phase:m - the only surviving record of how #490-#510
#     were ordered. Sub-issues supersede this pattern going forward; existing history
#     isn't worth erasing to enforce that retroactively.
#
# Usage:
#   ./scripts/migrate_labels.sh --dry-run   (default if no flag given — prints the plan)
#   ./scripts/migrate_labels.sh --execute   (actually applies labels, then deletes legacy ones)
#
# Requires: gh (authenticated with repo write access for --execute)

set -euo pipefail

REPO="mmogr/gglib"
MODE="${1:---dry-run}"

if [[ "$MODE" != "--dry-run" && "$MODE" != "--execute" ]]; then
  echo "Usage: $0 [--dry-run|--execute]" >&2
  exit 1
fi

# legacy_label -> canonical_label. One canonical label per legacy label; a canonical
# label may appear on the right of multiple rows (e.g. many things collapse to
# type: bug), but each legacy label maps to exactly one replacement.
declare -A REPLACEMENT=(
  ["bug"]="type: bug"
  ["enhancement"]="type: feature"
  ["epic"]="type: epic"
  ["type:epic"]="type: epic"
  ["refactor"]="type: refactor"
  ["regression"]="bug: regression"
  ["frontend"]="component: frontend"
  ["area:frontend"]="component: frontend"
  ["area:db"]="component: db"
  ["area:proxy"]="component: proxy"
)

# These have no single canonical replacement — report items missing a component: label
# rather than guessing one. area:backend/area:surfaces are historically applied broadly
# and don't map 1:1 onto a single component:.
declare -a VERIFY_ONLY=("area:backend" "area:surfaces")

# No replacement needed — deleting outright is correct.
#   fixed  - redundant with the issue/PR's own closed state.
#   proxy  - 0 uses per the last audit; safe to delete with nothing to migrate.
declare -a DELETE_OUTRIGHT=("fixed" "proxy")

echo "Mode: $MODE"
echo ""

fetch_items_with_label() {
  local label="$1"
  gh api -X GET search/issues -f q="repo:$REPO label:\"$label\"" --paginate \
    --jq '.items[] | "\(.number)\t\(.pull_request != null)"' 2>/dev/null
}

total_migrated=0
total_verify_flagged=0

for legacy in "${!REPLACEMENT[@]}"; do
  canonical="${REPLACEMENT[$legacy]}"
  items="$(fetch_items_with_label "$legacy")"
  count=$(echo "$items" | grep -c . || true)

  if [[ "$count" -eq 0 ]]; then
    echo "[$legacy -> $canonical] no items carry this label, skipping"
    continue
  fi

  echo "[$legacy -> $canonical] $count item(s)"
  while IFS=$'\t' read -r number is_pr; do
    [[ -z "$number" ]] && continue
    kind="issue"; [[ "$is_pr" == "true" ]] && kind="pr"
    if [[ "$MODE" == "--dry-run" ]]; then
      echo "  would add '$canonical' to $kind #$number"
    else
      gh api -X POST "repos/$REPO/issues/$number/labels" -f "labels[]=$canonical" >/dev/null
      echo "  added '$canonical' to $kind #$number"
    fi
    total_migrated=$((total_migrated + 1))
  done <<< "$items"
done

echo ""
for legacy in "${VERIFY_ONLY[@]}"; do
  items="$(fetch_items_with_label "$legacy")"
  count=$(echo "$items" | grep -c . || true)
  [[ "$count" -eq 0 ]] && { echo "[$legacy] no items, skipping"; continue; }

  echo "[$legacy] $count item(s) — verifying each already has a component: label"
  while IFS=$'\t' read -r number is_pr; do
    [[ -z "$number" ]] && continue
    kind="issue"; [[ "$is_pr" == "true" ]] && kind="pr"
    has_component=$(gh api "repos/$REPO/issues/$number/labels" --jq \
      '[.[] | select(.name | startswith("component:"))] | length')
    if [[ "$has_component" -eq 0 ]]; then
      echo "  NEEDS MANUAL REVIEW: $kind #$number has '$legacy' but no component: label — not auto-migrating, pick one by hand"
      total_verify_flagged=$((total_verify_flagged + 1))
    else
      echo "  ok: $kind #$number already has a component: label, '$legacy' is safe to delete"
    fi
  done <<< "$items"
done

echo ""
echo "=== Summary ==="
echo "Labels migrated (legacy label added canonical replacement): $total_migrated application(s)"
echo "Items needing manual component: review before deletion: $total_verify_flagged"
echo ""

if [[ "$MODE" == "--dry-run" ]]; then
  echo "This was a dry run — no labels were added or deleted."
  echo "Re-run with --execute to apply, IF total_verify_flagged above is 0 (or you've"
  echo "manually resolved those items first)."
  exit 0
fi

if [[ "$total_verify_flagged" -gt 0 ]]; then
  echo "Refusing to delete area:backend/area:surfaces while $total_verify_flagged item(s)" >&2
  echo "still lack a component: label — resolve those manually, then re-run." >&2
  exit 1
fi

echo "Deleting legacy labels (this removes them from every item, now that replacements are in place)..."
for legacy in "${!REPLACEMENT[@]}" "${VERIFY_ONLY[@]}" "${DELETE_OUTRIGHT[@]}"; do
  if gh label delete "$legacy" --repo "$REPO" --yes 2>/dev/null; then
    echo "  deleted: $legacy"
  else
    echo "  (already gone or delete failed: $legacy)"
  fi
done

echo ""
echo "Done."
