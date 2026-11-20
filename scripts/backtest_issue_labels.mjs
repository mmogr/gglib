#!/usr/bin/env node
// backtest_issue_labels.mjs - Measure what .github/scripts/issue-words.cjs reads
// from an issue's title and description against the labels the repository's
// issues ended with.
//
// For each rule, and for each category as a whole, it counts the issues where
// the rule spoke and sorts them four ways:
//   same   the issue ended with exactly these labels in the category
//   part   it ended with these and more
//   wrong  it ended without one of these, with another in its place
//   none   it ended with no label in the category at all
// An "applies" rule is one issue-labels.yml acts on when an issue is opened; a
// "suggests" rule only lists its labels in the triage comment, and its counts
// say what applying every one of them would have done. A rule that is wrong
// more than rarely belongs under "suggests".
//
// The issues are this repository's own, so a rule tuned on these numbers is
// tuned in-sample: read them as what the rule did here, not what it will do.
//
// Read-only. Makes no label changes. Not run in CI: re-run by hand before
// changing a rule in issue-words.cjs or a row of component-paths.cjs.
//
// Usage: ./scripts/backtest_issue_labels.mjs [issues.json]
//   issues.json is the output of
//   `gh issue list --state all --limit 2000 --json number,title,body,labels`;
//   without it the script runs that itself.
// Requires: gh (authenticated), node

import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';

import issueWords from '../.github/scripts/issue-words.cjs';

const REPO = 'mmogr/gglib';
const gh = (...args) => JSON.parse(execFileSync('gh', args, { encoding: 'utf8', maxBuffer: 1 << 28 }));

const issues = process.argv[2]
  ? JSON.parse(readFileSync(process.argv[2], 'utf8'))
  : gh('issue', 'list', '--repo', REPO, '--state', 'all', '--limit', '2000', '--json', 'number,title,body,labels');
const repoLabels = gh('label', 'list', '--repo', REPO, '--limit', '300', '--json', 'name').map(l => l.name);

const OUTCOMES = ['same', 'part', 'wrong', 'none'];

function outcome(derived, final) {
  if (!final.length) return 'none';
  if (!derived.every(l => final.includes(l))) return 'wrong';
  return derived.length === final.length ? 'same' : 'part';
}

// rule name -> outcome -> issue numbers
const rules = new Map();
const categories = new Map();
function count(table, name, result, number) {
  if (!table.has(name)) table.set(name, Object.fromEntries([...OUTCOMES, 'silent'].map(o => [o, []])));
  table.get(name)[result].push(number);
}

for (const issue of issues) {
  const derived = issueWords.deriveLabels({ title: issue.title, body: issue.body, repoLabels });
  for (const [prefix, found] of Object.entries(derived)) {
    // `type: epic` says an issue tracks others; it is not one of the kinds of
    // work a title names.
    const final = issue.labels.map(l => l.name)
      .filter(n => n.startsWith(`${prefix} `) && n !== 'type: epic');
    const labels = found.apply.length ? found.apply : found.suggest;
    count(categories, prefix, found.apply.length ? outcome(found.apply, final) : 'silent', issue.number);
    if (!labels.length) continue;
    // One row per type word; a scope's own name is left out of the rule's.
    const from = prefix === 'type:' ? found.from : found.from.replace(/ `[^`]*`/, '');
    const verb = found.apply.length ? 'applies' : 'suggests';
    count(rules, `${prefix} ${verb}, from ${from}`, outcome(labels, final), issue.number);
  }
}

const pct = (n, of) => (of ? `${Math.round(100 * n / of)}%` : '-');
const numbers = (list) => (list.length && list.length <= 12 ? `  (#${list.join(', #')})` : '');

console.log(`${issues.length} issues of ${REPO}\n`);
console.log('What issue-labels.yml would have applied, by category:\n');
for (const [prefix, r] of categories) {
  const spoke = OUTCOMES.reduce((n, o) => n + r[o].length, 0);
  console.log(`  ${prefix.padEnd(11)} applied to ${spoke} (${pct(spoke, issues.length)}), silent on ${r.silent.length}`);
  for (const o of OUTCOMES) console.log(`      ${o.padEnd(6)}${String(r[o].length).padStart(4)}  ${pct(r[o].length, spoke).padStart(4)}${o === 'wrong' ? numbers(r[o]) : ''}`);
}

console.log('\nRule by rule:\n');
for (const [name, r] of [...rules].sort(([a], [b]) => a.localeCompare(b))) {
  const spoke = OUTCOMES.reduce((n, o) => n + r[o].length, 0);
  console.log(`  ${name}: ${spoke}`);
  console.log(`      ${OUTCOMES.map(o => `${o} ${r[o].length} (${pct(r[o].length, spoke)})`).join(', ')}${numbers(r.wrong)}`);
}
