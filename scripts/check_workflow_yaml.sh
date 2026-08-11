#!/usr/bin/env bash
#
# Validate .github/workflows/*.yml for duplicate mapping keys.
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
