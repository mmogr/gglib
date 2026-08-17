#!/usr/bin/env bash
#
# Two annotations stand between generated TypeScript and a new class of lie.
#
# ts-rs reads Rust types, not serde's behaviour, so on two points it produces
# something plausible and wrong unless told otherwise:
#
#   1. `i64`/`u64` become `bigint`. `JSON.parse` never yields a `bigint`, so a
#      field typed that way is a value no client can ever hold — the generated
#      binding would be *more* wrong than the hand-written mirror it replaces.
#
#   2. `skip_serializing_if = "Option::is_none"` does not imply optional. The
#      wire omits the key; ts-rs still emits `field: T | null`, which says the
#      key is always present and may be null. Both halves of that are false.
#
# Neither fails to compile and neither shows up in a diff review of 150 fields,
# which is why this exists. The rule is a biconditional, checked both ways:
#
#     skip_serializing_if = "Option::is_none"   ⟺   #[ts(optional)]
#
# The reverse direction matters as much as the forward one: `ts(optional)` on a
# field serde always emits claims the key can be absent when it never is.
#
# With one exception, and it is a real one rather than a carve-out. The rule
# above reads a type Rust *serializes*, where `skip_serializing_if` is what
# omits a key. A request body is the other direction — Rust deserializes it, and
# what the client may leave out is governed by `#[serde(default)]`. The
# three-state `Option<Option<T>>` fields carry `default` with
# `serde_with::rust::double_option` and no `skip_serializing_if`, and for those
# `field?: T | null` is exactly right: absent means leave unchanged, `null`
# means clear, a value means set. So `double_option` also satisfies the
# may-be-omitted side. It is named specifically rather than accepting bare
# `default`, because `default` on a response field (`SamplingExplanationDto`
# has one) governs only how a *missing* field deserializes and does not stop
# serde emitting the key.
#
# Run with `--self-test` to check the checker against known-bad shapes.

set -euo pipefail

cd "$(dirname "$0")/.."

RED=$'\033[0;31m'
GREEN=$'\033[0;32m'
CYAN=$'\033[0;36m'
NC=$'\033[0m'

fail=0

# Types that derive `TS` *and* hand-write their serde. ts-rs reads fields, not
# impls, so for these the emitted shape has to be steered by hand and the
# optionality rule below cannot apply — what is omitted is decided by the impl.
# Each has been read against its impl and annotated to match; a new one has
# not been, which is what `check_manual_serde` is for.
REVIEWED_MANUAL_SERDE="ModelCapabilities AssistantContent"

# ── Half 1: nothing in the generated output may be a bigint ──────────────────
#
# Checked against the emitted TypeScript rather than the Rust, because that is
# where the answer actually is: any numeric override that was missed, wherever
# it lives and whatever shape it took, surfaces here as the literal `bigint`.
check_generated() {
  local dir="${1:-src/types/generated}"

  [ -d "$dir" ] || return 0

  local hits
  hits=$(grep -rn '\bbigint\b' "$dir" 2>/dev/null || true)

  if [ -n "$hits" ]; then
    echo "${RED}❌${NC} generated bindings contain \`bigint\`:"
    echo "$hits" | sed 's/^/     /'
    echo
    echo "   An i64/u64 reached TypeScript unannotated. \`JSON.parse\` never"
    echo "   produces a bigint, so this type describes a value no client can"
    echo "   hold. Add \`#[cfg_attr(feature = \"ts-bindings\", ts(type = \"number\"))]\`"
    echo "   to the field (or \`\"number | null\"\` when the Option is always sent)."
    return 1
  fi
}

