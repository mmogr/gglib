#!/usr/bin/env bash
# Record the trial a `/tried` comment on a pin pull request reports.
#
#   record_trial.sh <base> <head> <comment file> <pull request number>
#
# <base> and <head> are checkouts of the default branch and of the pull
# request's head. <comment file> is the comment's body, as
# scripts/engine_trial.py posts it: a first line
#
#   /tried <llama|sd> <release>
#
# then one fenced `json` block with what that script saw on the owner's
# machine: what the engine's binary says it is, the platform installed, when,
# the machine and its backend, the gglib that ran it, and how many replies
# and pictures gglib saved since the install.
#
# It writes <head>/contracts/upstream/<engine>/trial.json, which
# engine_contract_tests.rs holds to the pin, and prints the commit subject:
# `chore(contracts): <product> <release> was tried`. The subject holds the
# product and a release of the engine's own tag form only, so nothing a
# person wrote can link, mention or close anything from it.
#
# Nothing is written unless every check passes, and the first that fails
# names itself on stderr:
#
# - the first line is a request, for an engine there is, and <release> is a
#   tag of that engine's form;
# - <head> pins that engine at <release>, and <base> does not: the pull
#   request moves that pin, to that release;
# - the block is one JSON object with exactly the keys engine_trial.py
#   writes, each a short printable line with no backtick, its engine and
#   release the request's;
# - the binary's line is of <release>: llama-server's build is the `n` of
#   `b<n>`, sd-server's commit the last part of `master-<n>-<commit>`;
# - the engine was used: at least one reply for llama.cpp, at least one
#   picture for stable-diffusion.cpp;
# - the release is not already on record as tried.
#
# engine-trial.yml runs it with main's copy of this script against the pull
# request's files. A self-test runs first, as approve_reading.sh's does, on
# scripts/fixtures/engine-trial-comment.txt: the sample engine_trial.py's own
# self-test holds its comment to, so the writer and this reader cannot drift.

set -euo pipefail

command -v jq >/dev/null || { echo "record_trial.sh needs jq" >&2; exit 2; }

SCRIPTS=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SAMPLE=$SCRIPTS/fixtures/engine-trial-comment.txt

# What a value shown on a pull request may be: printable ASCII, no backtick.
PLAIN='^[ -_a-~]{1,200}$'

# pin <checkout> <file> <constant>: the release the constant names there.
pin() {
  sed -nE "s/.*$3: &str = \"([^\"]+)\".*/\1/p" "$1/$2" 2>/dev/null | head -1
}

# record <base> <head> <comment> <number>: check, write, print the subject.
# Returns 1 with the problem on stderr.
record() {
  local base=$1 head=$2 comment=$3 number=$4
  local first engine release product file constant form binary_form used_key

  [[ $number =~ ^[0-9]+$ ]] || { echo "the pull request's number is not a number" >&2; return 1; }

  first=$(tr -d '\r' <"$comment" | head -1)
  [[ $first =~ ^/tried\ ([a-z]+)\ ([^[:space:]]+)$ ]] \
    || { echo "the comment's first line is not '/tried <llama|sd> <release>'" >&2; return 1; }
  engine=${BASH_REMATCH[1]} release=${BASH_REMATCH[2]}

