#!/usr/bin/env bash
#
# Validate .github/workflows/*.yml for duplicate mapping keys.
# Also checks that every module and coverage path named in badges.yml still
# resolves under crates/ — see the second section below for why it lives here.
#
# GitHub rejects a workflow file containing a duplicate key outright: the run is
# marked "failed because of a workflow file issue" and NO jobs start. That makes
# it invisible to CI itself — a broken ci.yml cannot run the job that would have
# caught it — so this check has to happen before the push.
#
# Most YAML parsers won't help: the spec says duplicate keys are invalid, but
# Psych's safe_load and PyYAML both silently keep the last one. Walking the raw
# node tree is what makes them visible.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v ruby >/dev/null 2>&1; then
  echo "⚠ ruby not found — skipping workflow YAML validation"
  exit 0
fi

ruby -ryaml -e '
bad = 0
Dir.glob(".github/workflows/*.yml").sort.each do |file|
  begin
    doc = YAML.parse(File.read(file))
  rescue Psych::SyntaxError => e
    puts "  \e[31m✗\e[0m #{file}: #{e.message}"
    bad += 1
    next
  end
  next unless doc

  walk = lambda do |node, path|
    if node.is_a?(Psych::Nodes::Mapping)
      seen = {}
      node.children.each_slice(2) do |k, v|
        key = (k.respond_to?(:value) ? k.value : k.to_s)
        if seen[key]
          puts "  \e[31m✗\e[0m #{file}:#{k.start_line + 1} duplicate key \x27#{key}\x27 in #{path.empty? ? "(root)" : path} (first seen at line #{seen[key]})"
          bad += 1
        end
        seen[key] = k.start_line + 1
        walk.call(v, "#{path}/#{key}")
      end
    elsif node.respond_to?(:children) && node.children
      node.children.each { |c| walk.call(c, path) }
    end
  end
  walk.call(doc, "")
end

count = Dir.glob(".github/workflows/*.yml").length
if bad.zero?
  puts "\e[32m✓\e[0m no duplicate keys in #{count} workflow file(s)"
else
  puts "\e[31m#{bad} problem(s) found — GitHub would reject these and run no jobs at all\e[0m"
  exit 1
end
'

# ---------------------------------------------------------------------------
# badges.yml names module paths that must still exist in the tree
# ---------------------------------------------------------------------------
#
# The two jobs that extract test and coverage badges check out the `badges`
# branch, not the source, so they cannot see `crates/` and cannot notice when a
# module they name has moved. Nor does it fail
# when one has: a missed `extract_module_tests` writes a lightgrey "0" and a
# missed `extract_module_cov` writes nothing at all. Six paths had rotted that
# way while their coverage badges kept serving numbers from the last run that
# did resolve.
#
# This runs where the source IS present, on every PR, so the next move fails here
# instead of quietly freezing a badge.
#
# The crate is derived from the artifact each *_LCOV / *_TEST variable looks for
# (gglib-cli-lcov.info -> crates/gglib-cli), so no path mapping has to be kept in
# step by hand — the thing this check exists to prevent.
echo ""
echo "Checking badges.yml module paths resolve..."
BADGES=".github/workflows/badges.yml"
BADGE_PATH_FAILURES=0


while IFS='|' read -r var relpath; do
    [ -z "$var" ] && continue
    case "$relpath" in *'$'*) continue ;; esac
    crate=$(grep -oE "^ *${var}=\"\\\$\(find artifacts -name '[^']+'" "$BADGES" | head -1 | grep -oE "gglib-[a-z-]+" | head -1 | sed -E 's/-(lcov|tests)$//' || true)
    if [ -z "$crate" ]; then
        echo -e "\033[1;33m⚠\033[0m could not derive a crate for $var — skipping its paths"
        continue
    fi
    if [ ! -d "crates/$crate/$relpath" ]; then
        echo -e "\033[0;31m✗\033[0m badges.yml names crates/$crate/$relpath, which does not exist"
        BADGE_PATH_FAILURES=$((BADGE_PATH_FAILURES + 1))
    fi
done < <(grep -oE 'extract_module_cov "\$[A-Z_]+" "[^"]+"' "$BADGES" \
         | sed -E 's/extract_module_cov "\$([A-Z_]+)" "([^"]+)"/\1|\2/')

# Module names reach extract_module_tests two ways: as a literal, or as $MODULE
# from a `for MODULE in a b c` list just above the call. Expand the lists so both
# are checked; a bare "$MODULE" on its own would otherwise be treated as a path.
while IFS='|' read -r var modpath; do
    [ -z "$var" ] && continue
    crate=$(grep -oE "^ *${var}=\"\\\$\(find artifacts -name '[^']+'" "$BADGES" | head -1 | grep -oE "gglib-[a-z-]+" | head -1 | sed -E 's/-(lcov|tests)$//' || true)
    if [ -z "$crate" ]; then
        echo -e "\033[1;33m⚠\033[0m could not derive a crate for $var — skipping its paths"
        continue
    fi
    dir="crates/$crate/src/$(printf '%s' "$modpath" | sed 's|::|/|g')"
    if [ ! -d "$dir" ] && [ ! -f "$dir.rs" ]; then
        echo -e "\033[0;31m✗\033[0m badges.yml names module $modpath in $crate, which is neither $dir nor $dir.rs"
        BADGE_PATH_FAILURES=$((BADGE_PATH_FAILURES + 1))
    fi
done < <(awk '
    /for MODULE in / {
        line = $0
        sub(/.*for MODULE in /, "", line)
        sub(/;.*/, "", line)
        list = line
        next
    }
    /extract_module_tests "\$[A-Z_]+"/ {
        match($0, /extract_module_tests "\$[A-Z_]+"/)
        v = substr($0, RSTART, RLENGTH)
        gsub(/extract_module_tests "\$|"/, "", v)
        rest = $0
        sub(/.*extract_module_tests "\$[A-Z_]+" "/, "", rest)
        sub(/".*/, "", rest)
        if (rest == "$MODULE") {
            n = split(list, parts, /[ \t]+/)
            for (i = 1; i <= n; i++) if (parts[i] != "") print v "|" parts[i]
        } else if (rest !~ /\$/) {
            print v "|" rest
        }
    }
' "$BADGES")

if [ "$BADGE_PATH_FAILURES" -gt 0 ]; then
    echo -e "\033[0;31m$BADGE_PATH_FAILURES badge path(s) point at modules that have moved or gone\033[0m"
    exit 1
fi
echo -e "\033[0;32m✓\033[0m every badges.yml module and coverage path resolves"