# ── Half 2: skip_serializing_if ⟺ ts(optional), inside TS-deriving types ─────
#
# Field attributes are accumulated per field rather than matched line-by-line:
# an attribute sits on its own line, several may stack, and rustfmt is free to
# reorder them. String literals are blanked first so a `skip_serializing_if`
# named inside one — or the word `optional` inside a `rename` — cannot stand in
# for the real attribute. Comment lines are dropped for the same reason.
scan_rust() {
  awk -v manual=" ${REVIEWED_MANUAL_SERDE:-} " '
    # Blank string literals and strip line comments before anything reads the
    # line, so neither can impersonate an attribute.
    #
    # `double_option` is the exception that has to be read *before* blanking,
    # because it lives inside the string: `with = "serde_with::rust::
    # double_option"`. Matched as that whole attribute shape rather than as a
    # bare word, so prose mentioning it in a comment cannot stand in for it —
    # which is the same defeat the blanking exists to prevent, arriving from
    # the opposite side.
    {
      raw = $0
      line = raw
      gsub(/"[^"]*"/, "\"\"", line)
      sub(/\/\/.*$/, "", line)
    }

    # A TS derive marks the *next* declaration, not the current line.
    line ~ /cfg_attr\(feature = "", derive\(ts_rs::TS\)/ { pending = 1 }

    # Entering a declaration. Only the ones the derive marked are our business
    # — and not the ones that hand-write their serde, where what is omitted is
    # decided by an impl this scanner cannot read. Those are checked by
    # `check_manual_serde` instead, which requires a human to have read the
    # impl and annotated the type to match it.
    line ~ /^pub(\([a-z:]+\))?[[:space:]]+(struct|enum)[[:space:]]/ {
      inside = pending
      pending = 0
      attr = ""
      decl = line
      sub(/^pub(\([a-z:]+\))?[[:space:]]+(struct|enum)[[:space:]]+/, "", decl)
      sub(/[^A-Za-z0-9_].*$/, "", decl)
      if (index(manual, " " decl " ") > 0) inside = 0
      next
    }

    line ~ /^\}/ { inside = 0; attr = ""; has_double = 0; next }
    !inside { next }

    # A field line: decide, then reset. Anything else is another attribute
    # line, so keep collecting.
    {
      # `pub` is optional: a private field crosses the wire exactly as a public
      # one does, and `ErrorBody` — the body of every failing route — has
      # nothing but private fields. An earlier draft required `pub` and was
      # structurally blind to it. Anchored at the line start so this cannot
      # match a `name:` appearing mid-expression; enum variants are CamelCase
      # and so fall outside `[a-z0-9_]+` either way.
      if (match(line, /^[[:space:]]*(pub(\([a-z:]+\))?[[:space:]]+)?[a-z0-9_]+[[:space:]]*:/)) {
        name = substr(line, RSTART, RLENGTH)
        sub(/^[[:space:]]*/, "", name)
        sub(/^pub(\([a-z:]+\))?[[:space:]]+/, "", name)
        sub(/[[:space:]]*:$/, "", name)

        # `double_option` is the request-body form of "may be omitted" — see
        # the header. `optional` is matched as a bare word so the spellings
        # `ts(optional)`, `ts(type = …, optional)` and `ts(as = …, optional =
        # nullable)` all count, with `optional = false` excluded because it
        # means the opposite.
        skipped  = (attr ~ /skip_serializing_if/ || has_double)
        optional = (attr ~ /optional/ && attr !~ /optional[[:space:]]*=[[:space:]]*false/)

        if (skipped && !optional) {
          printf "%s:%d: %s is skipped when None but not #[ts(optional)]\n", FILENAME, FNR, name
        }
        if (optional && !skipped) {
          printf "%s:%d: %s is #[ts(optional)] but serde always sends it\n", FILENAME, FNR, name
        }
        attr = ""
        has_double = 0
      } else {
        attr = attr line "\n"
        if (raw ~ /with[[:space:]]*=[[:space:]]*"[^"]*double_option"/) has_double = 1
      }
    }
  ' "$@"
}

check_sources() {
  local files
  files=$(grep -rl 'derive(ts_rs::TS)' crates/ --include='*.rs' 2>/dev/null || true)

  [ -n "$files" ] || return 0

  local problems
  # shellcheck disable=SC2086
  problems=$(scan_rust $files)

  if [ -n "$problems" ]; then
    echo "${RED}❌${NC} optionality does not match serde:"
    echo "$problems" | sed 's/^/     /'
    echo
    echo "   \`skip_serializing_if\` omits the key, so TypeScript must say"
    echo "   \`field?: T\`, not \`field: T | null\`. The two are different claims"
    echo "   and only one is true. Pair every such field with"
    echo "   \`#[cfg_attr(feature = \"ts-bindings\", ts(optional))]\` — and only"
    echo "   those, since the attribute lies in the other direction on a field"
    echo "   serde always sends."
    return 1
  fi
}

