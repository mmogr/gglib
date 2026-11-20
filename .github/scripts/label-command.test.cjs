// Tests of label-command.cjs, against a repository held in memory.
// `make enforce` runs them.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { parseLabelCommands, matchLabels, runLabelCommands } = require('./label-command.cjs');

const REPO_LABELS = [
  'component: cli', 'component: proxy', 'component:build',
  'type: bug', 'type: chore', 'type:chore', 'type: epic',
  'platform: windows',
  'priority: low', 'priority: high', 'priority:high',
  'size: s', 'size: xl',
  'status: needs-triage', 'status: ready', 'no-issue', 'security', 'good first issue',
];

test('a line that is the command asks for its label', () => {
  assert.deepEqual(parseLabelCommands('/label component: cli'), ['component: cli']);
  assert.deepEqual(parseLabelCommands('It is the CLI.\n\n/label component: cli\n\nThanks.'), ['component: cli']);
  assert.deepEqual(parseLabelCommands('   /label   component: cli  \t'), ['component: cli']);
});

test('several commands in one comment are each read, with either line ending', () => {
  assert.deepEqual(parseLabelCommands('/label component: cli\n/label type: bug\n\n/label platform: windows'),
    ['component: cli', 'type: bug', 'platform: windows']);
  assert.deepEqual(parseLabelCommands('/label component: cli\r\n/label type: bug\r\n'),
    ['component: cli', 'type: bug']);
});

test('the command inside a sentence is not a command', () => {
  for (const body of [
    'please /label component: cli',
    'Reply with `/label component: cli` to add it.',
    'I tried "/label component: cli" already',
    '- /label component: cli',
    '> /label component: cli',
    '/labels component: cli',
    '/labelcomponent: cli',
    '/label',
    '/label   ',
    '//label component: cli',
  ]) {
    assert.deepEqual(parseLabelCommands(body), [], body);
  }
});

test('the command inside a code block is not a command', () => {
  assert.deepEqual(parseLabelCommands('```\n/label component: cli\n```'), []);
  assert.deepEqual(parseLabelCommands('```sh\n/label component: cli\n```\n/label type: bug'), ['type: bug']);
  assert.deepEqual(parseLabelCommands('~~~\n/label component: cli\n~~~'), []);
  // A longer fence is closed only by one as long.
  assert.deepEqual(parseLabelCommands('````\n```\n/label component: cli\n```\n````'), []);
  // A fence left open holds the rest of the comment.
  assert.deepEqual(parseLabelCommands('```\n/label component: cli'), []);
  // Four spaces or a tab make a code block too.
  assert.deepEqual(parseLabelCommands('    /label component: cli'), []);
  assert.deepEqual(parseLabelCommands('\t/label component: cli'), []);
});

test('the command word and a label copied in backticks or quotes are read', () => {
  assert.deepEqual(parseLabelCommands('/LABEL component: cli'), ['component: cli']);
  assert.deepEqual(parseLabelCommands('/label `component: cli`'), ['component: cli']);
  assert.deepEqual(parseLabelCommands('/label "component: cli"'), ['component: cli']);
});

test('a comment with no text asks for nothing', () => {
  assert.deepEqual(parseLabelCommands(''), []);
  assert.deepEqual(parseLabelCommands(null), []);
  assert.deepEqual(parseLabelCommands(undefined), []);
});

test('a label that exists is matched, and one that does not is not', () => {
  assert.deepEqual(matchLabels(['component: cli'], REPO_LABELS), [{ label: 'component: cli', needsWrite: false }]);
  assert.deepEqual(matchLabels(['component: clii'], REPO_LABELS), []);
  assert.deepEqual(matchLabels(['component: cli please'], REPO_LABELS), []);
  assert.deepEqual(matchLabels(['component'], REPO_LABELS), []);
});

test('a label is matched whatever its letter case or spacing', () => {
  const one = (text) => matchLabels([text], REPO_LABELS).map(m => m.label);
  assert.deepEqual(one('Component: CLI'), ['component: cli']);
  assert.deepEqual(one('component:cli'), ['component: cli']);
  assert.deepEqual(one('component:   cli'), ['component: cli']);
  assert.deepEqual(one('Platform: Windows'), ['platform: windows']);
});

