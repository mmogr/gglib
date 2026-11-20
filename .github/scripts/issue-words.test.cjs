// Tests of issue-words.cjs. `make enforce` runs them.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { deriveLabels } = require('./issue-words.cjs');

const REPO_LABELS = [
  'component: axum', 'component: ci', 'component: cli', 'component: core',
  'component: frontend', 'component: gui', 'component: proxy', 'component: runtime',
  'component: tauri', 'component:build',
  'platform: linux', 'platform: macos', 'platform: windows',
  'type: bug', 'type: chore', 'type: ci', 'type: docs', 'type: feature',
  'type: perf', 'type: refactor', 'type: test',
  'priority: low', 'size: s', 'status: needs-triage',
];

const derive = (title, body = '', repoLabels = REPO_LABELS) => deriveLabels({ title, body, repoLabels });

test('a conventional title sets the type and the component', () => {
  const d = derive('fix(cli): the spinner overwrites the progress line');
  assert.deepEqual(d['type:'], { apply: ['type: bug'], suggest: [], from: '`fix` in the title' });
  assert.deepEqual(d['component:'], { apply: ['component: cli'], suggest: [], from: 'the title\'s scope `cli`' });
  assert.deepEqual(d['platform:'], { apply: [], suggest: [], from: '' });
});

test('each type word maps to the label the repository uses for it', () => {
  const type = (title) => derive(title)['type:'].apply;
  assert.deepEqual(type('feat(cli): a picture in the terminal'), ['type: feature']);
  assert.deepEqual(type('fix: a thing'), ['type: bug']);
  assert.deepEqual(type('bug(proxy): a thing'), ['type: bug']);
  assert.deepEqual(type('refactor(core): a thing'), ['type: refactor']);
  assert.deepEqual(type('chore: a thing'), ['type: chore']);
  assert.deepEqual(type('perf(runtime): a thing'), ['type: perf']);
  assert.deepEqual(type('docs: a thing'), ['type: docs']);
  assert.deepEqual(type('feat(cli)!: a breaking thing'), ['type: feature']);
});

test('a type word history contradicts is suggested and not applied', () => {
  assert.deepEqual(derive('test(runtime): a harness')['type:'],
    { apply: [], suggest: ['type: test'], from: '`test` in the title' });
  assert.deepEqual(derive('ci: the pinned toolchain')['type:'],
    { apply: [], suggest: ['type: ci'], from: '`ci` in the title' });
});

test('a title that is not conventional says nothing of type or component', () => {
  for (const title of [
    'A model load fails when the endpoint\'s port is taken',
    'Fix: rustdoc links are broken',
    'tracking: the orchestrator',
    'fix(cli) the colon is missing',
    'fix(cli):no space after the colon',
    '[Tech Debt] extract the constants',
  ]) {
    const d = derive(title);
    assert.deepEqual(d['type:'], { apply: [], suggest: [], from: '' }, title);
    assert.deepEqual(d['component:'], { apply: [], suggest: [], from: '' }, title);
  }
});

test('a scope with no label is not guessed', () => {
  const d = derive('fix(agent): the run ends early');
  assert.deepEqual(d['type:'].apply, ['type: bug']);
  assert.deepEqual(d['component:'], { apply: [], suggest: [], from: '' });
  // The crate's name is not the label's.
  assert.deepEqual(derive('fix(gglib-runtime): a thing')['component:'].apply, []);
  // `component:build` is not `component: build`.
  assert.deepEqual(derive('fix(build): a thing')['component:'], { apply: [], suggest: [], from: '' });
});

test('a scope history contradicts is a candidate, and is not applied', () => {
  assert.deepEqual(derive('fix(gui): the inspector\'s text overlaps')['component:'],
    { apply: [], suggest: ['component: gui'], from: 'the title\'s scope `gui`' });
  assert.deepEqual(derive('fix(ci): the docs workflow fails')['component:'],
    { apply: [], suggest: ['component: ci'], from: 'the title\'s scope `ci`' });
  // Nor do the description's paths settle it, for it or against it.
  assert.deepEqual(derive('fix(ci): the docs workflow fails', '.github/workflows/a.yml and .github/workflows/b.yml')['component:'],
    { apply: [], suggest: ['component: ci'], from: 'the title\'s scope `ci` and the paths the description names' });
  assert.deepEqual(derive('fix(gui): the inspector\'s text overlaps', 'src/a.tsx and src/b.tsx')['component:'],
    { apply: [], suggest: ['component: gui', 'component: frontend'], from: 'the title\'s scope `gui` and the paths the description names' });
});

test('several scopes are candidates, and none is applied', () => {
  assert.deepEqual(derive('feat(proxy,axum): a new route')['component:'],
    { apply: [], suggest: ['component: proxy', 'component: axum'], from: 'the title\'s scopes' });
  assert.deepEqual(derive('feat(runtime/axum): a new route')['component:'].suggest,
    ['component: runtime', 'component: axum']);
  // One of two scopes has a label: still not applied.
  assert.deepEqual(derive('fix(cli, agent): a thing')['component:'],
    { apply: [], suggest: ['component: cli'], from: 'the title\'s scopes' });
});

test('two paths in one component set it', () => {
  const d = derive('The dashboard shows a stale figure', [
    'The figure is computed in `crates/gglib-proxy/src/stats.rs` and cached in',
    'crates/gglib-proxy/src/cache.rs, which never drops it.',
  ].join('\n'));
  assert.deepEqual(d['component:'],
    { apply: ['component: proxy'], suggest: [], from: 'the paths the description names' });
});

