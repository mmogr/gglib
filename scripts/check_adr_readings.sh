#!/usr/bin/env bash
# Fail when a reading an ADR's criterion names has moved and nobody has said
# what that means.
#
# `contracts/upstream/llama/observed.json` is written by the recorder
# (`crates/gglib-runtime/src/engine_contract_tests.rs`) from what the pinned
# llama-server answered. Each leaf an ADR names carries `adr`, the
# `criterion` quoted from that ADR, `how_read` and `if_moved`.
# `contracts/upstream/llama/approvals.json` is written by hand, one entry per
# such leaf: `{"approved": <the value>, "note": "<why>"}` and nothing else.
#
# This fails when:
#   - a reading differs from its approval, or has none;
#   - an approval names no ADR reading;
#   - an approval is not exactly `approved` and `note`, or its note is not one
#     of: `deleting <stage> (#<issue>)`, `keeping, because <reason>`,
#     `probe changed in this PR`, `first reading, <tag>`;
#   - a quoted criterion is not in its ADR file. Both sides are compared with
#     blockquote markers dropped, whitespace collapsed and `~~struck~~` text
#     removed, so a struck criterion no longer counts as quoted.
#
# It reads the two committed files and never the engine, so a recorder that
# fails cannot make it fail. A pull request that changes what a probe sends
# moves a reading too; "probe changed in this PR" is the note for that.
#
# CI runs it as the `ADR criterion moved` job, and `make pre-commit` through
# `make adr-readings`. A self-test runs first: it plants one of each fault in
# a scratch pair and stops unless each is reported.
#
# A multi-line message joins its lines with %0A, so GitHub keeps them all in
# the annotation.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LLAMA="contracts/upstream/llama"

command -v jq >/dev/null || { echo "check_adr_readings.sh needs jq" >&2; exit 2; }

# The bump readings no recording takes, and who takes them.
MANUAL_ONLY=(
  "ADR 0002: the measured table (3-4B models, N=30). Rerun scripts/experiments/lazy_grammar_conformance.py when an auto_grammar reading flips."
  "ADR 0016: an image in a tool message needs a model that can describe it. The Engine pins workflow's tier 3 records one at each llama.cpp bump, in contracts/upstream/llama/tier3/."
  "ADR 0018: a real picture, and the render-beside-a-chat pair. Taken by hand at a stable-diffusion.cpp bump."
)

# check <observed.json> <approvals.json> <adr folder>: print one ::error line
# per problem, and return 1 if there is any. It is called as `check || …`,
# which turns `set -e` off inside it, so every command whose failure would
# otherwise pass unseen ends in `|| return 1`.
check() {
  local observed="$1" approvals="$2" adrs="$3" failures=0 moved quotes adr criterion reading doc found
  local file="$LLAMA/approvals.json" name path

  # Both files must be one JSON object each, or nothing below means anything.
  for path in "$observed" "$approvals"; do
    name="${path##*/}"
    if ! jq -e 'type == "object"' "$path" >/dev/null 2>&1; then
      echo "::error title=$name is not valid JSON::$LLAMA/$name must exist and hold one JSON object; it is missing, does not parse, or is another type."
      failures=$((failures + 1))
    fi
  done
  if [ "$failures" -gt 0 ]; then
    return 1
  fi

  # Readings against approvals, and approvals against readings.
  moved="$(jq -r --slurpfile approvals "$approvals" --arg file "$file" '
    $approvals[0] as $a
    | (with_entries(select(.value.adr != null))) as $named
    | def how_to_approve($k; $v):
        "To approve, set in \($file):%0A  \($k | tojson): {\"approved\": \($v | tojson), \"note\": \"deleting <stage> (#<issue>)\"}%0Aor a note starting \"keeping, because …\" or \"probe changed in this PR\".";
      ( $named | to_entries[]
        | .key as $k | .value as $o
        | ($o.value | tojson) as $now
        | if ($a | has($k) | not) then
            "::error title=ADR criterion moved::\($k) reads \($now) on this tree; \($file) has no entry for it.%0AADR \($o.adr): \"\($o.criterion)\" → \($o.if_moved).%0A" + how_to_approve($k; $o.value)
          elif $a[$k].approved != $o.value then
            "::error title=ADR criterion moved::\($k) is now \($now); \($file) says \($a[$k].approved | tojson).%0AADR \($o.adr): \"\($o.criterion)\" → \($o.if_moved).%0A" + how_to_approve($k; $o.value)
          else empty end ),
      ( $a | to_entries[]
        | .key as $k
        | select($named | has($k) | not)
        | "::error title=ADR criterion moved::\($file) approves \($k), which observed.json does not record as an ADR reading. Remove the entry, or restore the reading." ),
      ( $a | to_entries[]
        | select((.value | type) != "object"
            or (.value | keys) != ["approved", "note"]
            or (.value.note | type) != "string"
            or (.value.note | test("^(deleting .+ \\(#[0-9]+\\)|keeping, because .+|probe changed in this PR|first reading, .+)$") | not))
        | "::error title=approvals.json entry is malformed::\(.key) must be {\"approved\": <value>, \"note\": \"deleting <stage> (#<issue>)\" | \"keeping, because …\" | \"probe changed in this PR\" | \"first reading, <tag>\"}." )
  ' "$observed")" || return 1
  if [ -n "$moved" ]; then
    printf '%s\n' "$moved"
    failures=$((failures + $(grep -c '^::error' <<<"$moved")))
  fi

  # Each quoted criterion is still in its ADR.
  quotes="$(jq -r '
    [to_entries[] | select(.value.adr != null) | {adr: .value.adr, criterion: .value.criterion, key}]
    | group_by([.adr, .criterion])[]
    | [.[0].adr, .[0].criterion, (.[0].key + (if length > 1 then " (and \(length - 1) more)" else "" end))]
    | @tsv' "$observed")" || return 1
  while IFS=$'\t' read -r adr criterion reading; do
    [ -n "$adr" ] || continue
    doc="$(find "$adrs" -maxdepth 1 -name "$adr-*.md")" || return 1
    doc="${doc%%$'\n'*}"
    if [ -z "$doc" ]; then
      echo "::error title=ADR quote is stale::$reading names ADR $adr, and docs/adr/ has no $adr-*.md."
      failures=$((failures + 1))
      continue
    fi
    found="$(jq -n --rawfile doc "$doc" --arg quote "$criterion" '
      def norm: gsub("(?m)^[ \t]*>[ \t]?"; "") | gsub("\\s+"; " ") | gsub("~~.*?~~"; "") | gsub("\\s+"; " ");
      ($doc | norm) | contains($quote | norm)')" || return 1
    if [ "$found" != "true" ]; then
      echo "::error title=ADR quote is stale::$reading quotes text not found in docs/adr/${doc##*/}."
      failures=$((failures + 1))
    fi
  done <<<"$quotes"

  if [ "$failures" -gt 0 ]; then
    echo "$failures ADR reading problem(s) in $LLAMA."
    return 1
  fi
}

