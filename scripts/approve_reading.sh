#!/usr/bin/env bash
# Write the approvals a `/approve` comment on a pull request asks for.
#
#   approve_reading.sh <observed.json> <approvals.json> <comment file>
#
# <observed.json> and <approvals.json> are the pull request's
# `contracts/upstream/llama/` pair (check_adr_readings.sh says what each
# holds). <comment file> is the comment's body, one request per line, lines
# that are not requests ignored:
#
#   /approve <key> keeping, because <reason>
#   /approve <key> deleting <stage> (#<issue>)
#   /approve <key> probe changed in this PR
#   /approve <key> first reading, <tag>
#   /approve <key> remove
#
# A request approves the reading <key> at the value observed.json records
# for it, with the note; the value is never typed. `remove` deletes an
# approval whose reading observed.json no longer records as an ADR reading.
# Every line is checked before any is written: <key> is a reading an ADR
# names (or, for `remove`, an approval with no such reading), the note is one
# of the four forms check_adr_readings.sh accepts, no key is requested twice,
# and no request repeats the approval already there. One bad line writes
# nothing and names itself on stderr. On success approvals.json is rewritten
# with `jq -S --indent 2`, the form it is kept in, and the commit subject is
# printed: `chore(contracts): approve <key> at <value>`, with the keys and
# values only; the note stays out of it, so nothing a person wrote can link,
# mention or close anything from a subject.
#
# approve-reading.yml runs it on the pull request's files with main's copy of
# this script, so a branch cut before the script existed is approved like any
# other. A self-test runs first, as check_adr_readings.sh's does.

set -euo pipefail

command -v jq >/dev/null || { echo "approve_reading.sh needs jq" >&2; exit 2; }

NOTE_RE='^(deleting .+ \(#[0-9]+\)|keeping, because .+|probe changed in this PR|first reading, .+)$'

