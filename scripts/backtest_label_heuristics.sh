#!/bin/bash
# backtest_label_heuristics.sh - Validate the auto-labelling heuristics in
# .github/workflows/label-check.yml against real merged-PR history.
#
# Answers two questions empirically, rather than by guessing:
#   1. Can size: be predicted from diff line count?           (no — see below)
#   2. Can component: be auto-applied from every touched path? (only when unambiguous)
#
# This is what justified the workflow's actual design:
#   - size: is never auto-applied, only suggested with historical base rates
#   - component: auto-applies ONLY when exactly one component is touched;
#     otherwise it's suggested, same as size:
#
# Read-only. Makes no label changes. Not run in CI — re-run by hand if the
# label taxonomy or crate layout changes enough that these numbers might have
# moved, to confirm the workflow's comments are still telling the truth.
#
# Usage: ./scripts/backtest_label_heuristics.sh
# Requires: gh (authenticated), node, jq

set -euo pipefail

REPO="mmogr/gglib"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

GENERATED_RE='(^|/)(Cargo\.lock|package-lock\.json|pnpm-lock\.yaml|yarn\.lock)$|\.snap$|(^|/)(dist|target|node_modules)/'

echo "=============================================================="
echo " 1. Can size: be predicted from diff line count?"
echo "=============================================================="

gh pr list --repo "$REPO" --state merged --limit 400 \
  --json number,labels,additions,deletions \
  --jq '.[] | select([.labels[].name | select(startswith("size:"))] | length == 1)
        | "\(.number)\t\([.labels[].name | select(startswith("size:"))] | first)\t\(.additions + .deletions)"' \
  > "$WORKDIR/size_pairs.tsv"

TOTAL=$(wc -l < "$WORKDIR/size_pairs.tsv" | tr -d ' ')
echo "Merged PRs with exactly one size: label: $TOTAL"
echo "Fetching per-file diff stats (excluding generated files)..." >&2

printf 'pr\thuman\tfiltered_lines\n' > "$WORKDIR/size_results.tsv"
i=0
while IFS=$'\t' read -r num human _raw; do
  i=$((i + 1))
  [ $((i % 40)) -eq 0 ] && echo "  ...$i/$TOTAL" >&2
  # grep -Ev exits 1 when nothing survives the filter — including the trivial case
  # of a PR with zero changed files (e.g. #66). Under `set -eo pipefail` that exit
  # code propagates and kills the whole script despite awk having already computed
  # the right answer (0). Neutralize it with a brace group; the pipeline's real
  # output (however empty) still flows through to awk unaffected.
  filtered=$(gh api "repos/$REPO/pulls/$num/files" --paginate \
      --jq '.[] | "\(.additions + .deletions)\t\(.filename)"' 2>/dev/null \
    | { grep -Ev "$GENERATED_RE" || true; } \
    | awk -F'\t' '{s += $1} END {print s + 0}')
  printf '%s\t%s\t%s\n' "$num" "$human" "${filtered:-0}" >> "$WORKDIR/size_results.tsv"
done < "$WORKDIR/size_pairs.tsv"

node --input-type=module -e "
import { readFileSync } from 'node:fs';
const ORDER = ['size: xs', 'size: s', 'size: m', 'size: l', 'size: xl'];
const rows = readFileSync('$WORKDIR/size_results.tsv', 'utf8').trim().split('\n').slice(1)
  .map(l => { const [pr, human, lines] = l.split('\t'); return { pr, human, lines: +lines }; });