test('one path is a candidate, and is not applied', () => {
  const d = derive('The dashboard shows a stale figure',
    'See crates/gglib-proxy/src/stats.rs:42, and crates/gglib-proxy/src/stats.rs again.');
  assert.deepEqual(d['component:'],
    { apply: [], suggest: ['component: proxy'], from: 'the one path the description names' });
});

test('paths in several components are candidates, and none is applied', () => {
  const d = derive('The dashboard shows a stale figure', [
    '- crates/gglib-proxy/src/stats.rs',
    '- crates/gglib-proxy/src/cache.rs',
    '- src/pages/Dashboard.tsx',
  ].join('\n'));
  assert.deepEqual(d['component:'],
    { apply: [], suggest: ['component: proxy', 'component: frontend'], from: 'the paths the description names' });
});

test('a path is read whole, from its start', () => {
  const component = (body) => derive('A thing is wrong', body)['component:'];
  // A crate's own src/ is not the frontend's.
  assert.deepEqual(component('crates/gglib-cli/src/a.rs and crates/gglib-cli/src/b.rs').apply, ['component: cli']);
  // A URL's path is not a path in the repository.
  assert.deepEqual(component('https://example.com/src/a.ts and https://example.com/src/b.ts'),
    { apply: [], suggest: [], from: '' });
  // A leading ./ is the repository's root.
  assert.deepEqual(component('./scripts/a.sh and ./scripts/b.sh').apply, ['component: ci']);
  // A folder named alone names no file in it.
  assert.deepEqual(component('everything under src/ and scripts/'), { apply: [], suggest: [], from: '' });
  // A crate with no component label.
  assert.deepEqual(component('crates/gglib-agent/src/a.rs and crates/gglib-agent/src/b.rs'),
    { apply: [], suggest: [], from: '' });
});

test('the title\'s one scope decides, whatever paths the description names', () => {
  const d = derive('fix(cli): the proxy\'s notice is printed twice',
    'crates/gglib-proxy/src/a.rs and crates/gglib-proxy/src/b.rs write it.');
  assert.deepEqual(d['component:'],
    { apply: ['component: cli'], suggest: [], from: 'the title\'s scope `cli`' });
});

test('a title whose scope has no label leaves the component to the paths', () => {
  const d = derive('fix(agent): the run ends early',
    'crates/gglib-core/src/a.rs and crates/gglib-core/src/b.rs');
  assert.deepEqual(d['component:'],
    { apply: ['component: core'], suggest: [], from: 'the paths the description names' });
});

test('several scopes stay candidates beside the description\'s paths', () => {
  const d = derive('feat(proxy,axum): a new route',
    'crates/gglib-proxy/src/a.rs, crates/gglib-proxy/src/b.rs and src/pages/A.tsx');
  assert.deepEqual(d['component:'], {
    apply: [],
    suggest: ['component: proxy', 'component: axum', 'component: frontend'],
    from: 'the title\'s scopes and the paths the description names',
  });
});

test('Windows or macOS named alone in the title sets the platform', () => {
  assert.deepEqual(derive('daemon.log stays empty on Windows')['platform:'],
    { apply: ['platform: windows'], suggest: [], from: '`Windows` in the title' });
  assert.deepEqual(derive('The tray icon is blank on macos 26')['platform:'],
    { apply: ['platform: macos'], suggest: [], from: '`macOS` in the title' });
});

test('Linux named in the title is a candidate, and is not applied', () => {
  assert.deepEqual(derive('CI\'s Linux jobs download from one mirror')['platform:'],
    { apply: [], suggest: ['platform: linux'], from: 'the platforms this issue names' });
  assert.deepEqual(derive('fix(cli): setup fails on linux')['platform:'].suggest, ['platform: linux']);
});

test('a platform is a whole word, and Windows is capitalised', () => {
  assert.deepEqual(derive('Two context windows are counted as one')['platform:'], { apply: [], suggest: [], from: '' });
  assert.deepEqual(derive('The Linuxbrew formula is stale')['platform:'], { apply: [], suggest: [], from: '' });
});

test('several platforms, or one named only in the description, are candidates', () => {
  assert.deepEqual(derive('Linux/Windows show a menubar meant for macOS')['platform:'], {
    apply: [],
    suggest: ['platform: windows', 'platform: macos', 'platform: linux'],
    from: 'the platforms this issue names',
  });
  assert.deepEqual(derive('The app does not start', 'I am on Windows 11.')['platform:'],
    { apply: [], suggest: ['platform: windows'], from: 'the platforms this issue names' });
  // The title names one and the description another: the title decides.
  assert.deepEqual(derive('The app does not start on Windows', 'It starts on macOS.')['platform:'].apply,
    ['platform: windows']);
});

test('a label the repository does not have is never produced', () => {
  const few = ['type: bug', 'component: cli'];
  assert.deepEqual(derive('perf(cli): a thing on Windows', '', few)['type:'], { apply: [], suggest: [], from: '' });
  assert.deepEqual(derive('perf(cli): a thing on Windows', '', few)['platform:'], { apply: [], suggest: [], from: '' });
  assert.deepEqual(derive('fix(proxy): a thing', 'src/a.ts and src/b.ts', few)['component:'],
    { apply: [], suggest: [], from: '' });
  assert.deepEqual(derive('fix(proxy,cli): a thing', '', few)['component:'].suggest, ['component: cli']);
});

test('an issue with no description is read from its title', () => {
  const d = deriveLabels({ title: 'fix(cli): a thing', body: null, repoLabels: new Set(REPO_LABELS) });
  assert.deepEqual(d['component:'].apply, ['component: cli']);
});