# ── Self-test ────────────────────────────────────────────────────────────────
#
# A guard nobody has watched fail is a guard nobody knows works. Each fixture
# below is a shape that defeated an earlier draft of this scanner.
self_test() {
  local tmp
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' RETURN

  cat > "$tmp/subject.rs" <<'FIXTURE'
// A type with no TS derive at all: none of its fields are this guard's
// business, however they are attributed.
#[derive(Serialize)]
pub struct NotExported {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignored_untagged: Option<u32>,
}

#[derive(Serialize)]
#[cfg_attr(feature = "ts-bindings", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "camelCase")]
pub struct Subject {
    // Correct: both halves present.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub good_optional: Option<u32>,

    // Correct: neither half. Always sent, may be null.
    pub good_nullable: Option<String>,

    // Correct: the attributes in the other order.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub good_reordered: Option<u32>,

    // Correct: combined with a type override in one attribute.
    #[cfg_attr(feature = "ts-bindings", ts(type = "number", optional))]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub good_combined: Option<u64>,

    // BAD: skipped, not marked optional.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bad_missing_optional: Option<u32>,

    // BAD: a digit in the name — invisible to a scanner whose field pattern
    // forgets [0-9], and then its attributes leak onto the next field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bad_http2_port: Option<u16>,

    // BAD: pub(crate), same invisibility-plus-leak mechanism.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) bad_crate_visible: Option<u32>,

    // BAD: marked optional but serde always sends it.
    #[cfg_attr(feature = "ts-bindings", ts(optional))]
    pub bad_optional_but_sent: Option<String>,

    // BAD: a doc comment naming skip_serializing_if does not make it so.
    /// Mentions skip_serializing_if in prose only.
    pub bad_doc_mentions_skip: Option<String>,

    // BAD: the words appear inside a string literal, not as attributes.
    #[serde(rename = "skip_serializing_if_ts_optional")]
    pub bad_string_literal: Option<String>,

    // Correct: the three-state request form. `double_option` is what may be
    // omitted here, not `skip_serializing_if`, and `optional = nullable`
    // spells `field?: T | null`.
    #[cfg_attr(
        feature = "ts-bindings",
        ts(as = "Option<ServerConfig>", optional = nullable)
    )]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub good_double_option: Option<Option<ServerConfig>>,

    // Correct: the same, spelled with a type override for a primitive inner
    // type, where no import is needed.
    #[cfg_attr(feature = "ts-bindings", ts(type = "boolean | null", optional))]
    #[serde(default, with = "serde_with::rust::double_option")]
    pub good_double_option_primitive: Option<Option<bool>>,

    // BAD: three-state on the wire, single-state in TypeScript. Absent and
    // null both become "unchanged" to a reader of this type.
    #[serde(default, with = "serde_with::rust::double_option")]
    pub bad_double_option_unmarked: Option<Option<bool>>,

    // BAD: prose naming double_option is not double_option. This one defeated
    // the first draft, which read the word instead of the attribute.
    /// Behaves a bit like double_option, but is not one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bad_doc_mentions_double_option: Option<u32>,

    // Correct: explicitly opted out, which means the opposite of optional and
    // must not be read as satisfying it.
    #[cfg_attr(feature = "ts-bindings", ts(optional = false))]
    pub good_optional_false: Option<String>,

    // BAD: a private field. It crosses the wire like any other, and requiring
    // `pub` made this whole class invisible — `ErrorBody` is one.
    #[serde(skip_serializing_if = "Option::is_none")]
    bad_private_field: Option<u32>,
}
FIXTURE

  local got expected_bad missing=0
  got=$(scan_rust "$tmp/subject.rs" || true)

  expected_bad="bad_missing_optional bad_http2_port bad_crate_visible bad_optional_but_sent \
                bad_double_option_unmarked bad_doc_mentions_double_option bad_private_field"

  for field in $expected_bad; do
    if ! echo "$got" | grep -q "\b$field\b"; then
      echo "${RED}❌${NC} self-test: $field should have been reported, was not"
      missing=1
    fi
  done

  for field in good_optional good_nullable good_reordered good_combined \
               good_double_option good_double_option_primitive good_optional_false \
               ignored_untagged bad_doc_mentions_skip bad_string_literal; do
    if echo "$got" | grep -q "\b$field\b"; then
      echo "${RED}❌${NC} self-test: $field was reported and should not be"
      echo "     (reported: $(echo "$got" | grep "\b$field\b"))"
      missing=1
    fi
  done

  # Exact count, so a scanner that reports everything cannot pass by covering
  # the four it owes.
  local count
  count=$(echo "$got" | grep -c . || true)
  if [ "$count" -ne 7 ]; then
    echo "${RED}❌${NC} self-test: expected exactly 7 findings, got $count"
    echo "$got" | sed 's/^/     /'
    missing=1
  fi

  if [ "$missing" -eq 0 ]; then
    echo "${GREEN}✅${NC} self-test: the scanner catches all seven known-bad shapes and no others"
  fi
  return "$missing"
}