function bucket(n) {
  if (n < 120) return 'size: xs';
  if (n < 350) return 'size: s';
  if (n < 750) return 'size: m';
  if (n < 2000) return 'size: l';
  return 'size: xl';
}
const hit = rows.filter(r => bucket(r.lines) === r.human).length;
console.log(\`\nPlanned thresholds (120/350/750/2000): \${hit}/\${rows.length} exact (\${Math.round(100*hit/rows.length)}%)\`);

// Grid search for the best achievable line-only thresholds, for comparison.
const cand = [20,30,40,50,60,80,100,120,150,180,220,260,300,350,400,450,500,600,700,800,900,1000,1200,1400,1700,2000,2400,3000];
function combos(arr, k) {
  if (k === 0) return [[]];
  if (arr.length < k) return [];
  const [h, ...t] = arr;
  return [...combos(t, k-1).map(c => [h, ...c]), ...combos(t, k)];
}
function score(cuts) {
  let h = 0;
  for (const r of rows) {
    let b = 0;
    while (b < 4 && r.lines >= cuts[b]) b++;
    if (ORDER[b] === r.human) h++;
  }
  return h;
}
let best = [0, null];
for (const c of combos(cand, 4)) { const s = score(c); if (s > best[0]) best = [s, c]; }
console.log(\`Best possible line-only thresholds (grid search): \${best[0]}/\${rows.length} (\${Math.round(100*best[0]/rows.length)}%)  cuts=\${JSON.stringify(best[1])}\`);
console.log('Conclusion: size: is denominated in hours, not diff volume — no threshold gets close to reliable.');
console.log('This is why the workflow suggests a historical band instead of applying a computed label.');
"

echo ""
echo "=============================================================="
echo " 2. Can component: be auto-applied from every touched path?"
echo "=============================================================="

gh pr list --repo "$REPO" --state merged --limit 150 --json number,labels \
  --jq '.[] | select([.labels[].name | select(startswith("component:"))] | length > 0)
        | "\(.number)\t\([.labels[].name | select(startswith("component:"))] | sort | join("|"))"' \
  > "$WORKDIR/comp_pairs.tsv"

CTOTAL=$(wc -l < "$WORKDIR/comp_pairs.tsv" | tr -d ' ')
echo "Merged PRs with human component: labels: $CTOTAL"
echo "Fetching changed-file paths..." >&2

: > "$WORKDIR/comp_results.tsv"
i=0
while IFS=$'\t' read -r num human; do
  i=$((i + 1))
  [ $((i % 20)) -eq 0 ] && echo "  ...$i/$CTOTAL" >&2
  derived=$(gh api "repos/$REPO/pulls/$num/files" --paginate --jq '.[].filename' 2>/dev/null \
    | node --input-type=module -e "
        import { readFileSync } from 'node:fs';
        const paths = readFileSync(0, 'utf8').trim().split('\n').filter(Boolean);
        const MAP = [
          [/^crates\/gglib-cli\//, 'cli'], [/^crates\/gglib-core\//, 'core'],
          [/^crates\/gglib-axum\//, 'axum'], [/^crates\/gglib-runtime\//, 'runtime'],
          [/^crates\/gglib-proxy\//, 'proxy'], [/^crates\/gglib-db\//, 'db'],
          [/^crates\/gglib-download\//, 'downloads'], [/^crates\/gglib-hf\//, 'hf'],
          [/^crates\/gglib-mcp\//, 'mcp'], [/^crates\/gglib-sse\//, 'sse'],
          [/^crates\/gglib-app-services\//, 'gui'], [/^crates\/gglib-tauri\//, 'tauri'],
          [/^src-tauri\//, 'tauri'], [/^src\//, 'frontend'], [/^web_ui\//, 'frontend'],
          [/^\.github\//, 'ci'], [/^scripts\//, 'ci'],
        ];
        const s = new Set();
        for (const p of paths) for (const [re, c] of MAP) if (re.test(p)) s.add('component: ' + c);
        process.stdout.write([...s].sort().join('|'));
      ")
  printf '%s\t%s\t%s\n' "$num" "$human" "$derived" >> "$WORKDIR/comp_results.tsv"
done < "$WORKDIR/comp_pairs.tsv"

node --input-type=module -e "
import { readFileSync } from 'node:fs';
const rows = readFileSync('$WORKDIR/comp_results.tsv', 'utf8').trim().split('\n')
  .map(l => { const [pr, human, derived] = l.split('\t'); return { pr, human, derived: derived || '' }; });

let n1 = 0, n1hit = 0, nM = 0, nMexact = 0, nMover = 0, n0 = 0;
for (const r of rows) {
  const H = new Set(r.human.split('|').filter(Boolean));
  const D = new Set(r.derived.split('|').filter(Boolean));
  if (D.size === 0) { n0++; continue; }
  if (D.size === 1) {
    n1++;
    if (H.size === 1 && [...H][0] === [...D][0]) n1hit++;
  } else {
    nM++;
    const exact = H.size === D.size && [...H].every(x => D.has(x));
    if (exact) nMexact++;
    if (H.size && [...H].every(x => D.has(x)) && D.size > H.size) nMover++;
  }
}
console.log(\`\nSingle-component-touch PRs (n=\${n1}): matched human's label \${n1hit}/\${n1} (\${Math.round(100*n1hit/n1)}%)\`);
console.log(\`Multi-component-touch PRs  (n=\${nM}): exact match \${nMexact}/\${nM} (\${Math.round(100*nMexact/nM)}%), over-labelled \${nMover}/\${nM} (\${Math.round(100*nMover/nM)}%)\`);
console.log(\`Unmapped-path-only PRs     (n=\${n0})\`);
console.log('Conclusion: auto-apply is reliable only when derivation is unambiguous (exactly one component).');
console.log('This is why the workflow auto-applies on n=1 and asks a human otherwise.');
"

echo ""
echo "Done. Raw data left nowhere (workdir is temp) — re-run to regenerate."
