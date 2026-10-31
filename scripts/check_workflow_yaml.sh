#!/usr/bin/env bash
#
# Validate every YAML file under .github/ for duplicate mapping keys: the
# workflows, and configuration such as the issue forms.
# Also checks that the two workflows that move the version, release-please.yml
# and promote.yml, re-lock the lockfiles rather than re-resolve them.
#
# GitHub rejects a workflow file containing a duplicate key outright: the run is
# marked "failed because of a workflow file issue" and NO jobs start. That makes
# it invisible to CI itself — a broken ci.yml cannot run the job that would have
# caught it — so this check has to happen before the push.
#
# Most YAML parsers won't help: the spec says duplicate keys are invalid, but
# Psych's safe_load and PyYAML both silently keep the last one. Walking the raw
# node tree is what makes them visible.
#
# The files outside .github/workflows/ need it as much. Nothing reliably checks
# them before they take effect, and a parser that keeps the last duplicate
# would, for example, drop the first of two `ignore:` lists in a Dependabot
# config without a word.
set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v ruby >/dev/null 2>&1; then
  echo "⚠ ruby not found — skipping workflow YAML validation"
  exit 0
fi

ruby -ryaml -e '
bad = 0
files = Dir.glob(".github/**/*.{yml,yaml}").sort
files.each do |file|
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

count = files.length
if bad.zero?
  puts "\e[32m✓\e[0m no duplicate keys in #{count} YAML file(s) under .github"
else
  puts "\e[31m#{bad} problem(s) found — GitHub runs no jobs from a workflow like this, and nothing reliably reports one in any other file under .github\e[0m"
  exit 1
end
'

# ---------------------------------------------------------------------------
# The version workflows must re-lock the workspace, not re-resolve the graph
# ---------------------------------------------------------------------------
#
# `cargo generate-lockfile` throws Cargo.lock away and re-resolves every
# dependency against whatever the registry holds that minute, so a PR titled
# only as a version change carries third-party upgrades nobody reviewed.
# #975, "Bump version to 0.16.0", shipped exactly that: 36 third-party crates
# moved under that title, aws-lc-rs and tokio-rustls among them.
# `cargo update --workspace` re-locks only the workspace's own members, which
# is the whole job of a version change.
#
# Nothing else can catch a regression here. Both workflows run outside pull
# requests (release-please.yml on pushes to main, promote.yml when
# dispatched), their output is a bot PR whose diff is expected to touch the
# lockfiles, and a reviewer scanning a release PR has no reason to read 250
# lockfile lines. The one-word edit that reintroduces it is invisible until
# it ships.

echo ""
echo "Checking the version workflows re-lock rather than re-resolve..."
for workflow in .github/workflows/release-please.yml .github/workflows/promote.yml; do
    if [ ! -f "$workflow" ]; then
        echo -e "\033[0;31m✗\033[0m $workflow is missing. Both version workflows re-lock the"
        echo "  lockfiles; if one was renamed, rename it in this check too."
        exit 1
    fi
    if grep -qE '^[^#]*cargo[[:space:]]+generate-lockfile' "$workflow"; then
        echo -e "\033[0;31m✗\033[0m $workflow runs 'cargo generate-lockfile', which re-resolves every"
        echo "  dependency from scratch. A version change must move only the workspace's own"
        echo "  crates: use 'cargo update --workspace'. See the note in the workflow."
        grep -nE '^[^#]*cargo[[:space:]]+generate-lockfile' "$workflow" | sed 's/^/    /'
        exit 1
    fi
    if ! grep -qE '^[^#]*cargo[[:space:]]+update[[:space:]]+--workspace' "$workflow"; then
        echo -e "\033[0;31m✗\033[0m $workflow no longer runs 'cargo update --workspace', so nothing"
        echo "  moves the workspace crate versions in Cargo.lock and 'cargo metadata --locked'"
        echo "  will fail the release PR or the promotion."
        exit 1
    fi
    echo -e "\033[0;32m✓\033[0m $workflow re-locks the workspace only"
done
