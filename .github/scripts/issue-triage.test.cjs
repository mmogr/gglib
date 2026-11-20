// Tests of issue-triage.cjs, against a repository held in memory. `make enforce`
// runs them.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { MARKER, TRIAGE, triageComment, applyFromWords, verify } = require('./issue-triage.cjs');

const REPO_LABELS = [
  'component: axum', 'component: cli', 'component: proxy', 'component: frontend',
  'platform: windows', 'platform: macos', 'platform: linux',
  'type: bug', 'type: feature', 'type: test',
  'priority: low', 'priority: high', 'size: s', 'size: m', TRIAGE,
];

const BOT = { login: 'github-actions[bot]', type: 'Bot' };
const PERSON = { login: 'someone', type: 'User' };

// One issue, its labels and its comments, behind the calls issue-triage.cjs
// makes. `calls` lists every write, in order.
function repository({ title, body = '', labels = [], comments = [], repoLabels = REPO_LABELS }) {
  const state = { title, body, labels: [...labels], comments: comments.map((c, i) => ({ id: i + 1, ...c })), calls: [] };
  let nextId = state.comments.length + 1;
  const issues = {
    get: async () => ({ data: { title: state.title, body: state.body } }),
    listLabelsOnIssue: async () => ({ data: state.labels.map(name => ({ name })) }),
    listLabelsForRepo: async () => ({ data: repoLabels.map(name => ({ name })) }),
    listComments: async () => ({ data: state.comments.map(c => ({ ...c })) }),
    addLabels: async ({ labels: added }) => {
      state.calls.push(['addLabels', added]);
      for (const l of added) if (!state.labels.includes(l)) state.labels.push(l);
    },
    removeLabel: async ({ name }) => {
      state.calls.push(['removeLabel', name]);
      state.labels = state.labels.filter(l => l !== name);
    },
    createComment: async ({ body: text }) => {
      state.calls.push(['createComment']);
      state.comments.push({ id: nextId++, user: BOT, body: text });
    },
    updateComment: async ({ comment_id, body: text }) => {
      state.calls.push(['updateComment', comment_id]);
      state.comments.find(c => c.id === comment_id).body = text;
    },
    deleteComment: async ({ comment_id }) => {
      state.calls.push(['deleteComment', comment_id]);
      state.comments = state.comments.filter(c => c.id !== comment_id);
    },
  };
  const github = { rest: { issues }, paginate: async (call, params) => (await call(params)).data };
  const core = { info() {}, warning() {} };
  return { state, args: { github, core, owner: 'o', repo: 'r', issue_number: 7 } };
}

const sticky = (state) => state.comments.filter(c => c.user.login === BOT.login && c.body.includes(MARKER));

test('the comment says what was set and from what, what is missing, and when triage clears', () => {
  const body = triageComment({
    missing: ['priority:', 'size:'],
    applied: [
      { label: 'type: bug', from: '`fix` in the title' },
      { label: 'component: cli', from: 'the title\'s scope `cli`' },
    ],
    open: [],
  });
  assert.equal(body, [
    MARKER,
    '### Label triage',
    '',
    'Set from this issue\'s own words:',
    '',
    '- `type: bug`, from `fix` in the title.',
    '- `component: cli`, from the title\'s scope `cli`.',
    '',
    'Still missing:',
    '',
    '- `priority:`',
    '- `size:`',
    '',
    '`priority:` and `size:` are never read from an issue\'s words: each is an estimate someone makes after reading the code.',
    '',
    'A reply with the line `/label priority: medium` adds that label. Anyone can give an issue its first `component:`, `type:` or `platform:` label this way; `priority:` and `size:` take write access. `status: needs-triage` clears by itself once `component:`, `priority:`, `size:` and `type:` are all present.',
  ].join('\n'));
});

