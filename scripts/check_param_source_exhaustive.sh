#!/usr/bin/env bash
# Fail if anything matches on `ParamSource` with a catch-all arm.
#
# `ParamSource` says where a resolved sampling value came from, and several
# decisions read it to mean "did a person choose this?". A wildcard arm makes
# adding a variant a silent behaviour change at every one of those sites
# instead of a compile error:
#
#   - `request_pipeline::sampling` gated the agentic temperature ceiling on a
#     `matches!` listing the *unchosen* variants. A new variant falls through
#     as "chosen", so the ceiling silently stops firing. That is the same
#     shape as #741's floor and #744's ceiling, both of which shipped inert
#     and were found in production rather than by a test.
#   - `is_floor` had the same problem in the other direction — a new variant
#     silently read as "not the floor". It has since been deleted for having
#     no callers, which is the other way that question stops being asked.
#
# The type system cannot enforce this on its own — `matches!` and a `match`
# with `_ =>` are both legal — so this is a grep, in the same spirit as
# check_transport_branching.sh and check-frontend-ipc.sh.
#
# The definition site is exempt: `sampling_provenance.rs` is where the
# exhaustive answer lives, and its own helpers are the thing being protected.
#
# ── How it detects a match, and why not the obvious way ──────────────────────
#
# The first version of this script triggered on `/match[[:space:]].*ParamSource/`
# — the type name on the `match` line. That matched ZERO lines in the whole
# repository, because nobody writes `match some_param_source_value` with the
# type spelled out; they write `match source {`. The only line in the tree
# where `match` and `ParamSource` share a line is inside the exempted
# definition file. The guard was a complete no-op and reported success for it.
#
# So detection keys on the *arm patterns* instead, which is the only signal
# that survives when the scrutinee's type is not textually present:
#
#   1. A block is a `ParamSource` match once a line begins with a
#      `ParamSource::` pattern and carries `=>`. Anchoring at the start of the
#      pattern is what keeps `match won { Some(i) => ParamSource::Layer(i) }`
#      out — that matches on `Option<usize>` and merely *builds* a
#      `ParamSource`, so exhaustiveness over `ParamSource` is meaningless there.
#   2. A catch-all is `_ =>`, `_ if … =>`, or a bare lowercase binding
#      `other =>`, at the *same indentation* as those arms. Same-indent
#      matching is what stops a nested `match` inside an arm — whose own total
#      `None =>` sits deeper — and `Self::Layer(_)`, a wildcard inside a tuple
#      pattern rather than an arm, from tripping it.
#
# Known limit: a call site doing `use ParamSource::*` and matching on bare
# variant names would evade this. Nothing does today.
#
# ── Two things that make it trustworthy ──────────────────────────────────────
#
# It reports how many matches it scanned and fails if that is zero, because
# "nothing to check" and "everything is fine" are the same output otherwise —
# which is exactly how the broken version survived. And it self-tests against
# a known-bad and a known-good fixture before scanning anything real, so a
# guard that has stopped detecting fails loudly rather than passing quietly.
# ci.yml records a prior incident of this same class: three of six CI runs
# were green with a failing test inside.
#
# Usage: ./scripts/check_param_source_exhaustive.sh
#
# Exit codes:
#   0 - every ParamSource match outside the definition site is exhaustive
#   1 - a catch-all arm was found, or the guard could not verify itself

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

DEFINITION="crates/gglib-core/src/domain/sampling_provenance.rs"

# Emits `V:<line>:<text>` per violation and a final `M:<count>` of the
# ParamSource matches it scanned.
read -r -d '' DETECTOR <<'AWK' || true
function indent_of(s,   p) { p = match(s, /[^ \t]/); return p == 0 ? -1 : p - 1 }
{
  line = $0

  # Whole-line comments are not code. Trailing comments are left alone: a `//`
  # after an arm does not change what the arm is.
  if (line ~ /^[[:space:]]*\/\//) next
  if (line ~ /^[[:space:]]*$/) next

  ind = indent_of(line)
  is_arm = (line ~ /^[[:space:]]*(\|[[:space:]]*)?ParamSource::[A-Za-z_][A-Za-z0-9_]*/) \
           && (line ~ /=>/)
  is_catchall = (line ~ /^[[:space:]]*(_|[a-z][a-zA-Z0-9_]*)[[:space:]]*(if[^=]*)?=>/)

  if (!in_match) {
    if (is_arm) { in_match = 1; arm_indent = ind; reported = 0; scanned++ }
    next
  }

  # The enclosing block closed: anything after this belongs to something else.
  if (ind < arm_indent && line ~ /\}/) { in_match = 0; next }

  if (ind == arm_indent && !reported) {
    if (is_arm) next
    if (is_catchall) { printf "V:%d:%s\n", FNR, line; reported = 1 }
  }
}
END { printf "M:%d\n", scanned + 0 }
AWK

