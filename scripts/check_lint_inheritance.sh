#!/usr/bin/env bash
# Every workspace member is held to the workspace lints, and the lints each
# one allows may not grow.
#
# Usage: ./scripts/check_lint_inheritance.sh [--update | --self-test]
#
# 1. Inheritance. A member's manifest has one lints table, `[lints]`, and its
#    only key is `workspace = true`. Cargo will not combine that key with a
#    crate's own lint keys, so a crate with any of its own is held to none of
#    the workspace's.
#
# 2. Allows. For each member, every lint named inside an `allow(...)` or
#    `expect(...)` attribute in its `.rs` files is counted, `cfg_attr` forms
#    included, whole-line `//` comments not. The files are the ones git
#    lists in the working tree: tracked, or untracked and not ignored. An
#    ignored file, such as build output or a worktree nested in the
#    checkout, is not counted. Two numbers per member, against
#    scripts/lint-allow-baseline.txt:
#
#      lints    every lint so named. The count may not exceed the baseline's
#               first number; `--update` sets that number to the count.
#      bare     the lints in an attribute with no `reason = "…"`. The count
#               may not exceed the baseline's second number, and `--update`
#               refuses to raise it.
#
#    A count below its baseline passes, and `--update` locks it in. An
#    attribute spelled out inside a string literal is counted too. A count
#    that failed, a baseline row listed twice and a baseline row whose second
#    or third field is not a number each fail the check, and `--update` then
#    writes nothing.
#
#    The listed `.rs` files that no member directory holds, such as one
#    `include!`d into several members' build scripts, are counted the same
#    way, in a row of their own named `(outside-members)`.
#
# The self-test runs before every check: five manifests that must
# fail rule 1 and a member without one, tried together and then the missing
# one and a bad one each alone; files whose counts are known; baselines each
# count must fail against; and a counter that exits 1 without printing a
# count, a row listed twice and a count field that is not a number, the row
# listed twice also under `--update`, which must leave the baseline as it
# was. Each fixture that must fail must also make the check exit non-zero.
# Its fixture repository reads no global or system git config and copies no
# init template, so an excludes file the caller keeps cannot hide a fixture.
#
# Exit codes: 0 pass, 1 a rule failed or the self-test did, 2 usage.

set -euo pipefail

# git finds each root's repository from the root itself. A hook in a linked
# worktree exports GIT_DIR and GIT_INDEX_FILE, and with them set the
# self-test would write its fixtures into the caller's index.
# shellcheck disable=SC2046
unset $(git rev-parse --local-env-vars)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
BASELINE="$SCRIPT_DIR/lint-allow-baseline.txt"
OUTSIDE="(outside-members)"

