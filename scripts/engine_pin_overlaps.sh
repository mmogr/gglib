#!/usr/bin/env bash
# Write the "Upstream work near gglib's own features" section of an engine
# pin pull request's body to stdout.
#
#   engine_pin_overlaps.sh <llama|sd> <current> <candidate> <before> <after>
#
# <before> and <after> are folders laid out as contracts/upstream, as
# engine_pin_body.sh takes them, which calls this. The rows of
# contracts/upstream/overlaps.json for the engine say what gglib does that the
# engine could come to do itself, and what of upstream's structure to watch
# for it: files, README headings, the code identifiers behind its flags, the
# words it uses, and the recorder's readings. This reads upstream at both tags
# with `gh api` and says, per row, what moved near it:
#
#   1. a new or changed row of the server README's option table, attributed
#      by the `params.<field>` and `LLAMA_ARG_*` identifiers `common/arg.cpp`
#      gives the flag, or "no row" when none matches (llama.cpp only: sd-server
#      has no such table, and its --help is diffed by engine_pin_body.sh);
#   2. a source file added directly under the server folder, with no row needed;
#   3. a README (llama) or api.md (sd) heading whose section grew, by row;
#   4. a route llama-server registers that came or went, in bold when gglib
#      asks it (the recorder's shape.txt names every endpoint gglib reads);
#   5. a commit in the range that touches a file a row names and whose subject
#      uses one of the row's words, under that row; a commit that touches a
#      row's file but says none of its words waits in the fold at the end;
#   6. a plain reading a row names that changed between the two recordings;
#   7. a path or heading a row names that the candidate no longer has, said,
#      never dropped;
#   8. one fold of what came that matched no row.
#
# Nothing here fails: a part that cannot be read says so. Upstream text goes
# into the body as engine_pin_body.sh puts it: in code spans, backticks
# removed, `#N` made plain so nothing links, mentions or closes here.
#
# Backticks in the printf formats below are Markdown, not command substitution.
# shellcheck disable=SC2016

set -euo pipefail

[ $# -eq 5 ] || { sed -n '5p' "$0" | sed 's/^# *//' >&2; exit 2; }
ENGINE=$1 CURRENT=$2 CANDIDATE=$3 BEFORE=$4 AFTER=$5
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TABLE=$ROOT_DIR/contracts/upstream/overlaps.json
TICK='`'

case $ENGINE in
  llama) REPO=ggml-org/llama.cpp PRODUCT=llama.cpp SERVER=tools/server
         README=tools/server/README.md ARGS=common/arg.cpp ROUTES=tools/server/server.cpp ;;
  sd)    REPO=leejet/stable-diffusion.cpp PRODUCT=stable-diffusion.cpp SERVER=examples/server
         README=examples/server/api.md ARGS='' ROUTES='' ;;
  *) echo "engine must be llama or sd, not $ENGINE" >&2; exit 2 ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
UNREAD=()   # what could not be read, one line each
UNMATCHED=() # what came and matched no row

# --- helpers ---------------------------------------------------------------

# Upstream text made safe for the body: printable, no backticks, `#N` plain.
safe() {
  tr -cd '[:print:]' | tr -d '`' | sed -E "s/(^|[^A-Za-z0-9_.\/-])#([0-9]+)/\1${REPO#*/}#\2/g"
}

# fetch <path> <tag> <out>: the file at the tag, or return 1 with the reason
# in $TMP/gh.err.
fetch() {
  gh api -H 'Accept: application/vnd.github.raw' "repos/$REPO/contents/$1?ref=$2" > "$3" 2> "$TMP/gh.err"
}

# tree <tag> <out>: every path at the tag, one per line.
tree() {
  gh api "repos/$REPO/git/trees/$1?recursive=1" --jq '.tree[].path' > "$2" 2> "$TMP/gh.err"
}

# The rows for this engine, as `id<TAB>json`.
rows() {
  jq -r --arg e "$ENGINE" 'to_entries[] | select(.value.engine == $e) | "\(.key)\t\(.value | tojson)"' "$TABLE"
}

# row_list <id> <jq path>: one list item per line.
row_list() {
  jq -r --arg id "$1" ".[\$id]$2[]?" "$TABLE"
}

# A row's label: `id` and its ADR when it has one.
label() {
  local adr
  adr=$(jq -r --arg id "$1" '.[$id].adr // empty' "$TABLE")
  if [ -n "$adr" ]; then printf '**%s** (ADR %s)' "$1" "$adr"; else printf '**%s**' "$1"; fi
}

# The README heading each line of a file sits under, as `heading<TAB>line`.
with_headings() {
  awk '/^#{1,6} / { h = $0 } { print h "\t" $0 }' "$1"
}

# --- the readings ----------------------------------------------------------

