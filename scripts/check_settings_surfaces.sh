#!/usr/bin/env bash
# Fail if a `Settings` field cannot be reached from any surface a person has.
#
# `tool_call_repair` sat in `Settings` for months with no CLI flag, no
# TypeScript field and no GUI control. It was defined, mirrored into
# `SettingsUpdate`, carried on the wire by `AppSettings`, read by the proxy —
# and settable from nowhere. The CLI's settings listing even *printed* it,
# because that display is derived from serde, so the CLI advertised a setting
# it could not write.
#
# Nothing catches this. Every layer compiles: the field exists, the DTO round
# trips, and `handlers/config/settings/mod.rs` simply passes `None` for it
# forever. The failure is an absence, and absences do not fail type checks.
#
# ── What counts as reachable ─────────────────────────────────────────────────
#
# A field is reachable if it is settable from the CLI, or from the GUI:
#
#   CLI — a key `gglib config set` writes, which means the field's
#         kebab-case name is a row of `KEYS`, in
#         `config_commands/setting_key.rs`, that takes a value (not
#         `Takes::Whole` or `Takes::Elsewhere`, which `set` refuses).
#   GUI — the camelCase name appears in `src/`, which is where the TypeScript
#         mirror and the settings place both live.
#
# One is enough. Several settings are deliberately CLI-only (a scripted
# install has no modal) and a few are deliberately GUI-only (`close_to_tray`
# means nothing to a terminal). What is not acceptable is neither.
#
# ── Exemptions ───────────────────────────────────────────────────────────────
#
# Listed below with a reason each. An exemption is a claim that the field is
# written by something other than a person, and it should be rare.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SETTINGS_RS="crates/gglib-core/src/settings.rs"
CLI_KEYS="crates/gglib-cli/src/config_commands/setting_key.rs"

GUI_DIR="src"

if [ ! -f "$CLI_KEYS" ]; then
  echo -e "${RED}❌ ${CLI_KEYS} does not exist${NC}"
  echo
  echo "The CLI half of this guard reads that file. When the path is wrong the"
  echo "grep below fails per field, and because it sits inside an \`if\` it does"
  echo "not trip \`set -e\` — every field then passes on the GUI check alone."
  echo "That is exactly how this guard ran half-blind after 333682f3 moved"
  echo "\`config_commands.rs\` into a directory."
  exit 1
fi

if [ ! -d "$GUI_DIR" ]; then
  echo -e "${RED}❌ ${GUI_DIR}/ does not exist${NC}"
  echo
  echo "The GUI half reads that tree, and had the same silent-failure shape as"
  echo "the CLI half: a missing directory made grep fail, \`2>/dev/null\` hid the"
  echo "error, the \`if\` kept \`set -e\` out of it, and every field passed on the"
  echo "CLI check alone. Checked here so the redirect is no longer needed."
  exit 1
fi

# Fields nothing settable is expected for, and why.
declare -A EXEMPT=(
  [setup_completed]="written by setup's Open gglib when it finishes, not chosen"
  [inference_defaults]="edited under Settings › Models and in the model inspector, not one value"
  [inference_profiles]="edited through \`gglib config profile\` and the profile editor"
  [remote_pairing]="the ticket and key \`gglib remote join\` receives when a pairing code is redeemed, not chosen"
  [remote_enabled]="the switch \`gglib remote enable\`/\`disable\` set; a settings field that could claim a machine is reachable with nothing bound would be a lie"
  [remote_serve]="the flags \`gglib remote enable\` was given, recorded so a restart arms the tunnel the same way, not configured"
  [remote_devices]="the devices \`gglib remote invite\` issued a key to, retired by forgetting one; the keys themselves are not in settings at all"
)

echo "=== Checking every Settings field has a surface ==="
echo

# The `Settings` struct's own fields: `pub name: Type,` between `pub struct
# Settings {` and its closing brace. `SettingsUpdate` further down the file has
# the same field names, so the range matters.
fields=$(
  awk '
    /^pub struct Settings \{/ { inside = 1; next }
    inside && /^\}/           { exit }
    inside && /^    pub [a-z_]+:/ {
      line = $0
      sub(/^    pub /, "", line)
      sub(/:.*/, "", line)
      print line
    }
  ' "$SETTINGS_RS"
)