if [ "${1:-}" = "--self-test" ]; then
  self_test
  exit $?
fi

# ── Half 3: every type that derives TS actually reached the output ──────────
#
# The failure this closes: a crate gains a `TS` derive but is left out of the
# Makefile's `TS_BINDING_FEATURES`. Its feature never turns on, its export test
# never runs, no file is written — and a `git diff` gate sees a clean tree,
# because there is no stale file to differ from. Nothing else notices, since
# the missing binding is only missed by code nobody has written yet.
check_completeness() {
  local dir="${1:-src/types/generated}"

  [ -d "$dir" ] || return 0

  local missing=""
  local name
  # Read the declaration that follows each derive, which is the name ts-rs
  # writes the file under.
  while IFS= read -r name; do
    [ -n "$name" ] || continue
    [ -f "$dir/$name.ts" ] || missing="$missing $name"
  done < <(
    grep -rA 4 'derive(ts_rs::TS)' crates/ --include='*.rs' 2>/dev/null |
      grep -oE '(struct|enum) [A-Z][A-Za-z0-9]*' |
      awk '{ print $2 }' |
      sort -u
  )

  if [ -n "$missing" ]; then
    echo "${RED}❌${NC} types derive TS but produced no binding:"
    for name in $missing; do
      echo "     $name — expected $dir/$name.ts"
    done
    echo
    echo "   Either the export run did not cover its crate, or the type has"
    echo "   \`derive(ts_rs::TS)\` without \`ts(export)\`, which generates the impl"
    echo "   but writes no file. Run \`make bindings\` and check both."
    return 1
  fi
}

# ── Half 4: a hand-written serde impl invalidates the derived shape ──────────
#
# ts-rs reads a type's *fields*. A manual `impl Serialize` may rename them, omit
# them conditionally, or emit something structurally different, and the derive
# cannot see any of it — so the binding is confidently wrong in a way nothing
# else catches. `AssistantContent` shipped exactly that: its impl writes
# `content` and omits it when absent, while the generated type declared a
# required `text` no payload has ever carried.
#
# The reviewed list is declared near the top, beside the other configuration.
check_manual_serde() {
  local unreviewed=""
  local name

  while IFS= read -r name; do
    [ -n "$name" ] || continue
    # Does it also derive TS? If not, its impl is nobody's business here.
    grep -rB 6 "^pub \(struct\|enum\) $name\( \|{\|$\)" crates/ --include='*.rs' 2>/dev/null |
      grep -qF 'derive(ts_rs::TS)' || continue
    case " $REVIEWED_MANUAL_SERDE " in
      *" $name "*) continue ;;
    esac
    unreviewed="$unreviewed $name"
  done < <(
    grep -rhoE "^impl( *<'[a-z]+>)? (Serialize|Deserialize<'[a-z]+>) for [A-Za-z0-9_]+" \
      crates/ --include='*.rs' 2>/dev/null |
      sed -E 's/.* for //' | sort -u
  )

  if [ -n "$unreviewed" ]; then
    echo "${RED}❌${NC} types derive TS but hand-write their serde:"
    for name in $unreviewed; do echo "     $name"; done
    echo
    echo "   ts-rs reads fields, not impls, so the generated binding describes"
    echo "   the struct rather than the wire — silently, and with authority."
    echo "   Read the impl, annotate the type to match what it really emits"
    echo "   (\`ts(rename)\`, \`ts(optional)\`, \`ts(as)\`), then add it to"
    echo "   REVIEWED_MANUAL_SERDE in this script to record that it was checked."
    return 1
  fi
}

echo "${CYAN}Checking ts-rs binding annotations${NC}"
echo "================================================"

self_test || fail=1
check_generated || fail=1
check_sources || fail=1
check_completeness || fail=1
check_manual_serde || fail=1

if [ "$fail" -ne 0 ]; then
  echo
  echo "${RED}❌ ts-rs binding checks failed.${NC}"
  exit 1
fi

echo "${GREEN}✅ every skipped field is optional, and no bigint reached TypeScript${NC}"
