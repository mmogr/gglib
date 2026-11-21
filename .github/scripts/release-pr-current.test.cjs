// Tests of release-pr-current.cjs, against a repository held in memory.
// `make enforce` runs them.

'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');

const { WORKFLOW_FILE, isReleasePr, checkCurrent, recheck } = require('./release-pr-current.cjs');

// A pull request as the `pull_request` event carries it. `base.sha` is the
// base's tip when the event was sent, which a later run must not trust.
function pullRequest({ number = 12, head = 'release-please--branches--main', base = 'main', fork = false, sha = 'head1' } = {}) {
  return {
    number,
    head: { ref: head, sha, repo: { full_name: fork ? 'someone/r' : 'o/r' } },
    base: { ref: base, sha: 'tip-when-sent', repo: { full_name: 'o/r' } },
  };
}

// The repository: each branch's tip, how far behind a tip each commit is,
// the open pull requests and the check's runs. `calls` is every request made.
function repository({ tips = { main: 'tip1' }, behind = {}, pulls = [], runs = [] } = {}) {
  const state = { tips, behind, pulls, runs, calls: [], reran: [], failed: null, slept: 0 };
  const github = {
    rest: {
      repos: {
        getBranch: async ({ branch }) => {
          state.calls.push(['getBranch', branch]);
          return { data: { commit: { sha: state.tips[branch] } } };
        },
        compareCommitsWithBasehead: async ({ basehead }) => {
          state.calls.push(['compare', basehead]);
          return { data: { behind_by: state.behind[basehead] ?? 0 } };
        },
      },
      pulls: {
        list: async ({ state: wanted, base, head }) => {
          state.calls.push(['listPulls', wanted, base, head]);
          return { data: state.pulls.filter(p => p.base.ref === base && `o:${p.head.ref}` === head) };
        },
      },
      actions: {
        listWorkflowRuns: async ({ workflow_id, head_sha, event }) => {
          state.calls.push(['listRuns', workflow_id, head_sha, event]);
          return { data: { workflow_runs: state.runs.filter(r => r.head_sha === head_sha && r.event === event) } };
        },
        getWorkflowRun: async ({ run_id }) => {
          state.calls.push(['getRun', run_id]);
          const run = state.runs.find(r => r.id === run_id);
          // A run that is under way ends after the polls it was given.
          if (run.status !== 'completed' && run.polls !== undefined && --run.polls <= 0) run.status = 'completed';
          return { data: run };
        },
        reRunWorkflow: async ({ run_id }) => {
          state.calls.push(['rerun', run_id]);
          state.reran.push(run_id);
        },
      },
    },
  };
  const core = { info() {}, notice() {}, setFailed: (message) => { state.failed = message; } };
  const sleep = async () => { state.slept += 1; };
  return { state, github, core, sleep };
}

const event = (pr) => ({ repo: { owner: 'o', repo: 'r' }, payload: { pull_request: pr } });

test('a release pull request is the branch release-please keeps for its base, in this repository', () => {
  assert.equal(isReleasePr(pullRequest()), true);
  assert.equal(isReleasePr(pullRequest({ head: 'release-please--branches--release/0.29.x', base: 'release/0.29.x' })), true);

  assert.equal(isReleasePr(pullRequest({ head: 'fix(cli)/the-spinner-ends' })), false);
  assert.equal(isReleasePr(pullRequest({ head: 'dependabot/cargo/tokio-1.50.0' })), false);
  // A fork's branch of the same name is not release-please's.
  assert.equal(isReleasePr(pullRequest({ fork: true })), false);
  // main's release branch aimed at a line, and a line's at main, are neither's.
  assert.equal(isReleasePr(pullRequest({ base: 'release/0.29.x' })), false);
  assert.equal(isReleasePr(pullRequest({ head: 'release-please--branches--release/0.29.x' })), false);
  // A pull request whose fork has been deleted has no head repository.
  const orphan = pullRequest();
  orphan.head.repo = null;
  assert.equal(isReleasePr(orphan), false);
});

test('any other pull request passes without a request', async () => {
  for (const pr of [pullRequest({ head: 'fix(cli)/the-spinner-ends' }), pullRequest({ fork: true })]) {
    const { state, github, core } = repository({ behind: { 'tip1...head1': 3 } });
    await checkCurrent({ github, core, context: event(pr) });
    assert.equal(state.failed, null);
    assert.deepEqual(state.calls, []);
  }
});

test('a release pull request that holds its branch\'s latest commit passes', async () => {
  const { state, github, core } = repository();
  await checkCurrent({ github, core, context: event(pullRequest()) });
  assert.equal(state.failed, null);
  // The tip is read now, not taken from the event: a run started again later
  // carries the event it first had.
  assert.deepEqual(state.calls, [['getBranch', 'main'], ['compare', 'tip1...head1']]);
});

