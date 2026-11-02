#!/usr/bin/env bash
# Fail when a reading an ADR's criterion names has moved and nobody has said
# what that means.
#
# `contracts/upstream/llama/observed.json` is written by the recorder
# (`crates/gglib-runtime/src/engine_contract_tests.rs`) from what the pinned
# llama-server answered. Each leaf an ADR names carries `adr`, the
# `criterion` quoted from that ADR, `how_read` and `if_moved`.
# `contracts/upstream/llama/approvals.json` holds one entry per such leaf,
# `{"approved": <the value>, "note": "<why>"}` and nothing else, written by
# scripts/approve_reading.sh from a `/approve <key> <note>` comment on the
# pull request (approve-reading.yml).
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
# `make adr-readings`. With `ADR_COMMENT_FILE` set it also writes, to that
# file, the comment the job leaves on a pull request: each reading that
# moved, its ADR's criterion, and the `/approve` lines to choose from; the
# file is empty when no reading moved. A self-test runs first: it plants one
# of each fault in a scratch pair and stops unless each is reported, in the
# annotations and in the comment.
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

  # Readings against approvals, and approvals against readings: one row per
  # reading that moved, has no approval, or has an approval and no reading
  # (`moved`, `missing`, `ghost`), then the key, its ADR, the criterion, what
  # approvals.json says, what this tree reads, and for two lists what came
  # and went. A list is never printed whole: it is its length.
  local rows
  rows="$(jq -r --slurpfile approvals "$approvals" '
    $approvals[0] as $a
    | (with_entries(select(.value.adr != null))) as $named
    # Values are what the engine answered: in code spans, where nothing
    # links or mentions. A list is its length.
    | def brief: "`" + (if type == "array" then "a list of \(length)" else tojson end) + "`";
      def delta($old; $new):
        if ($old | type) == "array" and ($new | type) == "array" then
          " (\(([($new - $old)[] | "+`\(.)`"] + [($old - $new)[] | "−`\(.)`"]) | join(", ")))"
        else "" end;
      # An empty field is written as `-`: `read` with a tab IFS would fold
      # two tabs into one and shift the fields after it.
      def field: (. // "" | tostring | if . == "" then "-" else . end);
      ( $named | to_entries[] | .key as $k | .value as $o
        | if ($a | has($k) | not) then
            ["missing", $k, $o.adr, $o.criterion, $o.if_moved, "", ($o.value | brief), ""]
          elif $a[$k].approved != $o.value then
            ["moved", $k, $o.adr, $o.criterion, $o.if_moved, ($a[$k].approved | brief), ($o.value | brief), delta($a[$k].approved; $o.value)]
          else empty end ),
      ( $a | keys[] | . as $k | select($named | has($k) | not)
        | ["ghost", $k, "", "", "", "", "", ""] )
    | map(field) | @tsv' "$observed")" || return 1
  local kind key if_moved said now delta said_lines=() approve_lines=() v
  while IFS=$'\t' read -r kind key adr criterion if_moved said now delta; do
    [ -n "$kind" ] || continue
    for v in adr criterion if_moved said now delta; do
      [ "${!v}" = - ] && printf -v "$v" ''
    done
    failures=$((failures + 1))
    case $kind in
      moved)
        said_lines+=("\`$key\` has moved: $file says $said, this branch reads $now$delta.")
        said_lines+=("ADR $adr: \"$criterion\" → $if_moved.")
        approve_lines+=("/approve $key keeping, because <reason>" "/approve $key deleting <stage> (#<issue>)" "/approve $key probe changed in this PR")
        echo "::error title=ADR criterion moved::$key has moved: $file says $said, this tree reads $now$delta.%0AADR $adr: \"$criterion\" → $if_moved.%0ADecide, then comment on the pull request one of:%0A/approve $key keeping, because <reason>%0A/approve $key deleting <stage> (#<issue>)%0A/approve $key probe changed in this PR" ;;
      missing)
        said_lines+=("\`$key\` reads $now on this branch; $file has no entry for it.")
        said_lines+=("ADR $adr: \"$criterion\" → $if_moved.")
        approve_lines+=("/approve $key first reading, <tag>" "/approve $key keeping, because <reason>" "/approve $key deleting <stage> (#<issue>)")
        echo "::error title=ADR criterion moved::$key reads $now on this tree; $file has no entry for it.%0AADR $adr: \"$criterion\" → $if_moved.%0ADecide, then comment on the pull request one of:%0A/approve $key first reading, <tag>%0A/approve $key keeping, because <reason>%0A/approve $key deleting <stage> (#<issue>)" ;;
      ghost)
        said_lines+=("$file approves \`$key\`, which this branch no longer records as an ADR reading.")
        approve_lines+=("/approve $key remove")
        echo "::error title=ADR criterion moved::$file approves $key, which observed.json does not record as an ADR reading.%0AComment on the pull request:%0A/approve $key remove%0Aor restore the reading." ;;
    esac
  done <<<"$rows"

  # The same, as the comment the pull request gets: what moved, why it
  # matters, the lines to paste.
  if [ -n "${ADR_COMMENT_FILE-}" ]; then
    : > "$ADR_COMMENT_FILE"
    if [ "${#approve_lines[@]}" -gt 0 ]; then
      {
        echo '### ADR criterion moved'
        echo
        printf '%s\n\n' "${said_lines[@]}"
        echo 'Decide, then comment one of these per reading, filling in the brackets:'
        echo
        echo '```'
        printf '%s\n' "${approve_lines[@]}"
        echo '```'
        echo
        echo "The bot writes \`$file\` on this branch and CI runs again."
      } > "$ADR_COMMENT_FILE"
    fi
  fi

  # An approval is exactly `approved` and a note of one of the four forms.
  moved="$(jq -r '
    to_entries[]
    | select((.value | type) != "object"
        or (.value | keys) != ["approved", "note"]
        or (.value.note | type) != "string"
        or (.value.note | test("^(deleting .+ \\(#[0-9]+\\)|keeping, because .+|probe changed in this PR|first reading, .+)$") | not))
    | "::error title=approvals.json entry is malformed::\(.key) must be {\"approved\": <value>, \"note\": \"deleting <stage> (#<issue>)\" | \"keeping, because …\" | \"probe changed in this PR\" | \"first reading, <tag>\"}."
  ' "$approvals")" || return 1
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
      flags: r(["--a", "--b", "--c"]; "A criterion that is still quoted."),
      plain: {value: 6} }' >"$dir/observed.json"
  jq -n '
    def ok($v): {approved: $v, note: "first reading, b1"};
    { moved: ok(false), bad_note: {approved: 2, note: "because"},
      extra_key: (ok(3) + {by: "me"}), stale: ok(4), struck: ok(5), ghost: ok(7),
      flags: ok(["--a", "--z"]) }' >"$dir/approvals.json"
  out="$(ADR_COMMENT_FILE="$dir/comment.md" check "$dir/observed.json" "$dir/approvals.json" "$dir/adr")" && status=1
  for expected in \
    "ADR criterion moved::moved has moved: $LLAMA/approvals.json says \`false\`, this tree reads \`true\`." \
    "ADR criterion moved::flags has moved: $LLAMA/approvals.json says \`a list of 2\`, this tree reads \`a list of 3\` (+\`--b\`, +\`--c\`, −\`--z\`)." \
    "ADR criterion moved::missing reads \`1\` on this tree" \
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
  if [ "$(grep -c '^::error' <<<"$out")" -ne 8 ]; then
    echo "❌ self-test: expected 8 errors for 8 planted faults:"
    printf '%s\n' "$out" | sed 's/^/     /'
    status=1
  fi
  # The comment carries the four approvals, one line each to paste, and no
  # annotation markup; `plain`, which no ADR names, is not in it.
  local tick
  tick=$(printf '\x60')
  for expected in \
    '/approve moved keeping, because <reason>' \
    '/approve flags probe changed in this PR' \
    '/approve missing first reading, <tag>' \
    '/approve ghost remove' \
    "${tick}moved${tick} has moved: $LLAMA/approvals.json says ${tick}false${tick}, this branch reads ${tick}true${tick}." \
    "${tick}missing${tick} reads ${tick}1${tick} on this branch; $LLAMA/approvals.json has no entry for it." \
    "$LLAMA/approvals.json approves ${tick}ghost${tick}, which this branch no longer records as an ADR reading." \
    'ADR 0099: "A criterion that is still quoted." → m.'; do
    if ! grep -qF "$expected" "$dir/comment.md"; then
      echo "❌ self-test: the comment lacks \"$expected\""
      status=1
    fi
  done
  if grep -qE '::error|%0A|plain' "$dir/comment.md"; then
    echo "❌ self-test: the comment carries annotation markup or a plain reading:"
    sed 's/^/     /' "$dir/comment.md"
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
echo "Self-test passed: each of 12 planted faults was reported."
echo "All $count ADR readings in $LLAMA/observed.json match their approvals, and every quote is in its ADR."
echo "Taken by hand, not here:"
printf '  %s\n' "${MANUAL_ONLY[@]}"