# Prints "<lints> <bare>" summed over the files named as arguments.
read -r -d '' COUNTER <<'PERL' || true
use strict; use warnings;
my $str = qr/"(?:[^"\\]|\\.)*"/s;
my $paren; $paren = qr/\((?:[^()"]++|$str|(??{$paren}))*\)/s;
my ($lints, $bare) = (0, 0);
sub list {
  my @items = grep { /\S/ } split /,/, ($_[0] =~ s/$str/""/gr);
  my $n = grep { !/^\s*reason\s*=/ } @items;
  $lints += $n;
  $bare += $n if $n == @items;
}
for my $file (@ARGV) {
  open my $fh, '<', $file or die "$file: $!";
  my $src = join '', grep { !m{^\s*//} } <$fh>;
  while ($src =~ /#!?\[\s*(allow|expect|cfg_attr)\s*($paren)\s*\]/g) {
    my ($kind, $args) = ($1, substr($2, 1, -1));
    if ($kind eq 'cfg_attr') {
      list(substr($1, 1, -1)) while $args =~ /\b(?:allow|expect)\s*($paren)/g;
    } else {
      list($args);
    }
  }
}
print "$lints $bare\n";
PERL

# The quoted paths in the root manifest's `members = [...]`.
members() {
  awk '
    /^[[:space:]]*members[[:space:]]*=[[:space:]]*\[/ { on = 1 }
    on {
      line = $0; sub(/#.*/, "", line)
      while (match(line, /"[^"]*"/)) {
        print substr(line, RSTART + 1, RLENGTH - 2)
        line = substr(line, RSTART + RLENGTH)
      }
      if ($0 ~ /\]/) exit
    }
  ' "$1/Cargo.toml"
}

# Empty when the manifest's lints are exactly `[lints]` + `workspace = true`;
# otherwise one line saying what is wrong.
lints_problem() {
  awk '
    /^[[:space:]]*\[/ {
      header = $0; sub(/^[[:space:]]*\[[[:space:]]*/, "", header); sub(/[[:space:]]*\].*/, "", header)
      in_lints = (header == "lints")
      if (header ~ /^lints\./) { print "declares [" header "]"; bad = 1; exit }
      if (in_lints) tables++
      next
    }
    in_lints && $0 !~ /^[[:space:]]*(#.*)?$/ {
      if ($0 ~ /^[[:space:]]*workspace[[:space:]]*=[[:space:]]*true[[:space:]]*(#.*)?$/) inherit++
      else { print "[lints] holds `" $0 "`"; bad = 1; exit }
    }
    END {
      if (bad) exit
      if (tables != 1) print "has " tables + 0 " [lints] tables, not one"
      else if (inherit != 1) print "[lints] does not say `workspace = true`"
    }
  ' "$1"
}

# The .rs files git lists in <root>'s working tree, tracked or untracked and
# not ignored, NUL-separated and relative to <root>. A tracked file deleted
# from the working tree is left out; one the index holds more than once, as
# a merge conflict leaves it, is listed once.
rs_files() {
  local f
  git -C "$1" ls-files -z --cached --others --exclude-standard --deduplicate |
    while IFS= read -r -d '' f; do
      case "$f" in
        *.rs) if [ -f "$1/$f" ]; then printf '%s\0' "$f"; fi ;;
      esac
    done
}

# count <root> <file>...: prints "<lints> <bare>" over the files, whose paths
# are relative to <root>.
count() {
  local root="$1"
  shift
  if [ "$#" -eq 0 ]; then
    echo "0 0"
  else
    (cd "$root" && perl -e "$COUNTER" "$@")
  fi
}

# compare <row> <lints> <bare> <baseline> [update]: prints the failure and
# returns 1 when a count is over the row's baseline, or when a count or a
# baseline number is not one run of digits: a failed count is empty, and a
# row listed twice gives two lines.
compare() {
  local base_lints base_bare v
  base_lints="$(awk -v m="$1" '$1 == m { print $2 }' "$4" 2>/dev/null)"
  base_bare="$(awk -v m="$1" '$1 == m { print $3 }' "$4" 2>/dev/null)"
  base_lints="${base_lints:-0}"
  base_bare="${base_bare:-0}"
  for v in "$2" "$3" "$base_lints" "$base_bare"; do
    case "$v" in
      '' | *[!0-9]*)
        echo "❌ $1: a count or its baseline row is not one number each"
        return 1
        ;;
    esac
  done
  if [ "$3" -gt "$base_bare" ]; then
    echo "❌ $1: $3 lints allowed without a reason, baseline $base_bare"
    return 1
  fi
  if [ -z "${5:-}" ] && [ "$2" -gt "$base_lints" ]; then
    echo "❌ $1: $2 lints allowed, baseline $base_lints"
    return 1
  fi
}

# check <root> <baseline> [update]: prints findings, returns 1 on any failure.
check() {
  local root="$1" baseline="$2" update="${3:-}" failed=0 scanned=0
  local member problem lints bare f
  local new_baseline="" mems=() all=() files=() outside=()

  while IFS= read -r member; do mems+=("$member"); done < <(members "$root")
  while IFS= read -r -d '' f; do all+=("$f"); done < <(rs_files "$root")

  for member in ${mems[@]+"${mems[@]}"}; do
    if [ ! -f "$root/$member/Cargo.toml" ]; then
      echo "❌ $member: no Cargo.toml"
      failed=1
      continue
    fi
    problem="$(lints_problem "$root/$member/Cargo.toml")"
    if [ -n "$problem" ]; then
      echo "❌ $member/Cargo.toml $problem"
      failed=1
    fi

    files=()
    for f in ${all[@]+"${all[@]}"}; do
      case "$f" in "$member"/*) files+=("$f") ;; esac
    done
    read -r lints bare <<<"$(count "$root" ${files[@]+"${files[@]}"})"
    scanned=$((scanned + ${#files[@]}))
    compare "$member" "$lints" "$bare" "$baseline" "$update" || failed=1
    new_baseline+="$member $lints $bare"$'\n'
  done

  for f in ${all[@]+"${all[@]}"}; do
    for member in ${mems[@]+"${mems[@]}"}; do
      case "$f" in "$member"/*) continue 2 ;; esac
    done
    outside+=("$f")
  done
  read -r lints bare <<<"$(count "$root" ${outside[@]+"${outside[@]}"})"
  scanned=$((scanned + ${#outside[@]}))
  if ! compare "$OUTSIDE" "$lints" "$bare" "$baseline" "$update"; then
    printf '   counted in %s\n' ${outside[@]+"${outside[@]}"}
    failed=1
  fi
  new_baseline+="$OUTSIDE $lints $bare"$'\n'

  if [ "${#mems[@]}" -eq 0 ] || [ "$scanned" -eq 0 ]; then
    echo "❌ found ${#mems[@]} members and $scanned .rs files: nothing was checked"
    return 1
  fi
  while read -r member _; do
    case "$member" in ''|'#'* | "$OUTSIDE") continue ;; esac
    if ! members "$root" | grep -qxF "$member"; then
      echo "❌ baseline names $member, which is not a workspace member"
      failed=1
    fi
  done < <(cat "$baseline" 2>/dev/null)

  if [ -n "$update" ] && [ "$failed" -eq 0 ]; then
    {
      echo "# <member> <lints allowed> <of those, without a reason>"
      echo "# Written by scripts/check_lint_inheritance.sh --update."
      printf '%s' "$new_baseline"
    } >"$baseline"
  fi
  return "$failed"
}

selftest() {
  local dir out expected blob m
  dir="$(mktemp -d)"
  trap 'rm -rf "$dir"' RETURN
  # Local, so the real check still reads the caller's git config.
  local XDG_CONFIG_HOME="$dir" GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
  export XDG_CONFIG_HOME GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM
  for m in good/src/target good/tests table extra none empty off lost goodies \
    target .claude/worktrees/x/crates/y/src; do
    mkdir -p "$dir/$m"
  done
  cat >"$dir/Cargo.toml" <<'TOML'
[workspace]
members = [
    "good",
    # a comment naming "not-a-member"
    "table", "extra",
    "none", "empty", "off", "lost",
]
TOML
  printf '[package]\nname = "good"\n\n[lints]\nworkspace = true\n' >"$dir/good/Cargo.toml"
  # Five manifests that fail rule 1: a [lints.*] table, a key beside
  # `workspace = true`, no [lints] table, an empty [lints] table, and
  # `workspace = false`. The member `lost` has no manifest yet.
  printf '[package]\nname = "table"\n\n[lints.clippy]\npedantic = "warn"\n' >"$dir/table/Cargo.toml"
  printf '[package]\nname = "extra"\n\n[lints]\nworkspace = true\nrust = { unsafe_code = "deny" }\n' \
    >"$dir/extra/Cargo.toml"
  printf '[package]\nname = "none"\n' >"$dir/none/Cargo.toml"
  printf '[package]\nname = "empty"\n\n[lints]\n\n[dependencies]\n' >"$dir/empty/Cargo.toml"
  printf '[package]\nname = "off"\n\n[lints]\nworkspace = false\n' >"$dir/off/Cargo.toml"
  # Ten lints allowed, five of them without a reason; every other line must
  # not be counted.
  cat >"$dir/good/src/lib.rs" <<'RS'
#![allow(dead_code)]
#[allow(clippy::alpha, clippy::alpha2)]
fn a() {}
#[allow(
    clippy::beta,
    clippy::gamma,
    reason = "a reason, with a comma"
)]
fn b() {}
#[cfg_attr(test, allow(clippy::delta), expect(clippy::delta2, reason = "known"))]
#[expect(clippy::epsilon, reason = "known")]
#[allow(clippy::mu)] // a trailing comment
#[allow(clippy::nu, reason = "see https://example.com")]
fn c() {
    // #[allow(clippy::in_a_comment)]
    let _ = x.expect("allow(clippy::in_a_call)");
}
#[error("allow(clippy::in_an_error)")]
struct E;
RS
  # One more with a reason, in a file the index holds three times.
  printf '#[allow(clippy::kappa, reason = "known")]\nfn m() {}\n' >"$dir/good/src/merge.rs"
  # A module named `target`, which is source, not build output: one without
  # a reason.
  printf '#[allow(clippy::lambda)]\nfn t() {}\n' >"$dir/good/src/target/mod.rs"
  # Outside src/: a test crate root with two lints, one of them without a
  # reason, and a build script with one that has a reason.
  printf '#![allow(clippy::zeta)]\n#[allow(clippy::eta, reason = "known")]\nfn f() {}\n' \
    >"$dir/good/tests/it.rs"
  printf '#[allow(clippy::theta, reason = "known")]\nfn main() {}\n' >"$dir/good/build.rs"
  # Outside every member, so counted in the row of its own. `goodies` shares
  # a prefix with the member `good` but is not inside it.
  printf '#[allow(clippy::iota)]\nfn s() {}\n' >"$dir/goodies/shared.rs"
  # Ignored, so counted nowhere: build output, and a worktree nested in the
  # checkout.
  printf '/target\n.claude/\n' >"$dir/.gitignore"
  printf '#[allow(clippy::in_target)]\nfn t() {}\n' >"$dir/target/gen.rs"
  printf '#[allow(clippy::in_a_worktree)]\nfn w() {}\n' \
    >"$dir/.claude/worktrees/x/crates/y/src/lib.rs"
  # Not a .rs file, so counted nowhere.
  printf '#[allow(clippy::in_markdown)]\n' >"$dir/good/README.md"
  # lib.rs is tracked; gone.rs is tracked and then deleted, so counted
  # nowhere; merge.rs is in the index at three stages, as a merge conflict
  # leaves it, and counted once. The other files are untracked.
  printf '#[allow(clippy::gone)]\nfn g() {}\n' >"$dir/good/src/gone.rs"
  git -C "$dir" init -q --template=
  git -C "$dir" add good/src/lib.rs good/src/gone.rs
  rm "$dir/good/src/gone.rs"
  blob="$(git -C "$dir" hash-object -w good/src/merge.rs)"
  printf '100644 %s %s\tgood/src/merge.rs\n' "$blob" 1 "$blob" 2 "$blob" 3 |
    git -C "$dir" update-index --index-info
  expected="good 15 7
table 0 0
extra 0 0
none 0 0
empty 0 0
off 0 0
lost 0 0
$OUTSIDE 1 1"

  printf '%s\n' "$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: the first fixture exited 0"
    return 1
  fi
  if ! grep -qF 'table/Cargo.toml declares [lints.clippy]' <<<"$out" \
    || ! grep -qF 'extra/Cargo.toml [lints] holds `rust = ' <<<"$out" \
    || ! grep -qF 'none/Cargo.toml has 0 [lints] tables, not one' <<<"$out" \
    || ! grep -qF "empty/Cargo.toml [lints] does not say \`workspace = true\`" <<<"$out" \
    || ! grep -qF "off/Cargo.toml [lints] holds \`workspace = false\`" <<<"$out" \
    || ! grep -qF 'lost: no Cargo.toml' <<<"$out" \
    || [ "$(grep -c '^❌' <<<"$out")" -ne 6 ]; then
    echo "❌ self-test: the first fixture did not fail on exactly the six bad manifests"
    printf '%s\n' "$out" | sed 's/^/     /'
    return 1
  fi

  # Each manifest failure alone: `lost` with no manifest, then with one bad
  # key.
  for m in table extra none empty off; do
    printf '[package]\nname = "%s"\n\n[lints]\nworkspace = true\n' "$m" >"$dir/$m/Cargo.toml"
  done
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a member with no manifest exited 0"
    return 1
  fi
  if ! grep -qF 'lost: no Cargo.toml' <<<"$out" \
    || [ "$(grep -c '^❌' <<<"$out")" -ne 1 ]; then
    echo "❌ self-test: a member with no manifest did not fail alone"
    printf '%s\n' "$out" | sed 's/^/     /'
    return 1
  fi
  printf '[package]\nname = "lost"\n\n[lints]\nworkspace = true\nrust = { unsafe_code = "allow" }\n' \
    >"$dir/lost/Cargo.toml"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a manifest with a key beside \`workspace = true\` exited 0"
    return 1
  fi
  if ! grep -qF 'lost/Cargo.toml [lints] holds `rust = ' <<<"$out" \
    || [ "$(grep -c '^❌' <<<"$out")" -ne 1 ]; then
    echo "❌ self-test: a manifest with a key beside \`workspace = true\` did not fail alone"
    printf '%s\n' "$out" | sed 's/^/     /'
    return 1
  fi

  printf '[package]\nname = "lost"\n\n[lints]\nworkspace = true\n' >"$dir/lost/Cargo.toml"
  if ! out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a clean fixture at its baseline failed"
    printf '%s\n' "$out" | sed 's/^/     /'
    return 1
  fi

  awk '$1 == "good" { $2 = 14 } 1' <<<"$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: an allow above the baseline exited 0"
    return 1
  fi
  if ! grep -qF 'good: 15 lints allowed, baseline 14' <<<"$out"; then
    echo "❌ self-test: an allow above the baseline passed"
    return 1
  fi
  awk '$1 == "good" { $3 = 6 } 1' <<<"$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: an allow without a reason above the baseline exited 0"
    return 1
  fi
  if ! grep -qF 'good: 7 lints allowed without a reason, baseline 6' <<<"$out"; then
    echo "❌ self-test: an allow without a reason above the baseline passed"
    return 1
  fi

  awk -v o="$OUTSIDE" '$1 == o { $3 = 0 } 1' <<<"$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: an allow outside every member exited 0"
    return 1
  fi
  if ! grep -qF "$OUTSIDE: 1 lints allowed without a reason, baseline 0" <<<"$out" \
    || ! grep -qF 'counted in goodies/shared.rs' <<<"$out"; then
    echo "❌ self-test: an allow outside every member passed, or its file went unnamed"
    return 1
  fi
  awk -v o="$OUTSIDE" '$1 == o { $2 = 0 } 1' <<<"$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: an allow outside every member over the row's first number exited 0"
    return 1
  fi
  if ! grep -qF "$OUTSIDE: 1 lints allowed, baseline 0" <<<"$out" \
    || ! grep -qF 'counted in goodies/shared.rs' <<<"$out"; then
    echo "❌ self-test: an allow outside every member over the row's first number passed, or its file went unnamed"
    return 1
  fi

  # A counter that exits 1 without printing a count, a row listed twice and a
  # count field that is not a number each fail; under --update the row
  # listed twice leaves the baseline as it was.
  printf '%s\n' "$expected" >"$dir/baseline"
  if out="$(COUNTER='exit 1' check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a counter that exits 1 without printing a count exited 0"
    return 1
  fi
  if ! grep -qF 'good: a count or its baseline row is not one number each' <<<"$out"; then
    echo "❌ self-test: a counter that exits 1 without printing a count passed"
    return 1
  fi
  { printf '%s\n' "$expected"; grep '^good ' <<<"$expected"; } >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a baseline row listed twice exited 0"
    return 1
  fi
  if ! grep -qF 'good: a count or its baseline row is not one number each' <<<"$out"; then
    echo "❌ self-test: a baseline row listed twice passed"
    return 1
  fi
  cp "$dir/baseline" "$dir/baseline.before"
  if out="$(check "$dir" "$dir/baseline" update)"; then
    echo "❌ self-test: --update over a baseline row listed twice exited 0"
    return 1
  fi
  if ! grep -qF 'good: a count or its baseline row is not one number each' <<<"$out" \
    || ! cmp -s "$dir/baseline" "$dir/baseline.before"; then
    echo "❌ self-test: --update over a baseline row listed twice passed, or rewrote the baseline"
    return 1
  fi
  for m in 2 3; do
    awk -v f="$m" '$1 == "good" { $f = "x" } 1' <<<"$expected" >"$dir/baseline"
    if out="$(check "$dir" "$dir/baseline")"; then
      echo "❌ self-test: a baseline row whose field $m is not a number exited 0"
      return 1
    fi
    if ! grep -qF 'good: a count or its baseline row is not one number each' <<<"$out"; then
      echo "❌ self-test: a baseline row whose field $m is not a number passed"
      return 1
    fi
  done

  awk '$1 == "good" { $3 = 6 } 1' <<<"$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline" update)"; then
    echo "❌ self-test: --update over an allow without a reason exited 0"
    return 1
  fi
  if ! grep -qF 'good: 7 lints allowed without a reason, baseline 6' <<<"$out" \
    || ! grep -q '^good 15 6$' "$dir/baseline"; then
    echo "❌ self-test: --update raised the count of allows without a reason"
    return 1
  fi

  awk '$1 == "good" { $2 = 14 } 1' <<<"$expected" >"$dir/baseline"
  if ! check "$dir" "$dir/baseline" update >/dev/null \
    || ! grep -q '^good 15 7$' "$dir/baseline"; then
    echo "❌ self-test: --update did not raise the count of allows"
    return 1
  fi

  # Every count exactly, whichever way it is wrong.
  awk '{ print $1, 99, 99 }' <<<"$expected" >"$dir/baseline"
  if ! check "$dir" "$dir/baseline" update >/dev/null \
    || [ "$(grep -v '^#' "$dir/baseline")" != "$expected" ]; then
    echo "❌ self-test: the counts are not the fixture's; expected:"
    printf '%s\n' "$expected" | sed 's/^/     /'
    echo "   counted:"
    grep -v '^#' "$dir/baseline" | sed 's/^/     /'
    return 1
  fi

  printf 'gone 0 0\n' >>"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a baseline row for no member exited 0"
    return 1
  fi
  if ! grep -qF 'baseline names gone, which is not a workspace member' <<<"$out"; then
    echo "❌ self-test: a baseline row for no member passed"
    return 1
  fi

  # A tree in which git lists no .rs file checks nothing, and fails.
  printf '*\n' >"$dir/.gitignore"
  rm "$dir/.git/index"
  printf '%s\n' "$expected" >"$dir/baseline"
  if out="$(check "$dir" "$dir/baseline")"; then
    echo "❌ self-test: a tree with no .rs file listed exited 0"
    return 1
  fi
  if ! grep -qF 'found 7 members and 0 .rs files: nothing was checked' <<<"$out"; then
    echo "❌ self-test: a tree with no .rs file listed passed"
    return 1
  fi

  echo "✓ self-test: five bad manifests and a missing one flagged together, then the missing one and a bad one each alone; 15 lints and 7 without a reason counted in src/, tests/ and build.rs, 1 outside every member, none in ignored, deleted or non-.rs files, and one file in the index three times counted once; growth in either number of either row, a stale row, an empty listing, a counter that exits 1 without printing a count, a row listed twice and a count field that is not a number refused; every failing fixture exits non-zero; --update raises the first number, never the second, and leaves a baseline with a row listed twice as it was"
}

case "${1:-}" in
  --self-test)
    selftest
    exit
    ;;
  --update | "") ;;
  *)
    echo "usage: $0 [--update | --self-test]" >&2
    exit 2
    ;;
esac

if ! selftest; then
  echo "The check cannot verify its own detection, so its verdict means nothing."
  exit 1
fi

if [ ! -f "$BASELINE" ]; then
  echo "❌ missing baseline: $BASELINE"
  exit 1
fi

if [ "${1:-}" = "--update" ]; then
  if check "$ROOT_DIR" "$BASELINE" update; then
    echo "✅ baseline updated: $BASELINE"
    exit 0
  fi
  echo "❌ baseline not updated"
  exit 1
fi

if ! check "$ROOT_DIR" "$BASELINE"; then
  cat <<'MSG'

The ❌ lines above say what failed. If a row allows more than its
baseline, fix the lint rather than allowing it. If an allow is the right
answer, give it `reason = "…"` and raise the row's first number with
./scripts/check_lint_inheritance.sh --update, in the same change.
MSG
  exit 1
fi
echo "✅ every member inherits the workspace lints, and no row allows more than its baseline"