if [ -z "$fields" ]; then
  echo -e "${RED}❌ no Settings fields found in ${SETTINGS_RS}${NC}"
  echo
  echo "The struct was renamed or reshaped, so this guard is checking nothing."
  echo "A guard that finds nothing reports what a guard that found no problems"
  echo "reports — fix the parser rather than trusting this exit code."
  exit 1
fi

to_camel() {
  echo "$1" | awk -F_ '{
    out = $1
    for (i = 2; i <= NF; i++) out = out toupper(substr($i, 1, 1)) substr($i, 2)
    print out
  }'
}

# Every row of `KEYS`, as `name Takes-variant`, one per line. The table is
# joined onto one line first, so an entry rustfmt breaks across lines reads
# the same as one it leaves on one.
key_rows=$(
  awk '/^pub\(crate\) const KEYS/ { inside = 1; next } inside && /^\];/ { exit } inside' "$CLI_KEYS" |
    tr '\n' ' ' |
    grep -oE 'key\([[:space:]]*"[a-z0-9-]+",[[:space:]]*Takes::[A-Za-z]+' |
    sed -E 's/key\([[:space:]]*"([a-z0-9-]+)",[[:space:]]*Takes::([A-Za-z]+)/\1 \2/'
)

# The keys `config set` writes: every row but those that take no value.
settable_keys=$(printf '%s\n' "$key_rows" | awk '$2 != "Whole" && $2 != "Elsewhere" { print $1 }')

# CLI detection, factored into a function so the self-test below exercises the
# same code path the real scan uses.
cli_has_field() {
  printf '%s\n' "$settable_keys" | grep -qx "${1//_/-}"
}

# GUI detection, factored out for the same reason. No `2>/dev/null`: the only
# error it ever hid was a missing `$GUI_DIR`, which is now checked up front, and
# hiding anything else is how this half went quiet in the first place.
gui_has_field() {
  grep -rqE "\b$(to_camel "$1")\b" "$GUI_DIR" --include='*.ts' --include='*.tsx'
}

# Neither detector may match a name that is not a field. A pattern loose enough
# to match everything reports every field as reachable and is exactly as useless
# as one that matches nothing.
if cli_has_field "definitely_not_a_settings_field"; then
  echo -e "${RED}❌ self-test: the CLI detector matches a field that does not exist${NC}"
  echo
  echo "The pattern is too loose, so every field will look CLI-reachable."
  exit 1
fi

if gui_has_field "definitely_not_a_settings_field"; then
  echo -e "${RED}❌ self-test: the GUI detector matches a field that does not exist${NC}"
  echo
  echo "The pattern is too loose, so every field will look GUI-reachable."
  exit 1
fi

# Every `key(` the table holds, counted loosely. Where the count and the rows
# parsed above disagree, the parser has gone partially deaf: a row it cannot
# read is a key this guard silently stops checking. A count that only fails
# at zero catches the parser dying; this catches the same bug arriving more
# quietly.
declared_keys=$(
  awk '/^pub\(crate\) const KEYS/ { inside = 1; next } inside && /^\];/ { exit } inside' "$CLI_KEYS" |
    grep -oE 'key\(' | wc -l | tr -d ' '
)
parsed_keys=$(printf '%s\n' "$key_rows" | grep -c . || true)

if [ "$declared_keys" -eq 0 ]; then
  echo -e "${RED}❌ no keys found in KEYS${NC}"
  echo
  echo "The table was renamed or reshaped, so the scan below compares against"
  echo "nothing. Fix the parser rather than trusting this exit code."
  exit 1
fi

if [ "$declared_keys" -ne "$parsed_keys" ]; then
  echo -e "${RED}❌ KEYS has ${declared_keys} rows and the parser read ${parsed_keys}${NC}"
  echo
  echo "Each row must read \`key(\"name\", Takes::Variant…\`; a row in another"
  echo "shape is a key this guard cannot see. Fix the parser or the row."
  exit 1
fi

unreachable=()
checked=0
cli_reachable=0
gui_reachable=0