test('a missing category lists its candidates, and a platform is offered without being required', () => {
  const body = triageComment({
    missing: ['component:', 'type:'],
    applied: [],
    open: [
      { prefix: 'component:', labels: ['component: proxy', 'component: axum'], from: 'the title\'s scopes' },
      { prefix: 'type:', labels: ['type: test'], from: '`test` in the title' },
      { prefix: 'platform:', labels: ['platform: windows'], from: 'the platforms this issue names' },
    ],
  });
  assert.equal(body, [
    MARKER,
    '### Label triage',
    '',
    'Still missing:',
    '',
    '- `component:`, which may be `component: proxy` or `component: axum`, from the title\'s scopes.',
    '- `type:`, which may be `type: test`, from `test` in the title.',
    '',
    '`platform:` is not required. It may be `platform: windows`, from the platforms this issue names.',
    '',
    'A reply with the line `/label component: proxy` adds that label. Anyone can give an issue its first `component:`, `type:` or `platform:` label this way; `priority:` and `size:` take write access. `status: needs-triage` clears by itself once `component:`, `priority:`, `size:` and `type:` are all present.',
  ].join('\n'));
});

test('the reply the comment shows asks for the first missing category', () => {
  const shown = (missing, open = []) => /`(\/label [^`]+)`/.exec(triageComment({ missing, applied: [], open }))[1];
  assert.equal(shown(['component:', 'size:']), '/label component: cli');
  assert.equal(shown(['type:']), '/label type: bug');
  assert.equal(shown(['size:']), '/label size: s');
  assert.equal(shown(['type:'], [{ prefix: 'type:', labels: ['type: test'], from: '`test` in the title' }]), '/label type: test');
  // A candidate for another category is not the example.
  assert.equal(shown(['size:'], [{ prefix: 'platform:', labels: ['platform: linux'], from: 'x' }]), '/label size: s');
});

test('the comment holds no line that is itself a command', () => {
  const { parseLabelCommands } = require('./label-command.cjs');
  const body = triageComment({
    missing: ['component:', 'priority:', 'size:', 'type:'],
    applied: [{ label: 'platform: windows', from: '`Windows` in the title' }],
    open: [{ prefix: 'component:', labels: ['component: cli'], from: 'the one path the description names' }],
  });
  assert.deepEqual(parseLabelCommands(body), []);
});

test('opening applies what the words say to the categories that are empty', async () => {
  const { state, args } = repository({ title: 'fix(cli): the spinner overwrites the line on Windows' });
  const applied = await applyFromWords(args);
  assert.deepEqual(applied, ['type: bug', 'component: cli', 'platform: windows']);
  assert.deepEqual(state.labels, ['type: bug', 'component: cli', 'platform: windows']);
});

test('a category that has a label is left as it is', async () => {
  const { state, args } = repository({
    title: 'fix(cli): the spinner overwrites the line on Windows',
    labels: ['type: feature', 'component: proxy', 'platform: macos'],
  });
  assert.deepEqual(await applyFromWords(args), []);
  assert.deepEqual(state.calls, []);
  assert.deepEqual(state.labels, ['type: feature', 'component: proxy', 'platform: macos']);
});

test('candidates are not applied', async () => {
  const { state, args } = repository({
    title: 'test(proxy,axum): a harness',
    body: 'It fails on Windows and on macOS.',
  });
  assert.deepEqual(await applyFromWords(args), []);
  assert.deepEqual(state.calls, []);
});

test('an issue missing labels is flagged, with one comment saying what was set', async () => {
  const { state, args } = repository({ title: 'fix(cli): the spinner overwrites the line' });
  const applied = await applyFromWords(args);
  await verify({ ...args, applied });
  assert.ok(state.labels.includes(TRIAGE));
  assert.equal(sticky(state).length, 1);
  const text = sticky(state)[0].body;
  assert.match(text, /^- `type: bug`, from `fix` in the title\.$/m);
  assert.match(text, /^- `component: cli`, from the title's scope `cli`\.$/m);
  assert.match(text, /^- `priority:`$/m);
  assert.match(text, /^- `size:`$/m);
  assert.doesNotMatch(text, /^- `(type|component):`/m);
});

test('a later run lists what is still missing, with the candidates the words still give', async () => {
  const { state, args } = repository({
    title: 'feat(proxy,axum): a new route',
    body: 'Seen on Windows.',
    labels: ['type: feature', 'priority: low', TRIAGE],
    comments: [{ user: BOT, body: `${MARKER}\nsomething older` }],
  });
  await verify(args);
  assert.deepEqual(state.calls, [['updateComment', 1]]);
  const text = sticky(state)[0].body;
  assert.doesNotMatch(text, /Set from this issue's own words/);
  assert.match(text, /^- `component:`, which may be `component: proxy` or `component: axum`, from the title's scopes\.$/m);
  assert.match(text, /^- `size:`$/m);
  assert.doesNotMatch(text, /^- `(type|priority):`/m);
  assert.match(text, /^`platform:` is not required\. It may be `platform: windows`/m);
});

test('a label the words give for an empty category is offered on a later run, and not applied', async () => {
  const { state, args } = repository({
    title: 'fix(cli): the spinner overwrites the line',
    labels: ['type: bug', 'priority: low', 'size: s', TRIAGE],
  });
  await verify(args);
  assert.deepEqual(state.labels, ['type: bug', 'priority: low', 'size: s', TRIAGE]);
  assert.match(sticky(state)[0].body, /^- `component:`, which may be `component: cli`, from the title's scope `cli`\.$/m);
});

test('a label named as applied is said to be set only while the issue carries it', async () => {
  const { state, args } = repository({
    title: 'fix(cli): the spinner overwrites the line',
    labels: ['type: bug', 'component: proxy'],
  });
  await verify({ ...args, applied: ['type: bug', 'component: cli', 'priority: critical', 'not a label'] });
  const text = sticky(state)[0].body;
  assert.match(text, /^- `type: bug`, from `fix` in the title\.$/m);
  assert.doesNotMatch(text, /^- `component: cli`, from/m);
  assert.doesNotMatch(text, /critical|not a label/);
});

test('the comment is left alone when it already says the same', async () => {
  const { state, args } = repository({ title: 'A thing is wrong', labels: [TRIAGE] });
  await verify(args);
  assert.deepEqual(state.calls, [['createComment']]);
  await verify(args);
  assert.deepEqual(state.calls, [['createComment']]);
});

test('with all four categories present triage clears and the comment is removed', async () => {
  const { state, args } = repository({
    title: 'A thing is wrong',
    labels: ['component: cli', 'priority: low', 'size: s', 'type: bug', TRIAGE],
    comments: [{ user: BOT, body: `${MARKER}\nolder` }],
  });
  await verify(args);
  assert.deepEqual(state.calls, [['removeLabel', TRIAGE], ['deleteComment', 1]]);
  assert.deepEqual(state.labels, ['component: cli', 'priority: low', 'size: s', 'type: bug']);
});

test('an issue with all four and no comment is left untouched', async () => {
  const { state, args } = repository({
    title: 'A thing is wrong',
    labels: ['component: cli', 'priority: low', 'size: s', 'type: bug'],
  });
  await verify(args);
  assert.deepEqual(state.calls, []);
});

test('a person\'s comment that holds the marker is neither rewritten nor removed', async () => {
  const missing = repository({
    title: 'A thing is wrong',
    comments: [{ user: PERSON, body: `quoting the bot: ${MARKER}` }],
  });
  await verify(missing.args);
  assert.deepEqual(missing.state.calls, [['addLabels', [TRIAGE]], ['createComment']]);
  assert.equal(missing.state.comments[0].body, `quoting the bot: ${MARKER}`);

  const complete = repository({
    title: 'A thing is wrong',
    labels: ['component: cli', 'priority: low', 'size: s', 'type: bug'],
    comments: [{ user: PERSON, body: `quoting the bot: ${MARKER}` }],
  });
  await verify(complete.args);
  assert.deepEqual(complete.state.calls, []);
});
