#!/usr/bin/env bash
# Write the body of an engine pin pull request to stdout.
#
#   engine_pin_body.sh <llama|sd> <current> <candidate> <before> <after> <recorder> <run url> [<in-process>]
#
# <before> and <after> are folders laid out as contracts/upstream: the one
# committed on the default branch, and the one the recorder just wrote at the
# candidate. <recorder> is `ok`, or why the recorder did not run. On a
# llama.cpp bump <in-process> says the same of the in-process engine's
# recording into llama-inproc/; without it the body says nothing of it. The
# Engine pins workflow (.github/workflows/engine-pins.yml) calls it;
# CONTRIBUTING.md's "The engine pins" says what a reader does with the body.
#
# A llama.cpp body leads with the ADR readings that moved: each reading in the
# new observed.json that an ADR's criterion names and that differs from its
# entry in approvals.json, with the criterion as observed.json quotes it,
# checked against the ADR file. Then one line of what else the recording saw,
# the tier-3 probes (contracts/upstream/llama/tier3/), and the upstream
# commits in the range that touch a file an ADR in docs/adr/ cites, read with
# `gh api`. A stable-diffusion.cpp body adds the real picture to take, and
# has no tier 3. Nothing in it fails: a part that cannot be read says so.
#
# Backticks in the printf formats below are Markdown, not command substitution.
# shellcheck disable=SC2016

set -euo pipefail

[ $# -eq 7 ] || [ $# -eq 8 ] || { sed -n '4p' "$0" | sed 's/^# *//' >&2; exit 2; }
ENGINE=$1 CURRENT=$2 CANDIDATE=$3 BEFORE=$4 AFTER=$5 RECORDER=$6 RUN_URL=$7 INPROC=${8-}
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TICK='`'

case $ENGINE in
  llama) CONSTANT=PINNED_LLAMA_RELEASE PRODUCT=llama.cpp REPO=ggml-org/llama.cpp
         CITED='(tools/server|tools/mtmd|common)/[A-Za-z0-9_./-]+\.(cpp|h|hpp)' ;;
  sd)    CONSTANT=PINNED_SD_RELEASE PRODUCT=stable-diffusion.cpp REPO=leejet/stable-diffusion.cpp
         CITED='(^|[^A-Za-z0-9_/])src/[A-Za-z0-9_./-]+\.(cpp|h|hpp)' ;;
  *) echo "engine must be llama or sd, not $ENGINE" >&2; exit 2 ;;
esac

# `name` for each value, with up to five names after a count.
names() {
  local count
  count=$(grep -c . <<<"$1" || true)
  head -5 <<<"$1" | sed "s/.*/$TICK&$TICK/" | paste -sd, - | sed 's/,/, /g' \
    | { read -r list; [ "$count" -gt 5 ] && list="$list, …"; echo "$list"; }
}

# "1 thing", "2 things".
plural() {
  if [ "$1" = 1 ]; then echo "1 $2"; else echo "$1 ${2}s"; fi
}

# "+2 (`a`, `b`) −0", or "unchanged", from the lines of two files.
delta() {
  local added removed out=""
  added=$(comm -13 <(sort -u "$1") <(sort -u "$2"))
  removed=$(comm -23 <(sort -u "$1") <(sort -u "$2"))
  [ -z "$added$removed" ] && { echo unchanged; return; }
  out="+$(grep -c . <<<"$added" || true)"
  [ -n "$added" ] && out="$out ($(names "$added"))"
  out="$out −$(grep -c . <<<"$removed" || true)"
  [ -n "$removed" ] && out="$out ($(names "$removed"))"
  echo "$out"
}

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# The first flag of each option a --help lists: a line indented eight spaces
# or fewer whose first word is a flag, as pinned_engines_tests.rs reads it.
options() {
  [ -f "$1" ] || { : > "$2"; return; }
  awk '{ match($0, /^ */) } RLENGTH <= 8 && $1 ~ /^--?[A-Za-z0-9]/ { sub(/,$/, "", $1); print $1 }' "$1" > "$2"
}

# The key paths shape.txt lists under the endpoints matching $3.
keys() {
  if [ -f "$1" ]; then awk -F'\t' -v e="$3" '$1 ~ e { print $2 }' "$1"; fi | sort -u > "$2"
}

# --- the ADR readings that moved -------------------------------------------