# One line per row: what moved near it. Collected first, printed by row.
declare -A NEAR=()
near() { NEAR[$1]+="- $2"$'\n'; }

tree "$CURRENT" "$TMP/tree.before" || UNREAD+=("the tree at $CURRENT: $(head -1 "$TMP/gh.err" | safe)")
tree "$CANDIDATE" "$TMP/tree.after" || UNREAD+=("the tree at $CANDIDATE: $(head -1 "$TMP/gh.err" | safe)")
fetch "$README" "$CURRENT" "$TMP/readme.before" || UNREAD+=("$README at $CURRENT: $(head -1 "$TMP/gh.err" | safe)")
fetch "$README" "$CANDIDATE" "$TMP/readme.after" || UNREAD+=("$README at $CANDIDATE: $(head -1 "$TMP/gh.err" | safe)")
if [ -n "$ARGS" ]; then
  fetch "$ARGS" "$CANDIDATE" "$TMP/args.after" || UNREAD+=("$ARGS at $CANDIDATE: $(head -1 "$TMP/gh.err" | safe)")
fi
if [ -n "$ROUTES" ]; then
  fetch "$ROUTES" "$CURRENT" "$TMP/routes.before" || UNREAD+=("$ROUTES at $CURRENT: $(head -1 "$TMP/gh.err" | safe)")
  fetch "$ROUTES" "$CANDIDATE" "$TMP/routes.after" || UNREAD+=("$ROUTES at $CANDIDATE: $(head -1 "$TMP/gh.err" | safe)")
fi

# 1. The option table's new and changed rows, attributed by identifier.
options_section() {
  [ -s "$TMP/readme.before" ] && [ -s "$TMP/readme.after" ] && [ -s "$TMP/args.after" ] || return 0
  local side
  for side in before after; do
    sed -n '/HELP_START/,/HELP_END/p' "$TMP/readme.$side" | grep -E '^\| `-' > "$TMP/options.$side" || true
  done
  comm -13 <(sort "$TMP/options.before") <(sort "$TMP/options.after") > "$TMP/options.new"
  [ -s "$TMP/options.new" ] || return 0
  local line flag block ids id hit changed
  while IFS= read -r line; do
    flag=$(sed -E 's/^\| `([^`, ]+).*/\1/' <<<"$line")
    changed=""
    grep -qE "^\| ${TICK}${flag}([ ,\`])" "$TMP/options.before" && changed=" (text changed)"
    # The identifiers behind the flag: from the line naming it to the next
    # option's add_opt.
    block=$(awk -v f="\"$flag\"" '!on && index($0, f) { on = 1; print; next } on && /add_opt\(/ { exit } on { print }' "$TMP/args.after")
    ids=$( { grep -oE 'params\.[A-Za-z0-9_.]+' <<<"$block" | sed 's/^params\./params:/'
            grep -oE 'set_env\("[A-Z0-9_]+"' <<<"$block" | sed -E 's/set_env\("(.*)"/env:\1/'; } | sort -u)
    hit=""
    while IFS=$'\t' read -r id json; do
      if jq -e --argjson ids "$(jq -R . <<<"$ids" | jq -s .)" '
          [.upstream.ids[] | select(startswith("params:") or startswith("env:"))] as $want
          | any($ids[]; . as $have | any($want[]; . as $w | $have | startswith($w)))' <<<"$json" >/dev/null; then
        hit=$id; break
      fi
    done < <(rows)
    if [ -n "$hit" ]; then
      near "$hit" "$(printf '`%s`%s, a flag%s' "$flag" "$changed" "$( [ -n "$ids" ] && printf ' (%s)' "$(paste -sd, - <<<"$ids" | sed 's/,/, /g')")")"
    else
      UNMATCHED+=("flag $TICK$flag$TICK$changed$( [ -n "$ids" ] && printf ' (%s)' "$(paste -sd, - <<<"$ids" | sed 's/,/, /g')")")
    fi
  done < "$TMP/options.new"
}

# 2. Source files added directly under the server folder.
new_modules_section() {
  [ -s "$TMP/tree.before" ] && [ -s "$TMP/tree.after" ] || return 0
  comm -13 <(sort "$TMP/tree.before") <(sort "$TMP/tree.after") \
    | grep -E "^$SERVER/[^/]+\.(cpp|h|hpp)$" > "$TMP/modules.new" || true
  [ -s "$TMP/modules.new" ] || return 0
  local path id hit
  while IFS= read -r path; do
    hit=""
    while IFS=$'\t' read -r id json; do
      if jq -e --arg p "$path" '[.upstream.paths[] | ltrimstr("!")] | index($p) != null' <<<"$json" >/dev/null; then hit=$id; break; fi
    done < <(rows)
    if [ -n "$hit" ]; then near "$hit" "new module $TICK$path$TICK"; else UNMATCHED+=("new module $TICK$path$TICK"); fi
  done < "$TMP/modules.new"
}