# ── Self-test: one of each fault, each of which must be reported ────────────

selftest() {
  local dir out status=0
  dir="$(mktemp -d)"
  mkdir "$dir/adr"
  printf '%s\n' '# 0099' '' '> A criterion that is' '> still quoted.' '' '- ~~A struck' '  criterion.~~' >"$dir/adr/0099-selftest.md"
  jq -n '
    def r($v; $c): {value: $v, adr: "0099", criterion: $c, how_read: "h", if_moved: "m"};
    { moved: r(true; "A criterion that is still quoted."),
      missing: r(1; "A criterion that is still quoted."),
      bad_note: r(2; "A criterion that is still quoted."),
      extra_key: r(3; "A criterion that is still quoted."),
      stale: r(4; "A criterion nobody wrote."),
      struck: r(5; "A struck criterion."),
      plain: {value: 6} }' >"$dir/observed.json"
  jq -n '
    def ok($v): {approved: $v, note: "first reading, b1"};
    { moved: ok(false), bad_note: {approved: 2, note: "because"},
      extra_key: (ok(3) + {by: "me"}), stale: ok(4), struck: ok(5), ghost: ok(7) }' >"$dir/approvals.json"
  out="$(check "$dir/observed.json" "$dir/approvals.json" "$dir/adr")" && status=1
  for expected in \
    "ADR criterion moved::moved is now true" \
    "ADR criterion moved::missing reads 1" \
    "ADR criterion moved::$LLAMA/approvals.json approves ghost," \
    "approvals.json entry is malformed::bad_note " \
    "approvals.json entry is malformed::extra_key " \
    "ADR quote is stale::stale quotes" \
    "ADR quote is stale::struck quotes"; do
    if ! grep -qF "::error title=$expected" <<<"$out"; then
      echo "❌ self-test: no \"$expected\" for its planted fault"
      status=1
    fi
  done
  if [ "$(grep -c '^::error' <<<"$out")" -ne 7 ]; then
    echo "❌ self-test: expected 7 errors for 7 planted faults:"
    printf '%s\n' "$out" | sed 's/^/     /'
    status=1
  fi

  # Files that are not one JSON object: each must stop the check on its own.
  mkdir "$dir/broken"
  cp "$dir/observed.json" "$dir/broken/observed.json"
  for fault in trailing-comma missing array observed; do
    case "$fault" in
      trailing-comma) printf '{"moved": {"approved": true, "note": "first reading, b1"},}\n' >"$dir/broken/approvals.json" ;;
      missing) rm -f "$dir/broken/approvals.json" ;;
      array) echo '[]' >"$dir/broken/approvals.json" ;;
      observed) cp "$dir/approvals.json" "$dir/broken/approvals.json"; echo '{"moved": ' >"$dir/broken/observed.json" ;;
    esac
    expected="approvals.json is not valid JSON"
    [ "$fault" = observed ] && expected="observed.json is not valid JSON"
    if out="$(check "$dir/broken/observed.json" "$dir/broken/approvals.json" "$dir/adr")"; then
      echo "❌ self-test: a check with $fault passed"
      status=1
    elif [ "$(grep -c "^::error title=$expected::" <<<"$out")" -ne 1 ]; then
      echo "❌ self-test: no \"$expected\" for $fault:"
      printf '%s\n' "$out" | sed 's/^/     /'
      status=1
    fi
  done
  rm -rf "$dir"
  return "$status"
}

selftest || { echo "check_adr_readings.sh: the self-test failed, so a clean run would prove nothing." >&2; exit 1; }

check "$ROOT_DIR/$LLAMA/observed.json" "$ROOT_DIR/$LLAMA/approvals.json" "$ROOT_DIR/docs/adr" || exit 1

count="$(jq '[.[] | select(.adr != null)] | length' "$ROOT_DIR/$LLAMA/observed.json")"
echo "Self-test passed: each of 11 planted faults was reported."
echo "All $count ADR readings in $LLAMA/observed.json match their approvals, and every quote is in its ADR."
echo "Taken by hand, not here:"
printf '  %s\n' "${MANUAL_ONLY[@]}"
