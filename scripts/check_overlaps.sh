#!/usr/bin/env bash
# Fail when contracts/upstream/overlaps.json no longer describes gglib.
#
# overlaps.json names each gglib feature an engine could come to do itself,
# one row per feature: the engine (`llama` or `sd`), the ADR that decided it
# or null, the gglib paths that hold it, what of upstream's structure to
# watch for it (paths, README headings, code identifiers, phrases), and the
# recorder's readings that bear on it. scripts/engine_pin_overlaps.sh reads
# the rows against a bump; this check keeps the rows true:
#   - each row's `engine` is `llama` or `sd`, its `adr` is null or names a
#     file docs/adr/NNNN-*.md, each of its gglib `paths` exists, and
#     `upstream.paths`, `upstream.headings`, `upstream.ids`,
#     `upstream.phrases` and `readings` are lists of strings;
#   - every Rust file whose header says it is Tier A compensation
#     (`Tier A — Compensation`, ADR 0001) is under some row's paths, so a
#     stage that compensates for the engine cannot be added without a row
#     that watches for the engine doing it.
# It reads the committed files only. `make enforce` runs it; a self-test runs
# first, planting one of each fault, and stops unless each is reported.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TABLE=contracts/upstream/overlaps.json

command -v jq >/dev/null || { echo "check_overlaps.sh needs jq" >&2; exit 2; }

