// Which `component:` label a path in the repository belongs to. The one table
// for it: label-check.yml reads a pull request's changed files through it,
// issue-words.cjs the paths an issue's description names, and
// scripts/backtest_label_heuristics.sh and scripts/backtest_issue_labels.mjs
// measure both against history.
//
// A folder with no row has no component label: gglib-agent, gglib-bootstrap,
// gglib-gguf, gglib-build-info, gglib-engine-llama, gglib-integration-tests,
// gglib-test-http and gglib-test-proc among the crates.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

// A folder, from the repository's root and ending in `/`, and its label. No
// row is under another, so a path matches one row at most.
const COMPONENT_PATHS = [
  ['crates/gglib-cli/',          'component: cli'],
  ['crates/gglib-core/',         'component: core'],
  ['crates/gglib-axum/',         'component: axum'],
  ['crates/gglib-runtime/',      'component: runtime'],
  ['crates/gglib-proxy/',        'component: proxy'],
  ['crates/gglib-db/',           'component: db'],
  ['crates/gglib-download/',     'component: downloads'],
  ['crates/gglib-hf/',           'component: hf'],
  ['crates/gglib-mcp/',          'component: mcp'],
  ['crates/gglib-broadcast/',    'component: broadcast'],
  ['crates/gglib-app-services/', 'component: gui'],
  ['src-tauri/',                 'component: tauri'],
  ['src/',                       'component: frontend'],
  ['web_ui/',                    'component: frontend'],
  ['.github/',                   'component: ci'],
  ['scripts/',                   'component: ci'],
];

// The label of the folder `path` is under, or null when it is under none.
// `path` is relative to the repository's root, as the API gives a changed file.
function componentOfPath(path) {
  const row = COMPONENT_PATHS.find(([prefix]) => path.startsWith(prefix));
  return row ? row[1] : null;
}

module.exports = { COMPONENT_PATHS, componentOfPath };
