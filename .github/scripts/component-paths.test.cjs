// Tests of component-paths.cjs. `make enforce` runs them.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { COMPONENT_PATHS, componentOfPath } = require('./component-paths.cjs');

test('a path under a crate is that crate\'s component', () => {
  assert.equal(componentOfPath('crates/gglib-cli/src/main.rs'), 'component: cli');
  assert.equal(componentOfPath('crates/gglib-download/src/lib.rs'), 'component: downloads');
  assert.equal(componentOfPath('crates/gglib-app-services/src/servers.rs'), 'component: gui');
});

test('the top-level folders have a component', () => {
  assert.equal(componentOfPath('src-tauri/src/main.rs'), 'component: tauri');
  assert.equal(componentOfPath('src/components/Button.tsx'), 'component: frontend');
  assert.equal(componentOfPath('web_ui/index.html'), 'component: frontend');
  assert.equal(componentOfPath('.github/workflows/ci.yml'), 'component: ci');
  assert.equal(componentOfPath('scripts/promote.py'), 'component: ci');
});

test('a prefix is matched at the start of the path only', () => {
  // `src/` is the frontend's folder at the root, not every crate's `src/`.
  assert.equal(componentOfPath('crates/gglib-cli/src/x.rs'), 'component: cli');
  assert.equal(componentOfPath('docs/src/x.md'), null);
  assert.equal(componentOfPath('xsrc/x.ts'), null);
});

test('a path outside the table has no component', () => {
  assert.equal(componentOfPath('crates/gglib-agent/src/lib.rs'), null);
  assert.equal(componentOfPath('README.md'), null);
  assert.equal(componentOfPath(''), null);
});

test('every row is a folder prefix and a component: label, and no two rows overlap', () => {
  for (const [prefix, label] of COMPONENT_PATHS) {
    assert.match(prefix, /^[^/].*\/$/);
    assert.match(label, /^component: [a-z]+$/);
  }
  for (const [a] of COMPONENT_PATHS) {
    for (const [b] of COMPONENT_PATHS) {
      if (a !== b) assert.ok(!a.startsWith(b), `${a} is under ${b}`);
    }
  }
});
