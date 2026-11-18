#!/usr/bin/env bash
# Fail if a workflow or an action installs a system package anywhere but
# through the apt-packages action.
#
# A step that runs `apt-get install` downloads from the runner's first Ubuntu
# mirror, on every run, with nothing to turn to when that mirror is slow, and
# a mirror can serve tens of kB/s for hours, which outlasts a job's timeout
# (#2102). It is one line, and it works on the day it is written.
#
# .github/actions/apt-packages installs from GitHub's cache when an entry
# holds the packages, and otherwise gives each mirror on the runner's own
# list a time budget before the next. A step that installs on its own has
# neither. `playwright install --with-deps` is such a step: it is apt
# underneath.
#
# ── What is checked ──────────────────────────────────────────────────────────
#
# Every workflow, and every local action, for a line that is not a comment and
# runs `apt install`, `apt-get install` or `playwright install --with-deps`.
#
# ── What is allowed ──────────────────────────────────────────────────────────
#
# Nothing under .github/. The action itself holds no such line: its work is in
# scripts/apt_packages.py, which this does not read.
#
# scripts/release_smoke.py and scripts/verify_linux_repos.sh install gglib's
# own packages with apt, as a user would, because that install is what they
# test. They are scripts, so they are out of this check's reach, and that is
# on purpose.
#
# A job that needs a system package asks for it: `apt-packages:` on the
# rust-prelude action, or the apt-packages action itself.

set -euo pipefail

cd "$(dirname "$0")/.."

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

PATTERN='(^|[^[:alnum:]_-])apt(-get)?([[:space:]]+-[^[:space:]]+)*[[:space:]]+install([[:space:]]|$)|playwright[[:space:]]+install[^#]*--with-deps'

files=()
while IFS= read -r file; do
    files+=("$file")
done < <(find .github/workflows .github/actions -type f \( -name '*.yml' -o -name '*.yaml' \) | sort)

found=0
for file in "${files[@]}"; do
    # A line whose first character is `#` is a comment in YAML and in a
    # shell script alike.
    while IFS= read -r hit; do
        [ -n "$hit" ] || continue
        echo -e "  ${RED}✗${NC} ${file#./}:$hit"
        found=1
    done < <(grep -nE "$PATTERN" "$file" | grep -vE '^[0-9]+:[[:space:]]*#' || true)
done

if [ "$found" -ne 0 ]; then
    echo ""
    echo -e "${RED}✗${NC} A workflow or an action installs a system package on its own."
    echo "  Ask for it through the apt-packages action instead: \`apt-packages:\` on"
    echo "  rust-prelude, or \`uses: ./.github/actions/apt-packages\` with \`packages:\`."
    echo "  The header of this script says why."
    exit 1
fi

echo -e "${GREEN}✓${NC} no workflow or action installs a system package outside apt-packages (${#files[@]} files)"
