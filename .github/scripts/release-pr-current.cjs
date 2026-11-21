// The check that holds a release pull request to its branch's latest commit,
// and the step that runs the check again when the branch moves.
// release-pr-current.yml runs `checkCurrent` on every pull request and
// `recheck` on each push to a branch that releases.
//
// A release pull request merged behind its branch ships the commits it has
// not seen with no line in the changelog, and may propose the wrong version:
// release-please counts only what follows the last release's tag, and those
// commits sit before it. Every other pull request may merge behind its
// branch, so this asks nothing of one.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

// The workflow whose runs `recheck` starts again, and the name of its check.
const WORKFLOW_FILE = 'release-pr-current.yml';
const CHECK_NAME = 'Release PR Current';

// release-please keeps a branch's release pull request on a branch of this
// name with the branch's own name after it: `release-please--branches--main`,
// `release-please--branches--release/0.29.x`.
const HEAD_PREFIX = 'release-please--branches--';

// Whether a pull request, as the `pull_request` event carries it, is the
// release pull request of its base: on release-please's branch for that base,
// in this repository. A fork's branch of that name is not release-please's.
function isReleasePr(pr) {
  return pr.head.ref === `${HEAD_PREFIX}${pr.base.ref}`
    && pr.head.repo != null
    && pr.head.repo.full_name === pr.base.repo.full_name;
}

// Passes any pull request that is not a release pull request without a
// request to GitHub, so nothing GitHub refuses a fork's or Dependabot's token
// can fail it. Fails a release pull request that lacks a commit of its base.
//
// The base's tip is read when this runs and never taken from the event: a run
// started again by `recheck` carries the event it first had, with the tip of
// that moment.
async function checkCurrent({ github, core, context }) {
  const pr = context.payload.pull_request;
  if (!isReleasePr(pr)) {
    core.info(`#${pr.number} is not a release pull request: nothing is asked of it.`);
    return;
  }
  const { owner, repo } = context.repo;
  const base = pr.base.ref;
  const { data: branch } = await github.rest.repos.getBranch({ owner, repo, branch: base });
  const tip = branch.commit.sha;
  const { data: compared } = await github.rest.repos.compareCommitsWithBasehead({
    owner, repo, basehead: `${tip}...${pr.head.sha}`, per_page: 1,
  });
  const behind = compared.behind_by;
  if (behind === 0) {
    core.info(`Release PR #${pr.number} holds ${base}'s latest commit, ${tip.slice(0, 7)}.`);
    return;
  }
  core.setFailed(
    `Release PR #${pr.number} is ${behind} commit${behind === 1 ? '' : 's'} behind ${base}, now at ${tip.slice(0, 7)}. `
    + 'Merged as it is, those commits would ship in the release with no line in its changelog. '
    + `The Release PR workflow rebuilds it after CI passes on ${base}, and this check runs again on what it pushes. `
    + 'Do not press Update branch: it brings the commits in without their changelog lines, and this check then passes.',
  );
}

const wait = (ms) => new Promise(resolve => setTimeout(resolve, ms));

// Runs the check again on `branch`'s open release pull request, which the
// push that started this has just put behind. Returns the ids of the runs it
// started again.
//
// The run is the check's newest on the pull request's head commit. One that
// is under way is waited for first, since GitHub starts only an ended run
// again and that run may have read the branch before the push. A head with no
// run yet has one coming, from the push that made it, which reads the branch
// after this push. With no release pull request open there is nothing to do.
async function recheck({ github, core, owner, repo, branch, sleep = wait, polls = 30, interval = 10_000 }) {
  const { data: pulls } = await github.rest.pulls.list({
    owner, repo, state: 'open', base: branch, head: `${owner}:${HEAD_PREFIX}${branch}`, per_page: 100,
  });
  if (pulls.length === 0) {
    core.info(`No release pull request is open on ${branch}.`);
    return [];
  }
  const reran = [];
  for (const pr of pulls) {
    const { data: { workflow_runs: runs } } = await github.rest.actions.listWorkflowRuns({
      owner, repo, workflow_id: WORKFLOW_FILE, head_sha: pr.head.sha, event: 'pull_request', per_page: 100,
    });
    if (runs.length === 0) {
      core.notice(`${CHECK_NAME} has not run on #${pr.number}'s head ${pr.head.sha.slice(0, 7)} yet. The run that is coming reads ${branch} as it is now.`);
      continue;
    }
    let run = runs.reduce((newest, r) => (r.run_number > newest.run_number ? r : newest));
    for (let poll = 0; run.status !== 'completed'; poll += 1) {
      if (poll === polls) {
        throw new Error(`run ${run.id} of ${CHECK_NAME} on #${pr.number} had not ended after ${polls} polls, so it was not run again. Run this workflow by hand for ${branch}.`);
      }
      await sleep(interval);
      ({ data: run } = await github.rest.actions.getWorkflowRun({ owner, repo, run_id: run.id }));
    }
    await github.rest.actions.reRunWorkflow({ owner, repo, run_id: run.id });
    core.info(`Started run ${run.id} of ${CHECK_NAME} again on release PR #${pr.number}, which ${branch} has moved past.`);
    reran.push(run.id);
  }
  return reran;
}

module.exports = { WORKFLOW_FILE, CHECK_NAME, HEAD_PREFIX, isReleasePr, checkCurrent, recheck };