test('only component:, type: and platform: are open to everyone, and priority: and size: need write access', () => {
  assert.deepEqual(matchLabels(['type: epic', 'platform: windows'], REPO_LABELS),
    [{ label: 'type: epic', needsWrite: false }, { label: 'platform: windows', needsWrite: false }]);
  assert.deepEqual(matchLabels(['priority: high', 'size: xl'], REPO_LABELS),
    [{ label: 'priority: high', needsWrite: true }, { label: 'size: xl', needsWrite: true }]);
});

test('a label outside the five categories cannot be asked for', () => {
  for (const text of ['status: ready', 'status: needs-triage', 'no-issue', 'security', 'good first issue']) {
    assert.deepEqual(matchLabels([text], REPO_LABELS), [], text);
  }
});

test('a label whose name has no space after the colon is not one of a category\'s', () => {
  // `component:build` is a label, and is not a `component: ` label.
  assert.deepEqual(matchLabels(['component:build'], REPO_LABELS), []);
  // `type:chore` and `priority:high` are asked for as the labels with the space.
  assert.deepEqual(matchLabels(['type:chore', 'priority:high'], REPO_LABELS),
    [{ label: 'type: chore', needsWrite: false }, { label: 'priority: high', needsWrite: true }]);
});

test('a label asked for twice is matched once', () => {
  assert.deepEqual(matchLabels(['component: cli', 'Component: CLI', 'component:cli'], REPO_LABELS),
    [{ label: 'component: cli', needsWrite: false }]);
});

// One issue and one new comment on it, behind the calls label-command.cjs
// makes. `calls` lists every call, in order. `association` is what the event
// says of the comment's author; left out, the payload carries none.
function event({ body, labels = [], author = { login: 'reporter', type: 'User' }, association, pull_request, reactionFails = false }) {
  const state = { labels: [...labels], calls: [] };
  const github = {
    rest: {
      issues: {
        listLabelsOnIssue: async () => { state.calls.push(['listLabelsOnIssue']); return { data: state.labels.map(name => ({ name })) }; },
        listLabelsForRepo: async () => { state.calls.push(['listLabelsForRepo']); return { data: REPO_LABELS.map(name => ({ name })) }; },
        addLabels: async ({ labels: added }) => { state.calls.push(['addLabels', added]); state.labels.push(...added); },
      },
      reactions: {
        createForIssueComment: async ({ comment_id, content }) => {
          if (reactionFails) throw new Error('Resource not accessible by integration');
          state.calls.push(['react', comment_id, content]);
        },
      },
    },
    paginate: async (call, params) => (await call(params)).data,
  };
  const context = {
    repo: { owner: 'o', repo: 'r' },
    payload: { issue: { number: 7, pull_request }, comment: { id: 55, body, user: author, author_association: association } },
  };
  const core = { info() {}, warning() {} };
  return { state, args: { github, core, context } };
}

const writes = (state) => state.calls.filter(([name]) => name === 'addLabels' || name === 'react');

// The associations GitHub gives an author who cannot write to the repository.
const NO_WRITE = ['NONE', 'CONTRIBUTOR', 'FIRST_TIME_CONTRIBUTOR', 'FIRST_TIMER', 'MANNEQUIN', undefined];
const WRITE = ['OWNER', 'MEMBER', 'COLLABORATOR'];

test('a reply adds the label it asks for and gets a thumbs-up', async () => {
  const { state, args } = event({ body: 'It is the CLI.\n/label component: cli', labels: ['type: bug'], association: 'NONE' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state), [['addLabels', ['component: cli']], ['react', 55, '+1']]);
});

test('a comment on a pull request is not read at all', async () => {
  const { state, args } = event({ body: '/label component: cli', association: 'OWNER', pull_request: { url: 'https://example.invalid/pulls/7' } });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(state.calls, []);
});

test('a bot\'s comment is not read', async () => {
  const { state, args } = event({ body: '/label component: cli', author: { login: 'github-actions[bot]', type: 'Bot' } });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(state.calls, []);
});

test('a comment with no command calls nothing', async () => {
  const { state, args } = event({ body: 'Reply with `/label component: cli` to add it.' });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(state.calls, []);
});

test('a label that does not exist is ignored, and never created', async () => {
  const { state, args } = event({ body: '/label component: clii\n/label status: ready\n/label anything at all', association: 'OWNER' });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(writes(state), []);
});

test('priority: and size: are ignored from someone without write access', async () => {
  for (const association of NO_WRITE) {
    const { state, args } = event({ body: '/label priority: high\n/label size: s', association });
    assert.equal(await runLabelCommands(args), false, String(association));
    assert.deepEqual(writes(state), [], String(association));
  }
});

