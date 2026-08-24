#!/usr/bin/env bash
# Fail if a `sqlx` query's Result is discarded.
#
# `crates/gglib-db/src/setup.rs` carried six of these:
#
#     let _ = sqlx::query(r#"ALTER TABLE models ADD COLUMN template_caps TEXT"#)
#         .execute(pool)
#         .await;
#     // Ignore error if column already exists
#
# The comment names one error. The code discards every error: `duplicate
# column name`, `no such table`, `database is locked`, `disk I/O error` and
# `database or disk is full` all land in `_` and read as success. `Result` is
# `#[must_use]`, so `let _ =` is the one construct that switches off the only
# warning the compiler would otherwise give.
#
# This has already shipped a bug. #796: the `benchmark_runs.applied_json`
# ALTER sat above the CREATE that makes the table, so on a fresh database it
# failed with `no such table`, the error went into `_`, and the CREATE that
# followed carried no such column. Every fresh install could not store an
# apply record until a second boot re-ran the migration — a class of failure
# that is invisible by construction, because the swallow is what removes the
# evidence.
#
# The database is the one v1 surface that lives on the user's disk. Tolerating
# a specific error is fine, and this repo already has the pattern for it:
# `setup.rs::is_unique_violation` absorbs exactly one code and propagates the
# rest. What is banned is tolerating all of them by writing none of them down.
#
# ── What counts as a discard ─────────────────────────────────────────────────
#
#   A. `let _ = …` bound to a statement that runs a query. The exact shape
#      above, whether on one line or spread over four.
#   B. `.ok();` terminating an awaited query — the same discard wearing a
#      different hat.
#
# A statement "runs a query" if it names a `sqlx::query*` builder or one of the
# executors (`.execute(`, `.fetch_one(`, `.fetch_all(`, `.fetch_optional(`).
# Whole-line comments are not code and are skipped — `setup.rs` documents the
# old pattern in prose, and quoting a mistake is how the next person learns it
# was one.
#
# Form A is cleared by `?`, `.unwrap()` or `.expect(` in the same statement,
# because then the binding is discarding the *row* and not the error. That is
# what `let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM models") … .await
# .unwrap();` does in setup.rs's own tests: the query is run for its failure
# mode, and a failure panics.
#
# ── Known limits, stated rather than implied ─────────────────────────────────
#
# `row.try_get("col").ok()` in `repositories/row_mappers.rs` is not flagged and
# is not a bug: reading an absent column as `None` is how rows written before a
# migration keep loading, and that decision is made per column, with a type
# that says so. This guard is about statement-level discards of a query's
# outcome, not about every `Result` sqlx can produce.
#
# A discard split across more than four lines, or laundered through a variable
# (`let q = sqlx::query(…); let _ = q.execute(p).await;`), evades it. Nothing
# writes either shape, and a grep that tried to cover them would cost more in
# false positives than the cases are worth.
#
# ── Two things that make it trustworthy ──────────────────────────────────────
#
# It reports how many query statements it scanned and fails if that is zero:
# "nothing to check" and "everything is fine" are otherwise the same output,
# which is how check_param_source_exhaustive.sh spent its first life passing CI
# while detecting literally nothing. And it self-tests against known-bad and
# known-good fixtures before scanning anything real, so a detector that has
# stopped detecting fails loudly instead of passing quietly.
#
# Usage: ./scripts/check_swallowed_db_errors.sh
#
# Exit codes:
#   0 - every sqlx query's Result is propagated or deliberately matched
#   1 - a discarded query Result was found, or the guard could not verify itself

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Emits `V:<line>:<text>` per violation and a final `Q:<count>` of the query
# statements it scanned.
read -r -d '' DETECTOR <<'AWK' || true
function is_comment(s) { return s ~ /^[[:space:]]*(\/\/|\/\*|\*)/ }
function runs_query(s) {
  return s ~ /sqlx::(query|raw_sql)/ \
      || s ~ /\.(execute|fetch_one|fetch_all|fetch_optional)\(/
}
# `?`, `.unwrap()` and `.expect(` all handle the error; a `let _` alongside one
# of them is discarding the row, which is a different and legitimate thing.
function handles_error(s) {
  return s ~ /\?[[:space:]]*;?[[:space:]]*$/ || s ~ /\.unwrap\(\)/ || s ~ /\.expect\(/
}
{ line[NR] = $0 }
END {
  scanned = 0
  for (i = 1; i <= NR; i++) {
    if (is_comment(line[i])) continue

    # Liveness: every non-comment line that names a query builder.
    if (line[i] ~ /sqlx::(query|raw_sql)/) scanned++

    # A. The binding form. Read this line and the next three, stopping at the
    # first statement terminator, and ask whether a query is in there.
    if (line[i] ~ /^[[:space:]]*let[[:space:]]+_[[:space:]]*(:[^=]*)?=/) {
      hit = 0; handled = 0
      for (j = i; j <= NR && j <= i + 3; j++) {
        if (is_comment(line[j])) continue
        if (runs_query(line[j])) hit = 1
        if (handles_error(line[j])) handled = 1
        if (line[j] ~ /;[[:space:]]*$/) break
      }
      if (hit && !handled) { printf "V:%d:%s\n", i, line[i]; continue }
    }

    # B. The terminator form. `.ok();` closing an awaited query — read back
    # over the three lines it can plausibly be chained from.
    if (line[i] ~ /\.ok\(\)[[:space:]]*;[[:space:]]*$/) {
      hit = 0; awaited = 0
      for (j = i; j >= 1 && j >= i - 3; j--) {
        if (is_comment(line[j])) continue
        if (runs_query(line[j])) hit = 1
        if (line[j] ~ /\.await/) awaited = 1
        if (j < i && line[j] ~ /;[[:space:]]*$/) break
      }
      if (hit && awaited) { printf "V:%d:%s\n", i, line[i] }
    }
  }
  printf "Q:%d\n", scanned
}
AWK

# Run the detector over one file. Prints violations as `<file>:<line>:<text>`
# and echoes the scanned-statement count on its own line.
detect() {
  local file="$1" out
  out="$(awk "$DETECTOR" "$file")"
  printf '%s\n' "$out" | while IFS= read -r rec; do
    case "$rec" in
      V:*) printf '%s:%s\n' "$file" "${rec#V:}" ;;
    esac
  done
  printf '%s\n' "$out" | sed -n 's/^Q:\([0-9]*\)$/\1/p'
}

# ── Self-test: prove the detector can still fail ─────────────────────────────

selftest() {
  local dir bad_hits good_hits
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN

  # Four discards: one-line binding, multi-line binding, one-line `.ok();`,
  # multi-line `.ok();`.
  cat >"$dir/bad.rs" <<'RS'
async fn migrate(pool: &SqlitePool) -> Result<()> {
    let _ = sqlx::query("ALTER TABLE models ADD COLUMN a TEXT").execute(pool).await;

    let _ = sqlx::query("ALTER TABLE models ADD COLUMN b TEXT")
        .execute(pool)
        .await;

    sqlx::query("ALTER TABLE models ADD COLUMN c TEXT").execute(pool).await.ok();

    sqlx::query("ALTER TABLE models ADD COLUMN d TEXT")
        .execute(pool)
        .await
        .ok();

    Ok(())
}
RS

  cat >"$dir/good.rs" <<'RS'
async fn migrate(pool: &SqlitePool) -> Result<()> {
    // let _ = sqlx::query("this one is prose, not code").execute(pool).await;
    sqlx::query("ALTER TABLE models ADD COLUMN a TEXT")
        .execute(pool)
        .await?;

    // Exactly one error absorbed, by name; every other one propagates.
    match sqlx::query("UPDATE models SET model_key = ?")
        .bind(&key)
        .execute(pool)
        .await
    {
        Ok(_) => {}
        Err(e) if is_unique_violation(&e) => skipped += 1,
        Err(e) => return Err(e.into()),
    }

    // Not a query result: a repository call whose row is only probed for
    // existence, and which propagates anyway.
    let _ = self.get_by_id(id).await?;

    // The row is discarded here, not the error — the query is run for its
    // failure mode and `.unwrap()` panics the test if it has one.
    let _: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM models")
        .fetch_one(&pool)
        .await
        .unwrap();

    // Per-column tolerance in a row mapper, which is a different decision.
    let origin = row.try_get::<Option<String>, _>("defaults_origin").ok().flatten();
    let json = metadata.and_then(|s| serde_json::to_string(&s).ok());
    dotenvy::dotenv().ok();

    Ok(())
}
RS

  bad_hits="$(detect "$dir/bad.rs" | grep -c ':[0-9][0-9]*:' || true)"
  if [ "$bad_hits" -ne 4 ]; then
    echo -e "${RED}❌ self-test: the detector caught ${bad_hits} of 4 known discards${NC}"
    detect "$dir/bad.rs" | sed 's/^/     /'
    return 1
  fi

  good_hits="$(detect "$dir/good.rs" | grep -c ':[0-9][0-9]*:' || true)"
  if [ "$good_hits" -ne 0 ]; then
    echo -e "${RED}❌ self-test: the detector flags code that handles its errors${NC}"
    detect "$dir/good.rs" | sed 's/^/     /'
    return 1
  fi

  echo -e "${GREEN}✓${NC} self-test: four known discards caught, propagating and deliberately-matched code cleared"
  return 0
}

if ! selftest; then
  echo
  echo "The guard cannot verify its own detection, so its verdict on real code"
  echo "means nothing. Fix the detector before trusting this script again."
  exit 1
fi

# ── The real scan ────────────────────────────────────────────────────────────

violations=0
scanned_total=0

# Every file that touches sqlx, not only gglib-db's — the crate boundary rule
# keeps sqlx inside gglib-db, and this is what notices if that ever stops being
# true.
mapfile -t candidates < <(
  grep -rl 'sqlx' --include='*.rs' crates/ src-tauri/ tests/ 2>/dev/null | sort
)

for file in "${candidates[@]:-}"; do
  [ -n "$file" ] || continue

  offenders=""
  count=0
  while IFS= read -r rec; do
    case "$rec" in
      ''|*[!0-9]*) offenders="${offenders}${rec}"$'\n' ;;
      *) count="$rec" ;;
    esac
  done < <(detect "$file")

  scanned_total=$((scanned_total + count))

  offenders="$(printf '%s' "$offenders" | sed '/^$/d')"
  if [ -n "$offenders" ]; then
    echo -e "${RED}❌ $file${NC}"
    printf '%s\n' "$offenders" | sed 's/^/     /'
    violations=$((violations + 1))
  fi