# approve <observed> <approvals> <comment>: check every request, then write
# them all, then print the subject. Returns 1 with the problems on stderr.
approve() {
  local observed=$1 approvals=$2 comment=$3
  local -a keys=() notes=() subjects=()
  local line key note problems=0

  for path in "$observed" "$approvals"; do
    jq -e 'type == "object"' "$path" >/dev/null 2>&1 \
      || { echo "${path##*/} is missing, does not parse, or is not one JSON object" >&2; return 1; }
  done

  while IFS= read -r line; do
    line=${line%$'\r'}
    line=${line#"${line%%[![:space:]]*}"}
    [[ $line == "/approve "* ]] || continue
    line=${line#/approve }
    key=${line%% *}
    note=${line#"$key"}
    note=${note# }
    note=${note%"${note##*[![:space:]]}"}
    if [[ -z $key || -z $note ]]; then
      echo "a request needs a key and a note: /approve $line" >&2
      problems=$((problems + 1))
      continue
    fi
    if [[ " ${keys[*]-} " == *" $key "* ]]; then
      echo "$key: requested twice in one comment" >&2
      problems=$((problems + 1))
      continue
    fi
    if [[ $note == remove ]]; then
      if ! jq -e --arg k "$key" 'has($k)' "$approvals" >/dev/null; then
        echo "$key: nothing to remove; approvals.json has no entry for it" >&2
        problems=$((problems + 1))
      elif jq -e --arg k "$key" '.[$k].adr != null' "$observed" >/dev/null; then
        echo "$key: observed.json still records it as an ADR reading; approve it instead of removing it" >&2
        problems=$((problems + 1))
      fi
    else
      if ! jq -e --arg k "$key" 'has($k)' "$observed" >/dev/null; then
        echo "$key: no such reading in observed.json" >&2
        problems=$((problems + 1))
      elif ! jq -e --arg k "$key" '.[$k].adr != null' "$observed" >/dev/null; then
        echo "$key: no ADR names this reading, so it takes no approval" >&2
        problems=$((problems + 1))
      elif ! [[ $note =~ $NOTE_RE ]]; then
        echo "$key: the note must be one of: deleting <stage> (#<issue>) | keeping, because <reason> | probe changed in this PR | first reading, <tag>" >&2
        problems=$((problems + 1))
      elif jq -e --arg k "$key" --arg n "$note" --slurpfile o "$observed" \
          '.[$k] == {approved: $o[0][$k].value, note: $n}' "$approvals" >/dev/null; then
        echo "$key: already approved at that value with that note" >&2
        problems=$((problems + 1))
      fi
    fi
    keys+=("$key")
    notes+=("$note")
  done <"$comment"

  if [ "$problems" -gt 0 ]; then
    echo "$problems bad request(s); nothing written" >&2
    return 1
  fi
  if [ "${#keys[@]}" -eq 0 ]; then
    echo "no /approve line in the comment" >&2
    return 1
  fi

  local i tmp
  tmp=$(mktemp)
  cp "$approvals" "$tmp"
  for i in "${!keys[@]}"; do
    key=${keys[$i]} note=${notes[$i]}
    if [[ $note == remove ]]; then
      jq -S --indent 2 --arg k "$key" 'del(.[$k])' "$tmp" >"$tmp.next"
      subjects+=("remove $key")
    else
      jq -S --indent 2 --arg k "$key" --arg n "$note" --slurpfile o "$observed" \
        '.[$k] = {approved: $o[0][$k].value, note: $n}' "$tmp" >"$tmp.next"
      subjects+=("approve $key at $(jq -c --arg k "$key" '.[$k].value' "$observed")")
    fi
    mv "$tmp.next" "$tmp"
  done
  mv "$tmp" "$approvals"
  local joined
  joined=$(IFS=';'; echo "${subjects[*]}")
  printf 'chore(contracts): %s\n' "${joined//;/, }"
}

# ── Self-test: each fault refused, each good request written ────────────────

selftest() {
  local dir status=0 out
  dir=$(mktemp -d)
  jq -n '{ moved: {value: 261, adr: "0012", criterion: "c", how_read: "h", if_moved: "m"},
           flag: {value: true, adr: "0002", criterion: "c", how_read: "h", if_moved: "m"},
           same: {value: 0.8, adr: "0003", criterion: "c", how_read: "h", if_moved: "m"},
           dead: {value: 2},
           plain: {value: 6} }' >"$dir/observed.json"
  # `dead` is a reading that lost its ADR; `gone` one the recording no longer has.
  jq -n '{ moved: {approved: 258, note: "first reading, b1"},
           same: {approved: 0.8, note: "first reading, b1"},
           dead: {approved: 2, note: "first reading, b1"},
           gone: {approved: 1, note: "first reading, b1"} }' >"$dir/approvals.json"

  printf '%s\n' \
    'Looks fine to me.' \
    $'/approve moved keeping, because the tunnel reaches the daemon\r' \
    '  /approve flag first reading, b2' \
    '/approve gone remove' \
    '/approve dead remove' >"$dir/good.txt"
  if ! out=$(approve "$dir/observed.json" "$dir/approvals.json" "$dir/good.txt" 2>&1); then
    echo "❌ self-test: a good comment was refused: $out"; status=1
  fi
  [ "$out" = "chore(contracts): approve moved at 261, approve flag at true, remove gone, remove dead" ] \
    || { echo "❌ self-test: subject was: $out"; status=1; }
  [ "$(jq -c . "$dir/approvals.json")" = '{"flag":{"approved":true,"note":"first reading, b2"},"moved":{"approved":261,"note":"keeping, because the tunnel reaches the daemon"},"same":{"approved":0.8,"note":"first reading, b1"}}' ] \
    || { echo "❌ self-test: approvals.json was not written as expected:"; cat "$dir/approvals.json"; status=1; }

  local before='{"moved":{"approved":258,"note":"first reading, b1"},"same":{"approved":0.8,"note":"first reading, b1"}}'
  echo "$before" | jq -S --indent 2 . >"$dir/approvals.json"
  local fault
  for fault in \
    '/approve nosuch first reading, b2|no such reading' \
    '/approve plain first reading, b2|no ADR names this reading' \
    '/approve moved because|the note must be one of' \
    '/approve moved remove|still records it as an ADR reading' \
    '/approve nosuch remove|nothing to remove' \
    '/approve same first reading, b1|already approved' \
    '/approve same first reading, b1  |already approved' \
    '/approve moved|needs a key and a note' \
    $'/approve moved first reading, b2\n/approve moved first reading, b3|requested twice' \
    $'/approve moved first reading, b2\n/approve nosuch first reading, b2|no such reading' \
    'nothing here|no /approve line'; do
    printf '%s\n' "${fault%%|*}" >"$dir/bad.txt"
    if out=$(approve "$dir/observed.json" "$dir/approvals.json" "$dir/bad.txt" 2>&1); then
      echo "❌ self-test: '${fault%%|*}' was accepted"; status=1
    elif ! grep -qF "${fault##*|}" <<<"$out"; then
      echo "❌ self-test: '${fault%%|*}' refused without '${fault##*|}': $out"; status=1
    fi
    [ "$(jq -c . "$dir/approvals.json")" = "$before" ] \
      || { echo "❌ self-test: a refused comment changed approvals.json"; status=1; }
  done

  rm -rf "$dir"
  return "$status"
}

selftest || { echo "approve_reading.sh: the self-test failed, so nothing it wrote could be trusted." >&2; exit 1; }

[ $# -eq 3 ] || { sed -n '4p' "$0" | sed 's/^# *//' >&2; exit 2; }
approve "$1" "$2" "$3"