# Run the detector over one file. Prints violations as `<file>:<line>: <text>`
# on stdout and echoes the scanned-match count on fd 3.
detect() {
  local file="$1" out
  out="$(awk "$DETECTOR" "$file")"
  printf '%s\n' "$out" | while IFS= read -r rec; do
    case "$rec" in
      V:*) printf '%s:%s\n' "$file" "${rec#V:}" ;;
    esac
  done
  printf '%s\n' "$out" | sed -n 's/^M:\([0-9]*\)$/\1/p'
}

# ── Self-test: prove the detector can still fail ─────────────────────────────

selftest() {
  local dir good_hits
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN

  cat >"$dir/bad.rs" <<'RS'
fn describe(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Layer(_) => "layer",
        _ => "everything else",
    }
}
RS

  cat >"$dir/good.rs" <<'RS'
fn describe(source: ParamSource) -> &'static str {
    match source {
        ParamSource::Layer(_) => "layer",
        ParamSource::Floor | ParamSource::FloorCoupled => "floor",
        ParamSource::Unset => "unset",
    }
}

fn nested(source: ParamSource) -> String {
    match source {
        ParamSource::Layer(i) => match lookup(i) {
            Some(name) => name.to_owned(),
            None => format!("layer {i}"),
        },
        ParamSource::Floor | ParamSource::FloorCoupled => "floor".to_owned(),
        ParamSource::Unset => "unset".to_owned(),
    }
}

fn constructs(won: Option<usize>) -> ParamSource {
    match won {
        Some(i) => ParamSource::Layer(i),
        _ => ParamSource::Floor,
    }
}
RS

  if [ "$(detect "$dir/bad.rs" | grep -c ':.*=>' || true)" -ne 1 ]; then
    echo -e "${RED}❌ self-test: the detector no longer catches a wildcard arm${NC}"
    detect "$dir/bad.rs" | sed 's/^/     /'
    return 1
  fi

  good_hits="$(detect "$dir/good.rs" | grep -c ':.*=>' || true)"
  if [ "$good_hits" -ne 0 ]; then
    echo -e "${RED}❌ self-test: the detector flags an exhaustive match${NC}"
    detect "$dir/good.rs" | sed 's/^/     /'
    return 1
  fi

  echo -e "${GREEN}✓${NC} self-test: known-bad caught, known-good and a ParamSource-constructing match cleared"
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

mapfile -t candidates < <(
  grep -rl 'ParamSource' --include='*.rs' crates/ src-tauri/ tests/ 2>/dev/null \
    | grep -v "^${DEFINITION}$" \
    | sort
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

A match over `ParamSource` uses a catch-all arm.

Adding a variant would then change behaviour at that site silently instead of
failing the build. Spell out every variant, or — better — express the question
as a method on `ParamSource` itself, next to `is_deliberate_choice`, so there
is one exhaustive answer rather than several.
MSG
  exit 1
fi

# Liveness. A guard that checks nothing reports exactly what a guard that
# found nothing wrong reports, which is how the previous version of this
# script passed CI while detecting literally zero matches.
if [ "$scanned_total" -eq 0 ]; then
  echo -e "${RED}❌ no ParamSource matches were found to check${NC}"
  echo
  echo "Either every match moved into ${DEFINITION} — in which case delete this"
  echo "script — or the detector has stopped recognising them, in which case its"
  echo "success means nothing. It found none, and there were four the last time"
  echo "anyone looked."
  exit 1
fi

echo -e "${GREEN}✅ every ParamSource match outside its definition is exhaustive${NC} (${scanned_total} scanned in ${#candidates[@]} files)"