test('priority: and size: are set by the owner, a member or a collaborator', async () => {
  for (const association of WRITE) {
    const { state, args } = event({ body: '/label priority: high\n/label size: s', association });
    assert.equal(await runLabelCommands(args), true, association);
    assert.deepEqual(writes(state), [['addLabels', ['priority: high', 'size: s']], ['react', 55, '+1']], association);
  }
});

test('a reply that asks for both gets the labels its author may set', async () => {
  const { state, args } = event({ body: '/label component: cli\n/label size: s', association: 'CONTRIBUTOR' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state), [['addLabels', ['component: cli']], ['react', 55, '+1']]);
});

test('an association the payload does not carry is no write access', async () => {
  const { state, args } = event({ body: '/label component: cli\n/label priority: high' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state), [['addLabels', ['component: cli']], ['react', 55, '+1']]);
});

test('without write access a reply fills only a category that has no label', async () => {
  for (const association of NO_WRITE) {
    const { state, args } = event({
      body: '/label component: proxy\n/label type: chore\n/label platform: windows',
      labels: ['component: cli', 'type: bug'],
      association,
    });
    assert.equal(await runLabelCommands(args), true, String(association));
    assert.deepEqual(writes(state), [['addLabels', ['platform: windows']], ['react', 55, '+1']], String(association));
    assert.deepEqual(state.labels, ['component: cli', 'type: bug', 'platform: windows']);
  }
});

test('without write access a reply that only asks for categories that have a label does nothing', async () => {
  const { state, args } = event({
    body: '/label component: proxy\n/label type: chore\n/label platform: windows',
    labels: ['component: cli', 'type: bug', 'platform: windows'],
    association: 'NONE',
  });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(writes(state), []);
  // Nor for the very label the issue carries: the category is not empty.
  const same = event({ body: '/label component: cli', labels: ['component: cli'], association: 'NONE' });
  assert.equal(await runLabelCommands(same.args), false);
  assert.deepEqual(writes(same.state), []);
});

test('without write access one reply adds one label to an empty category, the first it asks for', async () => {
  const { state, args } = event({
    body: '/label component: cli\n/label component: proxy\n/label type: bug\n/label type: chore',
    association: 'NONE',
  });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state), [['addLabels', ['component: cli', 'type: bug']], ['react', 55, '+1']]);
});

test('a category counts as filled by whatever label starts with its name', async () => {
  // As `verify` counts it: `component:build` is a label of `component:`.
  const { state, args } = event({ body: '/label component: cli', labels: ['component:build'], association: 'NONE' });
  assert.equal(await runLabelCommands(args), false);
  assert.deepEqual(writes(state), []);
});

test('with write access a reply adds a label beside the ones a category has, and nothing is removed', async () => {
  for (const association of WRITE) {
    const { state, args } = event({
      body: '/label component: proxy\n/label type: chore\n/label priority: high',
      labels: ['component: cli', 'type: bug', 'priority: low'],
      association,
    });
    assert.equal(await runLabelCommands(args), true, association);
    assert.deepEqual(state.labels,
      ['component: cli', 'type: bug', 'priority: low', 'component: proxy', 'type: chore', 'priority: high'], association);
  }
});

test('a label the issue already carries is not added again, and the reply still gets its thumbs-up', async () => {
  const { state, args } = event({ body: '/label component: cli', labels: ['component: cli'], association: 'OWNER' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state), [['react', 55, '+1']]);
});

test('a thumbs-up that cannot be given does not undo the label', async () => {
  const { state, args } = event({ body: '/label component: cli', reactionFails: true });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(state.labels, ['component: cli']);
});

test('what is added is the repository\'s name for the label, not the comment\'s text', async () => {
  const { state, args } = event({ body: '/label Component:CLI\r\n/label `TYPE: Bug`' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(writes(state)[0], ['addLabels', ['component: cli', 'type: bug']]);
});

test('nothing but the issue\'s labels, the repository\'s labels and the reaction is asked of the API', async () => {
  const { state, args } = event({ body: '/label component: cli\n/label priority: high', association: 'OWNER' });
  assert.equal(await runLabelCommands(args), true);
  assert.deepEqual(state.calls.map(([name]) => name).sort(),
    ['addLabels', 'listLabelsForRepo', 'listLabelsOnIssue', 'react']);
});