test('a release pull request behind its branch fails, and says by how much, what brings it up to date and what must not', async () => {
  const { state, github, core } = repository({ tips: { main: 'tip2abcdefgh' }, behind: { 'tip2abcdefgh...head1': 2 } });
  await checkCurrent({ github, core, context: event(pullRequest()) });
  assert.equal(
    state.failed,
    'Release PR #12 is 2 commits behind main, now at tip2abc. Merged as it is, those commits would ship in the release with no line in its changelog. The Release PR workflow rebuilds it after CI passes on main, and this check runs again on what it pushes. Do not press Update branch: it brings the commits in without their changelog lines, and this check then passes.',
  );
});

test('one commit behind is said as one', async () => {
  const { state, github, core } = repository({ behind: { 'tip1...head1': 1 } });
  await checkCurrent({ github, core, context: event(pullRequest()) });
  assert.match(state.failed, /^Release PR #12 is 1 commit behind main, /);
});

test('a line\'s release pull request is held to the line', async () => {
  const pr = pullRequest({ head: 'release-please--branches--release/0.29.x', base: 'release/0.29.x' });
  const { state, github, core } = repository({ tips: { main: 'tip1', 'release/0.29.x': 'line1' }, behind: { 'line1...head1': 1 } });
  await checkCurrent({ github, core, context: event(pr) });
  assert.deepEqual(state.calls, [['getBranch', 'release/0.29.x'], ['compare', 'line1...head1']]);
  assert.match(state.failed, / behind release\/0\.29\.x, /);
});

const done = (id, sha, more = {}) => ({ id, head_sha: sha, event: 'pull_request', status: 'completed', run_number: id, ...more });

test('a branch with no open release pull request has nothing to run again', async () => {
  const { state, github, core, sleep } = repository({ runs: [done(1, 'head1')] });
  assert.deepEqual(await recheck({ github, core, owner: 'o', repo: 'r', branch: 'main', sleep }), []);
  assert.deepEqual(state.calls, [['listPulls', 'open', 'main', 'o:release-please--branches--main']]);
});

test('the check\'s newest run on the release pull request\'s head is run again', async () => {
  const { state, github, core, sleep } = repository({
    pulls: [pullRequest({ sha: 'head2' })],
    // An older head's run, two of this head's, and a run of another event.
    runs: [done(1, 'head1'), done(2, 'head2'), done(3, 'head2'), done(4, 'head2', { event: 'workflow_dispatch' })],
  });
  assert.deepEqual(await recheck({ github, core, owner: 'o', repo: 'r', branch: 'main', sleep }), [3]);
  assert.deepEqual(state.reran, [3]);
  assert.deepEqual(state.calls[1], ['listRuns', WORKFLOW_FILE, 'head2', 'pull_request']);
  assert.equal(state.slept, 0);
});

test('a run that is under way is waited for, then run again', async () => {
  const { state, github, core, sleep } = repository({
    pulls: [pullRequest()],
    runs: [done(5, 'head1', { status: 'in_progress', polls: 3 })],
  });
  assert.deepEqual(await recheck({ github, core, owner: 'o', repo: 'r', branch: 'main', sleep }), [5]);
  assert.equal(state.slept, 3);
  assert.deepEqual(state.reran, [5]);
});

test('a run that does not end is not run again, and the wait is an error', async () => {
  const { state, github, core, sleep } = repository({
    pulls: [pullRequest()],
    runs: [done(5, 'head1', { status: 'queued' })],
  });
  await assert.rejects(
    recheck({ github, core, owner: 'o', repo: 'r', branch: 'main', sleep, polls: 4 }),
    /run 5 of Release PR Current on #12 had not ended after 4 polls/,
  );
  assert.equal(state.slept, 4);
  assert.deepEqual(state.reran, []);
});

test('a head the check has not run on yet is left to the run that is coming', async () => {
  const { state, github, core, sleep } = repository({ pulls: [pullRequest({ sha: 'head2' })], runs: [done(1, 'head1')] });
  assert.deepEqual(await recheck({ github, core, owner: 'o', repo: 'r', branch: 'main', sleep }), []);
  assert.deepEqual(state.reran, []);
});

test('a line\'s push looks for the line\'s release pull request', async () => {
  const line = pullRequest({ number: 30, head: 'release-please--branches--release/0.29.x', base: 'release/0.29.x', sha: 'line-head' });
  const { state, github, core, sleep } = repository({
    pulls: [pullRequest(), line],
    runs: [done(1, 'head1'), done(2, 'line-head')],
  });
  assert.deepEqual(await recheck({ github, core, owner: 'o', repo: 'r', branch: 'release/0.29.x', sleep }), [2]);
  assert.deepEqual(state.calls[0], ['listPulls', 'open', 'release/0.29.x', 'o:release-please--branches--release/0.29.x']);
});