# check <root> <table>: one ::error line per problem; returns 1 if any.
check() {
  local root=$1 table=$2 failures=0 problems

  if ! jq -e 'type == "object" and length > 0' "$root/$table" >/dev/null 2>&1; then
    echo "::error title=overlaps.json is not valid::$table must be one JSON object with a row per feature."
    return 1
  fi

  # Shape, engine and lists, from jq alone.
  problems=$(jq -r --arg t "$table" '
    def strings: type == "array" and all(.[]; type == "string");
    to_entries[] | .key as $id | .value
    | ( select((.engine == "llama" or .engine == "sd") | not)
        | "::error title=overlaps.json row is malformed::\($id): engine must be llama or sd, not \(.engine | tojson)." ),
      ( select((.adr == null or (.adr | type == "string" and test("^[0-9]{4}$"))) | not)
        | "::error title=overlaps.json row is malformed::\($id): adr must be null or four digits, not \(.adr | tojson)." ),
      ( select((.paths | strings and length > 0) | not)
        | "::error title=overlaps.json row is malformed::\($id): paths must be a non-empty list of gglib paths." ),
      ( select((.upstream | type == "object" and (.paths | strings) and (.headings | strings) and (.ids | strings) and (.phrases | strings)) | not)
        | "::error title=overlaps.json row is malformed::\($id): upstream must hold the lists paths, headings, ids and phrases." ),
      ( select((.readings | strings) | not)
        | "::error title=overlaps.json row is malformed::\($id): readings must be a list of reading names.")
  ' "$root/$table") || return 1
  if [ -n "$problems" ]; then
    printf '%s\n' "$problems"
    failures=$((failures + $(grep -c . <<<"$problems")))
  fi

  # Each ADR and each gglib path exists.
  while IFS=$'\t' read -r id adr; do
    [ -n "$id" ] || continue
    if ! compgen -G "$root/docs/adr/$adr-*.md" >/dev/null; then
      echo "::error title=overlaps.json names a missing ADR::$id names ADR $adr, and docs/adr/ has no $adr-*.md."
      failures=$((failures + 1))
    fi
  done < <(jq -r 'to_entries[] | select(.value.adr != null) | "\(.key)\t\(.value.adr)"' "$root/$table")
  while IFS=$'\t' read -r id path; do
    [ -n "$id" ] || continue
    if [ ! -e "$root/$path" ]; then
      echo "::error title=overlaps.json names a missing path::$id names $path, which is not in the tree."
      failures=$((failures + 1))
    fi
  done < <(jq -r 'to_entries[] | .key as $id | .value.paths[]? | select(type == "string") | "\($id)\t\(.)"' "$root/$table")

  # Every Tier A compensation module is under some row's paths.
  local covered file hit
  covered=$(jq -r '[.[].paths[]?] | .[]' "$root/$table")
  while IFS= read -r file; do
    [ -n "$file" ] || continue
    hit=0
    while IFS= read -r path; do
      [ -n "$path" ] || continue
      if [ "$file" = "$path" ] || [[ $file == "$path"/* ]]; then hit=1; break; fi
    done <<<"$covered"
    if [ "$hit" -eq 0 ]; then
      echo "::error title=a compensation stage has no overlap row::$file says it is Tier A compensation, and no row of $table holds it. Add it to a row's paths, so a bump that brings the engine's own version is read against it."
      failures=$((failures + 1))
    fi
  done < <(cd "$root" && grep -rl --include='*.rs' 'Tier A — Compensation' crates 2>/dev/null | sort)

  if [ "$failures" -gt 0 ]; then
    echo "$failures problem(s) in $table."
    return 1
  fi
}

# ── Self-test: one of each fault, each of which must be reported ────────────

selftest() {
  local dir out status=0
  dir=$(mktemp -d)
  mkdir -p "$dir/contracts/upstream" "$dir/docs/adr" "$dir/crates/a/src" "$dir/crates/b/src"
  touch "$dir/docs/adr/0001-one.md" "$dir/crates/a/src/lib.rs"
  printf '//! Stage.\n//!\n//! **Tier A — Compensation** ([ADR 0001]).\n' >"$dir/crates/a/src/stage.rs"
  printf '//! Stage.\n//!\n//! **Tier A — Compensation** ([ADR 0001]).\n' >"$dir/crates/b/src/orphan.rs"
  jq -n '
    def up: {paths: [], headings: [], ids: [], phrases: []};
    { good: {engine: "llama", adr: "0001", paths: ["crates/a"], upstream: up, readings: []},
      engine: {engine: "cuda", adr: null, paths: ["crates/a"], upstream: up, readings: []},
      adr: {engine: "sd", adr: "0099", paths: ["crates/a"], upstream: up, readings: []},
      path: {engine: "llama", adr: null, paths: ["crates/nowhere"], upstream: up, readings: []},
      lists: {engine: "llama", adr: null, paths: ["crates/a"], upstream: {paths: "x"}, readings: "y"} }' \
    >"$dir/contracts/upstream/overlaps.json"
  out=$(check "$dir" contracts/upstream/overlaps.json) && status=1
  for expected in \
    "overlaps.json row is malformed::engine: engine must be" \
    "overlaps.json names a missing ADR::adr names ADR 0099" \
    "overlaps.json names a missing path::path names crates/nowhere" \
    "overlaps.json row is malformed::lists: upstream must hold" \
    "overlaps.json row is malformed::lists: readings must be" \
    "a compensation stage has no overlap row::crates/b/src/orphan.rs"; do
    if ! grep -qF "::error title=$expected" <<<"$out"; then
      echo "❌ self-test: no \"$expected\" for its planted fault"
      status=1
    fi
  done
  if grep -q "crates/a/src/stage.rs" <<<"$out"; then
    echo "❌ self-test: a covered compensation stage was reported"
    status=1
  fi
  if [ "$(grep -c '^::error' <<<"$out")" -ne 6 ]; then
    echo "❌ self-test: expected 6 errors for 6 planted faults:"
    printf '%s\n' "$out" | sed 's/^/     /'
    status=1
  fi
  echo '[]' >"$dir/contracts/upstream/overlaps.json"
  if check "$dir" contracts/upstream/overlaps.json >/dev/null; then
    echo "❌ self-test: a table that is not an object passed"
    status=1
  fi
  rm -rf "$dir"
  return "$status"
}

selftest || { echo "check_overlaps.sh: the self-test failed, so a clean run would prove nothing." >&2; exit 1; }

check "$ROOT_DIR" "$TABLE" || exit 1

rows=$(jq 'length' "$ROOT_DIR/$TABLE")
# No Tier A stage left is a pass: grep finding nothing must not end the script.
stages=$({ grep -rl --include='*.rs' 'Tier A — Compensation' "$ROOT_DIR/crates" || true; } | wc -l | tr -d ' ')
echo "Self-test passed: each of 7 planted faults was reported."
echo "All $rows rows of $TABLE name an engine, an ADR that exists or none, and paths that exist; all $stages Tier A compensation stages are under a row."