# heading_row <heading>: the row one of whose headings the README heading
# starts with (a README heading carries a description after the name), or
# nothing.
heading_row() {
  local id h
  while IFS=$'\t' read -r id json; do
    while IFS= read -r h; do
      [ -n "$h" ] || continue
      if [[ $1 == "$h"* ]]; then echo "$id"; return; fi
    done < <(row_list "$id" .upstream.headings)
  done < <(rows)
}

# 3. Headings whose sections grew, by row.
headings_section() {
  [ -s "$TMP/readme.before" ] && [ -s "$TMP/readme.after" ] || return 0
  # The lines the README gained, each counted under its heading in the after
  # file.
  diff "$TMP/readme.before" "$TMP/readme.after" | grep '^> ' | cut -c3- > "$TMP/readme.added" || true
  [ -s "$TMP/readme.added" ] || return 0
  awk -F'\t' 'NR == FNR { added[$0]++; next }
              NF >= 2 && $1 != "" && added[substr($0, index($0, "\t") + 1)] > 0 {
                added[substr($0, index($0, "\t") + 1)]--; count[$1]++ }
              END { for (h in count) printf "%d\t%s\n", count[h], h }' \
    "$TMP/readme.added" <(with_headings "$TMP/readme.after") | sort -rn > "$TMP/headings.grown"
  [ -s "$TMP/headings.grown" ] || return 0
  local count heading hit
  while IFS=$'\t' read -r count heading; do
    [ -n "$heading" ] || continue
    hit=$(heading_row "$heading")
    if [ -n "$hit" ]; then
      near "$hit" "$(printf '%s +%s lines' "$(printf '%s' "$heading" | safe)" "$count")"
    elif [ "$count" -ge 5 ]; then
      UNMATCHED+=("$(printf '%s +%s lines' "$(printf '%s' "$heading" | safe)" "$count")")
    fi
  done < "$TMP/headings.grown"
}

# 4. Routes that came or went; bold when gglib asks the route.
routes_section() {
  [ -s "$TMP/routes.before" ] && [ -s "$TMP/routes.after" ] || return 0
  local side
  for side in before after; do
    grep -oE 'ctx_http\.(get|post|ws|del|put) *\("[^"]+"' "$TMP/routes.$side" \
      | sed -E 's/ctx_http\.([a-z]+) *\("([^"]+)"/\1 \2/' | sort -u > "$TMP/routes.$side.list"
  done
  local asked route verb
  asked=$(cut -f1 "$AFTER/llama/shape.txt" 2>/dev/null | grep '^/' | sed 's/ .*//' | sort -u || true)
  while read -r verb route; do
    [ -n "$route" ] || continue
    UNMATCHED+=("new route $TICK$verb $route$TICK")
  done < <(comm -13 "$TMP/routes.before.list" "$TMP/routes.after.list")
  while read -r verb route; do
    [ -n "$route" ] || continue
    if grep -qxF "$route" <<<"$asked"; then
      UNMATCHED+=("**route $TICK$verb $route$TICK is gone, and gglib asks it**")
    else
      UNMATCHED+=("route $TICK$verb $route$TICK is gone")
    fi
  done < <(comm -23 "$TMP/routes.before.list" "$TMP/routes.after.list")
}