for field in $fields; do
  checked=$((checked + 1))

  if [[ -v "EXEMPT[$field]" ]]; then
    echo -e "  ${YELLOW}—${NC} ${field} (exempt: ${EXEMPT[$field]})"
    continue
  fi

  in_cli=false
  in_gui=false

  # CLI: the field is a key `config set` writes.
  if cli_has_field "$field"; then
    in_cli=true
    cli_reachable=$((cli_reachable + 1))
  fi

  # GUI: the camelCase name appears anywhere the frontend can act on it.
  if gui_has_field "$field"; then
    in_gui=true
    gui_reachable=$((gui_reachable + 1))
  fi

  if $in_cli || $in_gui; then
    surfaces=""
    $in_cli && surfaces="CLI"
    $in_gui && surfaces="${surfaces:+$surfaces, }GUI"
    echo -e "  ${GREEN}✓${NC} ${field} (${surfaces})"
  else
    echo -e "  ${RED}✗${NC} ${field}"
    unreachable+=("$field")
  fi
done

echo

# A surface on which *nothing* was found is not a real state — it is a detector
# that has stopped working. Reporting "all clear" on that is what this guard did
# between 333682f3 and the fix, and it is indistinguishable from a healthy run
# unless the counts are asserted.
if [ "$cli_reachable" -eq 0 ]; then
  echo -e "${RED}❌ no Settings field was found on the CLI surface${NC}"
  echo
  echo "Either every key was deleted from ${CLI_KEYS}, or the parser no longer"
  echo "reads it. The second is far more likely — check \`key_rows\` against the"
  echo "current shape of \`KEYS\`."
  exit 1
fi

if [ "$gui_reachable" -eq 0 ]; then
  echo -e "${RED}❌ no Settings field was found on the GUI surface${NC}"
  echo
  echo "Either every setting was removed from ${GUI_DIR}/, or the detector no"
  echo "longer matches it. Check the pattern in \`gui_has_field\` and that"
  echo "\`to_camel\` still produces the names the TypeScript side uses."
  exit 1
fi