  case $engine in
    llama)
      product=llama.cpp file=crates/gglib-core/src/domain/engine_pins.rs constant=PINNED_LLAMA_RELEASE
      form='^b[0-9]+$' used_key=replies
      ;;
    sd)
      product=stable-diffusion.cpp file=crates/gglib-runtime/src/sd/install/release.rs constant=PINNED_SD_RELEASE
      form='^master-[0-9]+-[0-9a-f]+$' used_key=pictures
      ;;
    *) echo "there is no engine '$engine': it is llama or sd" >&2; return 1 ;;
  esac
  [[ $release =~ $form ]] || { echo "'$release' is not a $product release tag" >&2; return 1; }

  local at_head at_base
  at_head=$(pin "$head" "$file" "$constant")
  at_base=$(pin "$base" "$file" "$constant")
  [ -n "$at_head" ] && [ -n "$at_base" ] || { echo "$constant could not be read from $file" >&2; return 1; }
  [ "$at_head" = "$release" ] \
    || { echo "the pull request pins $product at $at_head, and the comment tried $release" >&2; return 1; }
  [ "$at_base" != "$release" ] \
    || { echo "the pull request does not move $product's pin: $release is the pin already" >&2; return 1; }

  # The one fenced json block, by its fences.
  local block
  block=$(mktemp)
  tr -d '\r' <"$comment" | awk '
    /^```json[[:space:]]*$/ && !seen { inside = 1; seen = 1; next }
    /^```[[:space:]]*$/ && inside { inside = 0; next }
    inside { print }' >"$block"
  jq -e 'type == "object"' "$block" >/dev/null 2>&1 \
    || { rm -f "$block"; echo "the comment has no fenced json block holding one JSON object" >&2; return 1; }

  # llama-server prints `(build <n>, commit …)`; sd-server `… commit <hex>`.
  if [ "$engine" = llama ]; then
    binary_form="\\(build ${release#b},"
  else
    binary_form="commit ${release##*-}\$"
  fi

  local problem
  problem=$(jq -r --arg engine "$engine" --arg release "$release" --arg plain "$PLAIN" \
    --arg binary "$binary_form" --arg used "$used_key" --arg product "$product" '
    def text($k): (.[$k] | type) == "string" and (.[$k] | test($plain));
    def count($k): (.used[$k] | type) == "number" and .used[$k] >= 0 and .used[$k] == (.used[$k] | floor);
    if (keys | sort) != ["backend","binary","engine","gglib","installed_at","machine","platform","release","used"]
      then "the json block does not hold exactly the keys engine_trial.py writes"
    elif ([text("backend"), text("binary"), text("engine"), text("gglib"), text("installed_at"),
           text("machine"), text("platform"), text("release")] | all | not)
      then "a value in the json block is not a short printable line"
    elif (.used | type) != "object" or (.used | keys | sort) != ["pictures","replies"]
         or (count("pictures") | not) or (count("replies") | not)
      then "used, in the json block, is not a count of replies and of pictures"
    elif .engine != $engine or .release != $release
      then "the json block is of \(.engine) \(.release), and the request of \($engine) \($release)"
    elif (.installed_at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$") | not)
      then "installed_at is not a UTC time to the second"
    elif (.binary | test($binary) | not)
      then "the binary says it is \"\(.binary)\", which is not \($release)"
    elif .used[$used] < 1
      then "\($product) was not used: the trial counts no \($used)"
    else "" end' "$block")
  if [ -n "$problem" ]; then
    rm -f "$block"
    echo "$problem" >&2
    return 1
  fi

  local trial=$head/contracts/upstream/$engine/trial.json
  if [ -f "$trial" ] && jq -e --arg r "$release" '.release == $r and .trial != null' "$trial" >/dev/null 2>&1; then
    rm -f "$block"
    echo "$release is already on record as tried" >&2
    return 1
  fi

  local tmp
  tmp=$(mktemp)
  jq -S --indent 2 --argjson number "$number" '
    { release: .release,
      trial: { pull_request: $number, installed_at, binary, platform, machine, backend, gglib, used } }' \
    "$block" >"$tmp"
  rm -f "$block"
  mkdir -p "$(dirname "$trial")"
  mv "$tmp" "$trial"
  printf 'chore(contracts): %s %s was tried\n' "$product" "$release"
}

# ── Self-test: the sample recorded, each fault refused ──────────────────────

# plant <dir> <llama pin> <sd pin>: a checkout with the two pin files.
plant() {
  mkdir -p "$1/crates/gglib-core/src/domain" "$1/crates/gglib-runtime/src/sd/install" \
    "$1/contracts/upstream/llama" "$1/contracts/upstream/sd"
  printf 'pub const PINNED_LLAMA_RELEASE: &str = "%s";\n' "$2" >"$1/crates/gglib-core/src/domain/engine_pins.rs"
  printf '//! notes\npub const PINNED_SD_RELEASE: &str = "%s";\n' "$3" >"$1/crates/gglib-runtime/src/sd/install/release.rs"
}

selftest() {
  local dir status=0 out
  dir=$(mktemp -d)
  plant "$dir/base" b12359 master-1025-8ee08e3
  plant "$dir/head" b12359 master-1094-3bad892
  local seed='{"note":"the pin when trials began; it was not tried under them","release":"master-1025-8ee08e3","trial":null}'
  echo "$seed" >"$dir/head/contracts/upstream/sd/trial.json"
  local trial=$dir/head/contracts/upstream/sd/trial.json

  [ -f "$SAMPLE" ] || { echo "❌ self-test: $SAMPLE is missing"; rm -rf "$dir"; return 1; }

  # 1. The sample, with a carriage return at each line's end as a browser
  #    may send it, is recorded.
  awk '{ printf "%s\r\n", $0 }' "$SAMPLE" >"$dir/good.txt"
  if ! out=$(record "$dir/base" "$dir/head" "$dir/good.txt" 2073 2>&1); then
    echo "❌ self-test: the sample comment was refused: $out"; status=1
  fi
  [ "$out" = "chore(contracts): stable-diffusion.cpp master-1094-3bad892 was tried" ] \
    || { echo "❌ self-test: subject was: $out"; status=1; }
  local want='{"release":"master-1094-3bad892","trial":{"backend":"Metal","binary":"stable-diffusion.cpp version unknown, commit 3bad892","gglib":"0.38.0-rc.1","installed_at":"2026-11-17T09:30:00Z","machine":"macOS 27.2 arm64","platform":"macOS universal (Metal)","pull_request":2073,"used":{"pictures":1,"replies":3}}}'
  [ "$(jq -c -S . "$trial")" = "$want" ] \
    || { echo "❌ self-test: trial.json was not written as expected:"; cat "$trial"; status=1; }

  # 2. The same comment again: the release is on record.
  if out=$(record "$dir/base" "$dir/head" "$dir/good.txt" 2073 2>&1); then
    echo "❌ self-test: a release already tried was recorded again"; status=1
  elif ! grep -qF "already on record as tried" <<<"$out"; then
    echo "❌ self-test: the second record was refused without saying why: $out"; status=1
  fi

  # 3. A llama.cpp bump, composed in the sample's form.
  plant "$dir/head-llama" b12400 master-1025-8ee08e3
  sed -e 's|^/tried sd master-1094-3bad892$|/tried llama b12400|' \
      -e 's|"engine": "sd"|"engine": "llama"|' \
      -e 's|"release": "master-1094-3bad892"|"release": "b12400"|' \
      -e 's|"binary": "[^"]*"|"binary": "version: 1.0.0-dev (build 12400, commit b1b770d90)"|' \
      -e 's|"platform": "[^"]*"|"platform": "macOS ARM64 (Metal)"|' "$SAMPLE" >"$dir/llama.txt"
  if ! out=$(record "$dir/base" "$dir/head-llama" "$dir/llama.txt" 2090 2>&1) \
      || [ "$out" != "chore(contracts): llama.cpp b12400 was tried" ]; then
    echo "❌ self-test: a llama.cpp trial was not recorded: $out"; status=1
  fi
  [ "$(jq -r '.release, .trial.pull_request' "$dir/head-llama/contracts/upstream/llama/trial.json" | tr '\n' ' ')" = "b12400 2090 " ] \
    || { echo "❌ self-test: llama's trial.json was not written as expected"; status=1; }

  # 4. Each fault is refused with its reason and writes nothing. A fault is a
  #    sed program over the sample, then the words the refusal must hold.
  echo "$seed" >"$trial"
  local fault program words
  for fault in \
    '1s|^/tried|/try|@is not '"'"'/tried <llama|sd> <release>'"'"'' \
    '1s|sd|whisper|@there is no engine '"'"'whisper'"'"'' \
    '1s|master-1094-3bad892|latest|@is not a stable-diffusion.cpp release tag' \
    '1s|master-1094-3bad892|master-1098-b521db4|@the pull request pins stable-diffusion.cpp at master-1094-3bad892' \
    '1s|sd master-1094-3bad892|llama b12359|@the pull request does not move llama.cpp'"'"'s pin' \
    '/^```json/d@no fenced json block' \
    's|"machine": "[^"]*",||@does not hold exactly the keys' \
    's|"backend": "Metal"|"backend": "Metal", "note": "x"|@does not hold exactly the keys' \
    's|"machine": "[^"]*"|"machine": "a `b`"|@is not a short printable line' \
    's|"gglib": "[^"]*"|"gglib": 3|@is not a short printable line' \
    's|"pictures": 1|"pictures": "1"|@is not a count of replies and of pictures' \
    's|"pictures": 1|"pictures": 0|@stable-diffusion.cpp was not used: the trial counts no pictures' \
    's|"release": "master-1094-3bad892"|"release": "master-1025-8ee08e3"|@and the request of sd master-1094-3bad892' \
    's|"installed_at": "[^"]*"|"installed_at": "yesterday"|@installed_at is not a UTC time' \
    's|commit 3bad892"|commit 8ee08e3"|@which is not master-1094-3bad892' \
    's|commit 3bad892"|commit 3bad8921"|@which is not master-1094-3bad892'; do
    program=${fault%%@*} words=${fault##*@}
    sed -e "$program" "$SAMPLE" >"$dir/bad.txt"
    if out=$(record "$dir/base" "$dir/head" "$dir/bad.txt" 2073 2>&1); then
      echo "❌ self-test: '$program' was accepted"; status=1
    elif ! grep -qF "$words" <<<"$out"; then
      echo "❌ self-test: '$program' refused without '$words': $out"; status=1
    fi
    [ "$(cat "$trial")" = "$seed" ] \
      || { echo "❌ self-test: a refused comment ('$program') changed trial.json"; status=1; echo "$seed" >"$trial"; }
  done

  # 5. A number that is none, and a comment with nothing in it.
  if out=$(record "$dir/base" "$dir/head" "$dir/good.txt" '2073; rm' 2>&1); then
    echo "❌ self-test: a pull request number that is not one was accepted"; status=1
  fi
  : >"$dir/empty.txt"
  if out=$(record "$dir/base" "$dir/head" "$dir/empty.txt" 2073 2>&1); then
    echo "❌ self-test: an empty comment was accepted"; status=1
  fi
  [ "$(cat "$trial")" = "$seed" ] || { echo "❌ self-test: a refused request changed trial.json"; status=1; }

  rm -rf "$dir"
  return "$status"
}

selftest || { echo "record_trial.sh: the self-test failed, so nothing it wrote could be trusted." >&2; exit 1; }

[ $# -eq 4 ] || { sed -n '4p' "$0" | sed 's/^# *//' >&2; exit 2; }
record "$1" "$2" "$3" "$4"