# 5. Commits touching a row's files whose subject uses a row's words.
commits_section() {
  local own since
  if ! own=$(gh api "repos/$REPO/commits/$CURRENT" --jq '.sha + " " + .commit.committer.date' 2> "$TMP/gh.err"); then
    UNREAD+=("the commits since $CURRENT: $(head -1 "$TMP/gh.err" | safe)")
    return 0
  fi
  since=${own#* } own=${own%% *}
  : > "$TMP/commits"
  local id json path
  while IFS=$'\t' read -r id json; do
    while IFS= read -r path; do
      path=${path#!}
      [ -n "$path" ] || continue
      if ! gh api --paginate "repos/$REPO/commits?sha=$CANDIDATE&path=${path%/}&since=$since&per_page=100" \
          --jq ".[] | select(.sha != \"$own\") | [\"$id\", .sha, (.commit.message | split(\"\n\")[0]), .html_url] | @tsv" \
          >> "$TMP/commits" 2> "$TMP/gh.err"; then
        UNREAD+=("the commits touching $path: $(head -1 "$TMP/gh.err" | safe)")
        return 0
      fi
    done < <(row_list "$id" .upstream.paths)
  done < <(rows)
  [ -s "$TMP/commits" ] || return 0
  # Attribute: the subject uses one of the row's phrases (case-insensitive,
  # at a word start). A commit is named once per row; one named by no row
  # waits in the fold, once, with the rows whose files it touches.
  local sha subject url lower phrase hit
  declare -A seen=() attributed=() touches=() subjects=() urls=()
  while IFS=$'\t' read -r id sha subject url; do
    [ -n "$sha" ] || continue
    [ -z "${seen[$id $sha]-}" ] || continue
    seen[$id $sha]=1
    subjects[$sha]=$subject urls[$sha]=$url
    touches[$sha]+="${touches[$sha]:+, }$id"
    lower=$(tr '[:upper:]' '[:lower:]' <<<"$subject" | tr '_/-' '   ')
    hit=""
    while IFS= read -r phrase; do
      [ -n "$phrase" ] || continue
      if grep -qE "(^|[^a-z0-9])$(tr '_/-' '   ' <<<"$phrase")" <<<"$lower"; then hit=1; break; fi
    done < <(row_list "$id" .upstream.phrases)
    if [ -n "$hit" ]; then
      attributed[$sha]=1
      near "$id" "$(printf '[`%s`](%s) `%s`' "${sha:0:9}" "$url" "$(printf '%s' "$subject" | safe)")"
    fi
  done < "$TMP/commits"
  for sha in "${!subjects[@]}"; do
    [ -z "${attributed[$sha]-}" ] || continue
    UNMATCHED+=("$(printf '[`%s`](%s) `%s` (touches the files of %s)' "${sha:0:9}" "${urls[$sha]}" "$(printf '%s' "${subjects[$sha]}" | safe)" "${touches[$sha]}")")
  done
}

# 6. Plain readings a row names that changed.
readings_section() {
  local before=$BEFORE/llama/observed.json after=$AFTER/llama/observed.json
  [ -f "$before" ] && [ -f "$after" ] || return 0
  local id glob key was now
  while IFS=$'\t' read -r id json; do
    while IFS= read -r glob; do
      [ -n "$glob" ] || continue
      while IFS=$'\t' read -r key was now; do
        [ -n "$key" ] || continue
        near "$id" "reading $TICK$key$TICK: $was → $now"
      done < <(jq -r --arg g "^$(sed 's/\./\\./g; s/\*/.*/g' <<<"$glob")\$" --slurpfile b "$before" '
        $b[0] as $b | to_entries[] | select(.key | test($g))
        | select(($b[.key].value | tojson) != (.value.value | tojson))
        | [.key, (($b[.key].value // "absent") | tojson), (.value.value | tojson)] | @tsv' "$after")
    done < <(row_list "$id" .readings)
  done < <(rows)
}

# 7. Paths and headings a row names that the candidate no longer has.
staleness_section() {
  [ -s "$TMP/tree.after" ] || return 0
  local id path heading
  while IFS=$'\t' read -r id json; do
    while IFS= read -r path; do
      path=${path#!}
      [ -n "$path" ] || continue
      if ! grep -qE "^${path%/}(/|$)" "$TMP/tree.after"; then
        near "$id" "**$TICK$path$TICK is not at $CANDIDATE**: the row names a path upstream moved or dropped"
      fi
    done < <(row_list "$id" .upstream.paths)
    if [ -s "$TMP/readme.after" ]; then
      while IFS= read -r heading; do
        [ -n "$heading" ] || continue
        if ! awk -v h="$heading" 'index($0, h) == 1 { found = 1 } END { exit !found }' "$TMP/readme.after"; then
          near "$id" "**heading $TICK$(printf '%s' "$heading" | safe)$TICK is not in $README at $CANDIDATE**"
        fi
      done < <(row_list "$id" .upstream.headings)
    fi
  done < <(rows)
}

options_section
new_modules_section
headings_section
routes_section
commits_section
readings_section
staleness_section

# --- the section -----------------------------------------------------------

printf '### Upstream work near gglib'"'"'s own features\n\n'
any=0
while IFS=$'\t' read -r id json; do
  [ -n "${NEAR[$id]-}" ] || continue
  any=1
  printf '%s\n%s\n' "$(label "$id")" "${NEAR[$id]}"
done < <(rows)
if [ "$any" -eq 0 ]; then
  printf 'Nothing in %s from %s to %s landed near a row of `contracts/upstream/overlaps.json`.\n\n' "$PRODUCT" "$CURRENT" "$CANDIDATE"
fi
if [ "${#UNMATCHED[@]}" -gt 0 ]; then
  printf '<details><summary>%s things that came and matched no row</summary>\n\n' "${#UNMATCHED[@]}"
  printf -- '- %s\n' "${UNMATCHED[@]}"
  printf '\n</details>\n\n'
fi
if [ "${#UNREAD[@]}" -gt 0 ]; then
  printf 'Not read, so not compared: %s.\n\n' "$(printf '%s; ' "${UNREAD[@]}" | sed 's/; $//')"
fi
