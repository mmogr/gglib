#!/usr/bin/env bash
# Fail if a `Settings` field cannot be reached from any surface a person has.
#
# `tool_call_repair` sat in `Settings` for months with no CLI flag, no
# TypeScript field and no GUI control. It was defined, mirrored into
# `SettingsUpdate`, carried on the wire by `AppSettings`, read by the proxy —
# and settable from nowhere. `gglib config settings show` even *printed* it,
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
#   CLI — a `--kebab-case` flag on `gglib config settings set`, which means the
#         field appears in `SettingsSetArgs`, in
#         `config_commands/settings_args.rs`.
#   GUI — the camelCase name appears in `src/`, which is where the TypeScript
#         mirror and the settings modal both live.
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
CLI_SET_ARGS="crates/gglib-cli/src/config_commands/settings_args.rs"

GUI_DIR="src"

if [ ! -f "$CLI_SET_ARGS" ]; then
  echo -e "${RED}❌ ${CLI_SET_ARGS} does not exist${NC}"
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
  [setup_completed]="written by the setup wizard when it finishes, not chosen"
  [default_model_id]="set by \`gglib model default\`, not by config settings"
  [inference_defaults]="edited through \`gglib config inference\` and the model inspector"
  [inference_profiles]="edited through \`gglib config profile\` and the profile editor"
  [title_generation_prompt]="edited through the chat UI's title settings"
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

# CLI detection, factored into a function so the self-test below exercises the
# same code path the real scan uses. The `pub ` is optional because the fields
# moved from a clap variant's inline list (no `pub`) to a named `Args` struct
# (`pub`) — a pattern that assumes either one silently matches nothing.
cli_has_field() {
  grep -qE "^[[:space:]]+(pub )?$1: Option<" "$CLI_SET_ARGS"
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

# Every field `SettingsSetArgs` declares, by name — matched loosely, on any
# field of any type. Deliberately a different pattern from `cli_has_field`'s:
# where the two disagree about a field `Settings` also declares, the strict one
# has drifted and the cross-check after the scan says so. A count that only
# fails at zero catches the detector dying; this catches it going partially
# deaf, which is the same bug arriving more quietly.
declared_flags=$(
  awk '
    /^pub struct SettingsSetArgs \{/ { inside = 1; next }
    inside && /^\}/                  { exit }
    inside && /^[[:space:]]+(pub )?[a-z_0-9]+[[:space:]]*:/ {
      line = $0
      sub(/^[[:space:]]+/, "", line)
      sub(/^pub /, "", line)
      sub(/[[:space:]]*:.*/, "", line)
      print line
    }
  ' "$CLI_SET_ARGS"
)

if [ -z "$declared_flags" ]; then
  echo -e "${RED}❌ no flags found in SettingsSetArgs${NC}"
  echo
  echo "The struct was renamed or reshaped, so the cross-check below compares"
  echo "against nothing. Fix the parser rather than trusting this exit code."
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

  # CLI: the field name appears in `SettingsSetArgs`.
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
  echo "Either every flag was deleted from ${CLI_SET_ARGS}, or the detector no"
  echo "longer matches it. The second is far more likely — check the pattern in"
  echo "\`cli_has_field\` against the current shape of \`SettingsSetArgs\`."
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

# Partial drift: a flag `SettingsSetArgs` declares, that `Settings` also
# declares and does not exempt, must have been found by `cli_has_field`. The
# enumeration above and the detector use different patterns on purpose, so a
# detector that has drifted out of step with the struct's shape fails here
# instead of quietly under-reporting a subset.
missed=()
for flag in $declared_flags; do
  printf '%s\n' $fields | grep -qx "$flag" || continue
  [[ -v "EXEMPT[$flag]" ]] && continue
  cli_has_field "$flag" || missed+=("$flag")
done

if [ ${#missed[@]} -gt 0 ]; then
  echo -e "${RED}❌ the CLI detector missed ${#missed[@]} flag(s) that SettingsSetArgs declares${NC}"
  echo
  for flag in "${missed[@]}"; do
    echo "  - ${flag}"
  done
  echo
  echo "These are declared in ${CLI_SET_ARGS} and are non-exempt \`Settings\`"
  echo "fields, so \`cli_has_field\` should match them. It does not, which means"
  echo "its pattern no longer fits the struct — the same failure as a wrong"
  echo "path, only partial. Fix the pattern; do not exempt your way out."
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

  1. Give it a CLI flag: add it to `SettingsSetArgs` in
     `crates/gglib-cli/src/config_commands/settings_args.rs` and map it in
     `crates/gglib-cli/src/handlers/config/settings/mod.rs` — all four places
     (the destructure, the `changed` set, the `SettingsUpdate`, the
     pre-validate merge).
  2. Give it a GUI control: add the camelCase field to `src/types/index.ts`
     and a control in the settings modal.
  3. Delete the setting, and whatever reads it.
  4. Exempt it in this script, with a reason — but only if something other
     than a person is meant to write it.
MSG
  exit 1
fi

echo -e "${GREEN}✅ every Settings field is reachable${NC} (${checked} checked, ${cli_reachable} on the CLI, ${gui_reachable} on the GUI)"