if [ ${#unreachable[@]} -gt 0 ]; then
  echo -e "${RED}❌ ${#unreachable[@]} setting(s) nobody can change${NC}"
  echo
  for field in "${unreachable[@]}"; do
    echo "  - ${field}"
  done
  cat <<'MSG'

Each of these is stored, plumbed and read, and settable from no surface a
person has. Pick one:

  1. Give it a CLI key: add a row to `KEYS` in
     `crates/gglib-cli/src/config_commands/setting_key.rs`. That is the one
     place: a test beside it fails for a field left out of it.
  2. Give it a GUI control: add the camelCase field to `src/types/index.ts`
     and a control in the settings place (`src/places/settings/`).
  3. Delete the setting, and whatever reads it.
  4. Exempt it in this script, with a reason — but only if something other
     than a person is meant to write it.
MSG
  exit 1
fi

echo -e "${GREEN}✅ every Settings field is reachable${NC} (${checked} checked, ${cli_reachable} on the CLI, ${gui_reachable} on the GUI)"

# ── Second guard: `null` must clear, for every field ─────────────────────────
#
# Reachability is not enough. `UpdateSettingsRequest` promises in its own doc
# comment that every field is `Option<Option<T>>` with
# `serde_with::rust::double_option`, which is what makes an explicit `null`
# ("clear it") distinguishable from an omitted key ("leave it alone").
#
# `tool_call_repair` was declared bare. Serde collapsed its `null` into the
# same `None` an omitted key produces, so the setting could be written but
# never cleared — the same field that motivated the reachability half above,
# broken a second time for a different reason.
#
# The damage is at the HTTP surface, not in today's settings modal: that modal
# reads the tri-state down to a `bool` and only ever sends `true` or `false`,
# so it never asks for a clear. Any client that does — the API, a future
# reset control — is the one that gets a silent no-op.
#
# Nothing typechecks this. The struct compiles, the DTO round trips, and the
# setting is simply unclearable.

UPDATE_REQ_RS="crates/gglib-app-services/src/types_settings.rs"

if [ ! -f "$UPDATE_REQ_RS" ]; then
  echo -e "${RED}❌ ${UPDATE_REQ_RS} does not exist${NC}"
  echo
  echo "The double-option guard below reads that file. A missing path would"
  echo "make it enumerate nothing and pass — checked here instead."
  exit 1
fi

echo
echo "=== Checking every UpdateSettingsRequest field can be cleared ==="
echo

# Both halves of the attribute matter. `double_option` alone still leaves the
# key *required* — serde has no default to fall back on — so a PATCH that omits
# it fails outright. The guard demands `default` too.
#
# Written as one function so the self-test below exercises the same code the
# real scan does. Emits one bad field name per line, then `#TOTAL n`.
scan_double_options() {
  awk '
    /^pub struct UpdateSettingsRequest \{/ { inside = 1; next }
    inside && /^\}/                        { exit }
    !inside                                { next }

    # A comment can *mention* the attribute without carrying it. Skipping
    # these is what stops a doc line reading "uses double_option" from
    # vouching for a field that has none.
    /^[[:space:]]*\/\//                    { next }

    # The declaration is matched anywhere on the line, not anchored to its
    # start: an attribute sharing the line would otherwise hide the field from
    # the checker *and* from the counter. `[a-z0-9_]+`, not `[a-z_]+`, for the
    # same double reason — a name like `http2_enabled` was invisible to both.
    #
    # Only the text *before* the declaration joins the attribute buffer. The
    # declaration itself must never vouch for the field: `default_model_id`
    # and `default_context_size` both contain "default" in their names, and
    # would otherwise satisfy the check on their own spelling.
    # `pub(crate)` and friends are matched too. Requiring whitespace straight
    # after `pub` reproduced the original bug exactly: the field was invisible
    # to the checker *and* the counter, and its unmatched line then kept the
    # previous field'"'"'s attribute alive to vouch for the next one.
    {
      if (match($0, /pub(\([a-z:]+\))?[[:space:]]+[a-z0-9_]+[[:space:]]*:/)) {
        attr = attr substr($0, 1, RSTART - 1) "\n"

        name = substr($0, RSTART, RLENGTH)
        sub(/^pub(\([a-z:]+\))?[[:space:]]+/, "", name)
        sub(/[[:space:]]*:$/, "", name)

        # `default` must be an attribute token, not any occurrence of the
        # word. A `rename = "defaultContextSize"` would otherwise satisfy it
        # from inside a string literal — and this struct already renames and
        # aliases, beside a `Settings` full of `default_*` names.
        #
        # `double_option` is checked on the raw text, because it legitimately
        # lives inside the `with = "..."` string.
        bare = attr
        gsub(/"[^"]*"/, "", bare)

        total++
        if (attr !~ /double_option/ || bare !~ /(^|[^a-z_])default([^a-z_]|$)/) {
          print name
        }
        attr = ""
      } else {
        # A multi-line `#[serde(...)]` block reaches its field this way.
        attr = attr $0 "\n"
      }
    }

    END { print "#TOTAL " total + 0 }
  ' "$1"
}

# ── Self-test: four shapes that used to pass, and one that must still pass ──
selftest_dir="$(mktemp -d)"
trap 'rm -rf "$selftest_dir"' EXIT

cat > "$selftest_dir/bad.rs" <<'SELFTEST'
pub struct UpdateSettingsRequest {
    /// Cleared the same way as the double_option fields above.
    pub doc_comment_lies: Option<Option<bool>>,
    // clearing works like the other double_option settings
    pub line_comment_lies: Option<Option<bool>>,
    /// Uses serde default plus double_option, like every field here.
    pub comment_names_both_words: Option<Option<bool>>,
    pub http2_enabled: Option<Option<bool>>,
    #[serde(default)] pub attr_on_field_line: Option<Option<bool>>,
    #[serde(with = "serde_with::rust::double_option")]
    pub missing_default: Option<Option<bool>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub(crate) restricted_visibility: Option<Option<bool>>,
    pub vouched_by_the_field_above: Option<Option<bool>>,
    #[serde(
        rename = "defaultContextSize",
        with = "serde_with::rust::double_option"
    )]
    pub default_only_in_a_string: Option<Option<u32>>,
}
SELFTEST