done

if [ "$violations" -gt 0 ]; then
  cat <<'MSG'

A sqlx query's Result is discarded.

`let _ =` and `.ok();` do not tolerate the one error you had in mind — they
tolerate a locked database, a full disk and a missing table on the same terms.
#796 shipped that way: an ALTER failed with `no such table`, said nothing, and
left every fresh install without the column.

Propagate it with `?`, or absorb exactly one error by name the way
`setup.rs::is_unique_violation` does. If the point was "do this only if it is
not already done", ask the database its shape first — `add_column_if_missing`
in setup.rs reads `PRAGMA table_info` and then runs the ALTER with `?`.
MSG
  exit 1
fi

# Liveness. A guard that checks nothing prints exactly what a guard that found
# nothing wrong prints, which is how the first version of
# check_param_source_exhaustive.sh passed CI while scanning zero matches.
if [ "$scanned_total" -eq 0 ]; then
  echo -e "${RED}❌ no sqlx queries were found to check${NC}"
  echo
  echo "Either the database layer stopped using sqlx's query builders — in which"
  echo "case retarget this script — or the detector no longer recognises them, in"
  echo "which case its success means nothing. It found none, and there were over"
  echo "two hundred the last time anyone looked."
  exit 1
fi

echo -e "${GREEN}✅ every sqlx query's Result is handled${NC} (${scanned_total} scanned in ${#candidates[@]} files)"