# One line per moved reading: adr, criterion, if_moved, reading, approved, now.
# A reading approvals.json names that the recording no longer has is "gone".
moved_readings() {
  local observed=$AFTER/llama/observed.json approvals=$AFTER/llama/approvals.json
  local before=$BEFORE/llama/observed.json
  [ -f "$before" ] || before=$observed
  jq -r --slurpfile a "$approvals" --slurpfile b "$before" '
    ($a[0]) as $a | ($b[0]) as $b
    | (with_entries(select(.value.adr != null))) as $named
    | ( $named | to_entries[] | .key as $k
        | select(($a | has($k) | not) or $a[$k].approved != .value.value)
        | [.value.adr, .value.criterion, .value.if_moved, $k,
           (if $a | has($k) then $a[$k].approved | tojson else "no approval" end),
           (.value.value | tojson)] ),
      ( $a | keys[] | . as $k | select($named | has($k) | not)
        | ($b[$k] // {}) as $o
        | [($o.adr // "?"), ($o.criterion // ""), "remove its entry", $k,
           ($a[$k].approved | tojson), "gone"] )
    | @tsv' "$observed"
}

# The ADR file for a number, and whether it still holds the quote.
adr_names() {
  local adr=$1 quote=$2 docs doc
  docs=("$ROOT_DIR"/docs/adr/"$adr"-*.md)
  doc=${docs[0]}
  if [ -f "$doc" ] && [ "$(jq -n --rawfile doc "$doc" --arg q "$quote" '
      def norm: gsub("(?m)^[ \t]*>[ \t]?"; "") | gsub("\\s+"; " ") | gsub("~~.*?~~"; "") | gsub("\\s+"; " ");
      ($doc | norm) | contains($q | norm)')" = true ]; then
    printf 'ADR %s: "%s"' "$adr" "$quote"
  else
    printf 'ADR %s (its quote is no longer in docs/adr/%s-…)' "$adr" "$adr"
  fi
}

readings_section() {
  local rows criteria count lines
  rows=$(moved_readings)
  if [ -z "$rows" ]; then
    printf '**Nothing an ADR watches moved.**\n\n'
    return
  fi
  count=$(grep -c . <<<"$rows")
  criteria=$(cut -f1,2 <<<"$rows" | sort -u | grep -c .)
  local noun="criterion" it="it is"
  [ "$criteria" -gt 1 ] && noun="criteria"
  [ "$count" -gt 1 ] && it="they are"
  local extra=""
  [ "$count" -gt "$criteria" ] && extra=" ($count readings)"
  printf '**%s ADR %s moved%s. This PR is red until %s approved.**\n' "$criteria" "$noun" "$extra" "$it"
  # One bullet per criterion, its readings first.
  while IFS=$'\t' read -r adr criterion; do
    lines=$(awk -F'\t' -v a="$adr" -v c="$criterion" -v t="$TICK" '
      $1 == a && $2 == c { printf "%s%s%s%s: %s → %s", sep, t, $4, t, $5, $6; sep = "; "; m = $3 }
      END { printf "\t%s", m }' <<<"$rows")
    local readings=${lines%%$'\t'*} if_moved=${lines#*$'\t'}
    printf -- '- %s. %s → %s. Approve in `contracts/upstream/llama/approvals.json`, in this PR.\n' \
      "$readings" "$(adr_names "$adr" "$criterion")" "$if_moved"
  done < <(cut -f1,2 <<<"$rows" | sort -u)
  echo
}

# --- what else the recording saw -------------------------------------------

recorded_line() {
  local side
  if [ "$ENGINE" = sd ]; then
    options "$BEFORE/sd/help.txt" "$TMP/sd.before"; options "$AFTER/sd/help.txt" "$TMP/sd.after"
    printf 'Recorded on %s: sd-server `--help` options %s.\n\n' "$CANDIDATE" "$(delta "$TMP/sd.before" "$TMP/sd.after")"
    return
  fi
  for side in before after; do
    local dir=$BEFORE; [ $side = after ] && dir=$AFTER
    options "$dir/llama/help.txt" "$TMP/help.$side"
    keys "$dir/llama/shape.txt" "$TMP/props.$side" '^/props$'
    keys "$dir/llama/shape.txt" "$TMP/slots.$side" '^/slots'
    if [ -f "$dir/llama/shape.txt" ]; then cat "$dir/llama/shape.txt"; fi > "$TMP/shape.$side"
    jq -r 'to_entries[] | select(.value.adr == null) | "\(.key)\t\(.value.value | tojson)"' \
      "$dir/llama/observed.json" 2>/dev/null > "$TMP/plain.$side" || : > "$TMP/plain.$side"
  done
  local shape others
  shape=$(comm -3 <(sort "$TMP/shape.before") <(sort "$TMP/shape.after") | grep -c . || true)
  others=$(comm -3 <(sort "$TMP/plain.before") <(sort "$TMP/plain.after") | sed 's/^\t//' | cut -f1 | sort -u)
  printf 'Recorded on %s: `--help` options %s; `/props` keys %s; `/slots` keys %s; %s' \
    "$CANDIDATE" "$(delta "$TMP/help.before" "$TMP/help.after")" \
    "$(delta "$TMP/props.before" "$TMP/props.after")" "$(delta "$TMP/slots.before" "$TMP/slots.after")" \
    "$(plural "$shape" "shape.txt line") changed"
  if [ -n "$others" ]; then
    printf '; %s no ADR names changed (%s)' "$(plural "$(grep -c . <<<"$others")" reading)" "$(names "$others")"
  fi
  printf '.\n\n'
}

# --- tier 3 ----------------------------------------------------------------

tier3_line() {
  local now=$AFTER/llama/tier3/readings.json was=$BEFORE/llama/tier3/readings.json
  if [ ! -f "$now" ]; then
    printf 'Qwen3.5-0.8B: the tier-3 probes wrote no readings.\n\n'
    return
  fi
  [ -f "$was" ] || was=/dev/null
  jq -rn --slurpfile now "$now" --rawfile was "$was" '
    ($now[0]) as $n | (try ($was | fromjson) catch null) as $w
    | def word: if . == true then "yes" elif . == false then "no" else "not run" end;
      def why($p): if $p.value == true then "" else ", `\($p.reason // "no reason given" | gsub("`"; ""))`" end;
      [["tool_call_native", "tool call native"], ["tool_call_parsed", "tool call parsed"],
       ["reasoning_split", "reasoning split"],
       ["image_in_tool_message_answered", "image in tool message answered"]] as $probes
    | ($n.model // "the model" | sub("-Q[0-9].*$"; "") | sub("\\.gguf$"; "")) as $model
    | if $w != null and all($probes[]; $n[.[0]].value == $w[.[0]].value) then
        "\($model): \([$probes[] | .[1]] | join(", ")): unchanged"
        + ([$probes[] | select($n[.[0]].value != true) | "\(.[1]): \($n[.[0]].value | word)\(why($n[.[0]]))"]
           | if length > 0 then "; \(join("; "))" else "" end) + "."
      else
        "\($model)\(if $w == null then " (first reading)" else "" end): "
        + ([$probes[] | .[0] as $k | $n[$k] as $p
            | "\(.[1]): \(if $w != null and $w[$k].value != $p.value then "\($w[$k].value | word) → " else "" end)\($p.value | word)\(why($p))"]
           | join("; ")) + "."
      end'
  echo
}

# --- the in-process engine -------------------------------------------------

# Why llama-inproc/ was not recorded, at the top of the body.
inproc_failed() {
  if [ -n "$INPROC" ] && [ "$INPROC" != ok ]; then
    printf '**The in-process engine was not recorded (`%s`): `contracts/upstream/llama-inproc/` is still the last pin'"'"'s, so `Test (Rust)` is red until someone runs `make record-upstream-inproc` on a Mac.**\n\n' "${INPROC//\`/}"
  fi
}

# What the in-process recording says of itself against llama-server's.
inproc_line() {
  [ "$INPROC" = ok ] || return 0
  jq -r --arg c "$CANDIDATE" '
    [to_entries[] | select(.key | startswith("template_parity.")) | .value.value] as $p
    | "The in-process engine, recorded on \($c): \([$p[] | select(. == true)] | length) of \($p | length) pinned templates render as llama-server renders them, and tinygemma3 streams \(if .["greedy_parity.tinygemma3"].value == true then "the same" else "**another**" end) greedy reply."' \
    "$AFTER/llama-inproc/observed.json" 2>/dev/null || echo 'The in-process engine wrote no readings.'
  echo
}

# --- upstream commits that touch a file an ADR cites -----------------------

commits_section() {
  local paths since list
  paths=$(grep -ohE "$CITED" "$ROOT_DIR"/docs/adr/*.md | sed -E 's|^[^s]*src/|src/|' | sort -u | head -40)
  if [ -z "$paths" ]; then return; fi
  local own
  if ! own=$(gh api "repos/$REPO/commits/$CURRENT" --jq '.sha + " " + .commit.committer.date' 2> "$TMP/gh.err"); then
    printf 'The %s commits that touch files the ADRs cite were not read: %s\n\n' "$PRODUCT" "$(head -1 "$TMP/gh.err")"
    return
  fi
  since=${own#* } own=${own%% *}
  : > "$TMP/commits"
  while read -r path; do
    if ! gh api "repos/$REPO/commits?sha=$CANDIDATE&path=$path&since=$since&per_page=100" \
        --jq ".[] | [.commit.committer.date, .sha, (.commit.message | split(\"\n\")[0]), .html_url, \"$path\"] | @tsv" \
        >> "$TMP/commits" 2> "$TMP/gh.err"; then
      printf 'The %s commits that touch files the ADRs cite were not read: %s\n\n' "$PRODUCT" "$(head -1 "$TMP/gh.err")"
      return
    fi
  done <<<"$paths"
  # One line per commit, newest first, with every cited file it touches. A
  # subject goes in a code span with its #N made plain, so the body neither
  # links nor closes anything in this repository.
  list=$(sort -r "$TMP/commits" \
    | awk -F'\t' -v t="$TICK" '
        !($2 in seen) { seen[$2] = 1; order[++n] = $2; subj[$2] = $3; url[$2] = $4 }
        { files[$2] = files[$2] (files[$2] ? ", " : "") t $5 t }
        END { for (i = 1; i <= n; i++) { s = order[i]; m = subj[s]; gsub(/`/, "", m)
              printf "- [%s%s%s](%s) %s%s%s (%s)\n", t, substr(s, 1, 9), t, url[s], t, m, t, files[s] } }' \
    | sed -E "s/(^|[^A-Za-z0-9_.\/-])#([0-9]+)/\1${REPO#*/}#\2/g")
  # The range starts after the current pin, which `since` includes.
  list=$(grep -v "/commit/$own)" <<<"$list" || true)
  local count
  count=$(grep -c . <<<"$list" || true)
  if [ "$count" -eq 0 ]; then
    printf 'No %s commit from %s to %s touches a file the ADRs cite.\n\n' "$PRODUCT" "$CURRENT" "$CANDIDATE"
    return
  fi
  printf '<details><summary>%s %s commits touch files the ADRs cite</summary>\n\n' "$count" "$PRODUCT"
  head -50 <<<"$list"
  [ "$count" -gt 50 ] && printf -- '- … and %s more\n' $((count - 50))
  printf '\n</details>\n\n'
}

# --- the body --------------------------------------------------------------

printf 'Moves %s from %s to %s: https://github.com/%s/compare/%s...%s\n\n' \
  "$TICK$CONSTANT$TICK" "$TICK$CURRENT$TICK" "$TICK$CANDIDATE$TICK" "$REPO" "$CURRENT" "$CANDIDATE"

# Reasons come from logs and probes: in a code span, nothing in them links
# or mentions anyone.
if [ "$RECORDER" != ok ]; then
  printf '**The recorder did not run (`%s`); "Compare with contracts/upstream" is red until someone records.**\n\n' "${RECORDER//\`/}"
fi
inproc_failed
# Every bump records llama-server again, so an sd bump reads the ADR
# readings too, before its picture.
if [ "$RECORDER" = ok ]; then
  readings_section
fi
if [ "$ENGINE" = sd ]; then
  printf -- '- [ ] Take a real picture on %s before merging, with ADR 0018'"'"'s render-beside-a-chat pair.\n\n' "$TICK$CANDIDATE$TICK"
fi
if [ "$RECORDER" = ok ]; then
  recorded_line
fi
inproc_line
if [ "$ENGINE" = llama ]; then
  tier3_line
fi
commits_section

printf 'Push fixes onto this branch: a closed candidate is not proposed again, and nothing newer is proposed while this is open. Opened by the Engine pins workflow: %s\n' "$RUN_URL"