cat > "$selftest_dir/good.rs" <<'SELFTEST'
pub struct UpdateSettingsRequest {
    #[serde(default, with = "serde_with::rust::double_option")]
    pub plain: Option<Option<bool>>,
    /// A doc comment, then the real thing.
    #[serde(default, with = "serde_with::rust::double_option")]
    pub documented: Option<Option<bool>>,
    #[serde(
        default,
        alias = "toolCallFloor",
        with = "serde_with::rust::double_option"
    )]
    pub multi_line: Option<Option<bool>>,
    #[serde(default, with = "serde_with::rust::double_option")]
    pub http2_enabled: Option<Option<bool>>,
}
SELFTEST

# Asserted as an exact set, not a count. `restricted_visibility` carries the
# right attribute and must NOT be flagged — it is in the bad fixture only to
# create the leak that `vouched_by_the_field_above` depends on, which is the
# whole point of that pair.
bad_expected='attr_on_field_line
comment_names_both_words
default_only_in_a_string
doc_comment_lies
http2_enabled
line_comment_lies
missing_default
vouched_by_the_field_above'

bad_flagged=$(scan_double_options "$selftest_dir/bad.rs" | grep -v '^#TOTAL ' | sort)
bad_total=$(scan_double_options "$selftest_dir/bad.rs" | sed -n 's/^#TOTAL //p')
good_flagged=$(scan_double_options "$selftest_dir/good.rs" | grep -v '^#TOTAL ' || true)
good_total=$(scan_double_options "$selftest_dir/good.rs" | sed -n 's/^#TOTAL //p')

if [ "$bad_flagged" != "$bad_expected" ] || [ "$bad_total" -ne 9 ]; then
  echo -e "${RED}❌ self-test: the double-option scanner does not flag what it must${NC}"
  echo
  echo "Saw ${bad_total} of 9 fields. Expected to flag:"
  printf '%s\n' "$bad_expected" | sed 's/^/  + /'
  echo "Actually flagged:"
  printf '%s\n' "$bad_flagged" | sed 's/^/  - /'
  cat <<'MSG'

Every case in that fixture is a shape that once passed this guard: a doc
comment naming the attribute, a line comment naming it, a comment naming both
words, a field name containing a digit, an attribute on the field's own line,
`double_option` without `default`, a `pub(crate)` field going uncounted and
leaking its attribute to the next field, and `default` occurring only inside a
`rename` string.

A field that goes *uncounted* is the worst of them: it escapes the checker and
the "found nothing, so something is wrong" sentinel at the same time, so the
guard reports success having quietly checked fewer fields.

Fix the scanner, not the fixture.
MSG
  exit 1
fi

if [ -n "$good_flagged" ] || [ "$good_total" -ne 4 ]; then
  echo -e "${RED}❌ self-test: the double-option scanner flagged a correct field${NC}"
  echo
  echo "Expected 0 flagged of 4 seen; saw ${good_total} and flagged:"
  printf '%s\n' "$good_flagged" | sed 's/^/  - /'
  exit 1
fi

scan_output=$(scan_double_options "$UPDATE_REQ_RS")
bare_fields=$(printf '%s\n' "$scan_output" | grep -v '^#TOTAL ' || true)
total_update_fields=$(printf '%s\n' "$scan_output" | sed -n 's/^#TOTAL //p')

if [ "$total_update_fields" -eq 0 ]; then
  echo -e "${RED}❌ no UpdateSettingsRequest fields found in ${UPDATE_REQ_RS}${NC}"
  echo
  echo "The struct was renamed or reshaped, so this guard is checking nothing."
  exit 1
fi

if [ -n "$bare_fields" ]; then
  echo -e "${RED}❌ setting(s) an explicit null cannot clear${NC}"
  echo
  for field in $bare_fields; do
    echo "  - ${field}"
  done
  cat <<'MSG'

Each of these is `Option<Option<T>>` without
`#[serde(default, with = "serde_with::rust::double_option")]`, so serde reads
an explicit `null` as "leave unchanged" — identical to omitting the key. The
setting can be written but never cleared, and any client that asks for a
clear gets a silent no-op rather than an error.

Add the attribute to the field in `crates/gglib-app-services/src/types.rs`.
MSG
  exit 1
fi

echo -e "${GREEN}✅ every UpdateSettingsRequest field honours an explicit null${NC} (${total_update_fields} checked)"
