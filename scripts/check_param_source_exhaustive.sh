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
#   - `is_floor` had the same problem in the other direction: a new variant
#     silently reads as "not the floor".
#
# The type system cannot enforce this on its own — `matches!` and a `match`
# with `_ =>` are both legal — so this is a grep, in the same spirit as
# check_transport_branching.sh and check-frontend-ipc.sh.
#
# The definition site is exempt: `sampling_provenance.rs` is where the
# exhaustive answer lives, and its own helpers are the thing being protected.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEFINITION="crates/gglib-core/src/domain/sampling_provenance.rs"

cd "$ROOT"

violations=0

# Files that mention ParamSource at all, minus the definition site.
mapfile -t candidates < <(
  grep -rl 'ParamSource' --include='*.rs' crates/ 2>/dev/null \
    | grep -v "^${DEFINITION}$" \
    | sort
)

for file in "${candidates[@]:-}"; do
  [ -n "$file" ] || continue

  # A `match` on ParamSource, or a `matches!(x, ParamSource::...)`, whose
  # block contains a catch-all. Scanned with awk because the arms are on
  # following lines.
  offenders=$(
    awk '
      /match[[:space:]].*ParamSource|matches!\(.*ParamSource/ { inblock = 1; depth = 0; start = NR }
      inblock {
        n = gsub(/\{|\(/, "&"); m = gsub(/\}|\)/, "&")
        depth += n - m
        if ($0 ~ /(^|[^[:alnum:]_])_[[:space:]]*=>/ || $0 ~ /\.\.[[:space:]]*=>/) {
          print start ": " $0
          inblock = 0
          next
        }
        if (depth <= 0 && NR > start) { inblock = 0 }
      }
    ' "$file" || true
  )

  if [ -n "$offenders" ]; then
    echo "❌ $file"
    echo "$offenders" | sed 's/^/     /'
    violations=$((violations + 1))
  fi
done

if [ "$violations" -gt 0 ]; then
  cat <<'MSG'

A match over `ParamSource` uses a catch-all arm.

Adding a variant would then change behaviour at that site silently instead of
failing the build. Spell out every variant, or — better — express the question
as a method on `ParamSource` itself, next to `is_floor` and
`is_deliberate_choice`, so there is one exhaustive answer rather than several.
MSG
  exit 1
fi

echo "✅ every ParamSource match outside its definition is exhaustive"
