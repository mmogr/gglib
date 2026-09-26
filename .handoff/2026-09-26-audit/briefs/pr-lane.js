export const meta = {
  name: 'pr-lane',
  description: 'Implement, adversarially review, fix and publish a short chain of plan-v2 PRs in one lane worktree',
  phases: [
    { title: 'Implement', detail: 'one implementer per PR, in the lane worktree' },
    { title: 'Review', detail: 'tiered adversarial reviewers, each in its own worktree' },
    { title: 'Fix', detail: 'the implementer fixes exactly what the reviewers listed' },
    { title: 'Publish', detail: 'push and open the PR on a PASS' },
  ],
}

const SP = '/private/tmp/claude-501/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/scratchpad'
const BRIEFS = `${SP}/briefs`
const PLAN = '/Users/mattogrady/.claude/plans/could-you-make-a-adaptive-candle.md'
const MAX_ROUNDS = 3

const IMPL = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['done', 'blocked'] },
    branch: { type: 'string' },
    base: { type: 'string' },
    head_sha: { type: 'string' },
    commits: { type: 'array', items: { type: 'string' } },
    summary: { type: 'string' },
    commands_run: { type: 'string' },
    mutations: { type: 'string' },
    deviations: { type: 'string' },
    review_focus: { type: 'string' },
  },
  required: ['status', 'branch', 'head_sha', 'summary', 'commands_run'],
}
const VERDICT = {
  type: 'object',
  properties: {
    verdict: { type: 'string', enum: ['PASS', 'FAIL'] },
    defects: { type: 'array', items: { type: 'object', properties: {
      where: { type: 'string' }, what: { type: 'string' }, fix: { type: 'string' } }, required: ['where', 'what', 'fix'] } },
    commands: { type: 'string' },
    mutations: { type: 'string' },
  },
  required: ['verdict', 'defects'],
}
const PUB = {
  type: 'object',
  properties: {
    status: { type: 'string', enum: ['published', 'refused'] },
    pr_number: { type: ['integer', 'null'] },
    url: { type: 'string' },
    notes: { type: 'string' },
  },
  required: ['status', 'notes'],
}

const LENS_TEXT = {
  correctness: 'Lens: correctness and mutations. Build and run the targeted tests; do every mutation the plan lists for this PR (and at least one per behaviour); confirm each named test goes red and green again.',
  platform: 'Lens: build, lints, platform and contracts, by reading (no build needed unless you choose to build a small crate). Read every cfg-gated path the change touches (linux, windows, macos), wire and ts-rs contracts, docs, ratchet effects, and whether the commit messages and doc comments are true at this commit.',
  build: 'Lens: a single build reviewer. Build and run the targeted tests, try at least one mutation per behaviour, read cfg-gated paths, check ratchets, docs and commit messages.',
  docs: 'Lens: source-only docs/metadata review. Every added sentence is true at this commit (check each against the code), no fenced block removed, links and anchors resolve, scripts and CI YAML still parse (run the repo check scripts that apply, e.g. check_workflow_yaml.sh), nothing outside the plan section changed.',
}

function lensesFor(pr) {
  if (pr.tier === 1) return ['correctness', 'platform']
  if (pr.tier === 3) return ['docs']
  return ['build']
}

function implPrompt(pr) {
  return `Read ${BRIEFS}/IMPLEMENTER.md first and follow it exactly.

PR: ${pr.id} in repo ${pr.repo}. Plan section: "${pr.section}" in ${PLAN} (read the whole Process section too).
Worktree (yours alone): ${pr.wt}
Create branch '${pr.branch}' from ${pr.base}.
Subject: ${pr.subject}
Review tier: ${pr.tier}.
${pr.notes ? `Notes from the owner's orchestrator (these override the plan where they conflict):\n${pr.notes}\n` : ''}
When done, return the report in the schema. head_sha is the final commit on '${pr.branch}'.`
}

function reviewPrompt(pr, impl, lens, round) {
  const wtLeaf = pr.repo
  const path = `${SP}/review/${pr.id}-${lens}-r${round}/${wtLeaf}`
  const tgt = lens === 'correctness' || lens === 'build' ? (pr.review_target || '') : ''
  return `Read ${BRIEFS}/REVIEWER.md first and follow it exactly.

Review PR ${pr.id} (repo ${pr.repo}, main repo path /Users/mattogrady/.local/src/${pr.repo}).
Plan section: "${pr.section}" in ${PLAN}. Review tier ${pr.tier}.
Commit under review: ${impl.head_sha} on branch '${impl.branch}', base ${pr.base}.
Make your own worktree at: ${path}  (git -C /Users/mattogrady/.local/src/${pr.repo} worktree add --detach "${path}" ${impl.head_sha}); the author's worktree is ${pr.wt} and is off limits.
${tgt ? `Build with CARGO_TARGET_DIR=${tgt} CARGO_PROFILE_DEV_DEBUG=line-tables-only CARGO_INCREMENTAL=0.` : 'Source-only: do not build unless a check genuinely needs it; if it does, use a target under ' + SP + '/targets/tmp-' + pr.id + ' and delete it at the end.'}
${pr.repo === 'gglib' && (lens === 'correctness' || lens === 'build') ? 'gglib needs web_ui to build gglib-app: copy it with cp -R /Users/mattogrady/.local/src/gglib/web_ui "' + path + '/web_ui" and symlink node_modules from /Users/mattogrady/.local/src/gglib/node_modules if you run frontend checks.' : ''}
${LENS_TEXT[lens]}
Round ${round}.${round > 1 ? ' Earlier rounds found defects that the author says are fixed; verify every one of them, and review the whole range again, since a repair is new prose.' : ''}
${pr.review_notes ? `Specific things to check:\n${pr.review_notes}\n` : ''}
Author's report (unverified):
${JSON.stringify(impl, null, 1)}`
}

function fixPrompt(pr, impl, fails) {
  return `Read ${BRIEFS}/IMPLEMENTER.md again and follow it.

You are fixing PR ${pr.id} (repo ${pr.repo}) in worktree ${pr.wt}, branch '${impl.branch}' (head ${impl.head_sha}, base ${pr.base}). Plan section: "${pr.section}" in ${PLAN}.
Adversarial reviewers found the defects below. Fix every one, or, if a defect is wrong, leave the code and explain precisely why with file:line evidence. Keep the history atomic: fold each fix into the commit it belongs to (git commit --fixup=<sha> then GIT_SEQUENCE_EDITOR=: git rebase -i --autosquash ${pr.base}), keeping the subjects. Re-run the fast checks and the mutations the defects touch. Do not push.

DEFECTS:
${JSON.stringify(fails, null, 1)}

Return the same report schema with the new head_sha and, in deviations, one line per defect: fixed / disputed (why).`
}

function publishPrompt(pr, impl) {
  return `Read ${BRIEFS}/PUBLISHER.md first and follow it exactly.

Publish PR ${pr.id}: repo mmogr/${pr.repo}, worktree ${pr.wt}, branch '${impl.branch}', reviewed sha ${impl.head_sha} (a reviewer PASS was given for exactly this sha).
Title: ${pr.subject}
Labels: ${(pr.labels || []).join(', ') || '(none for this repo)'}
${pr.draft ? 'Open it as a DRAFT pull request (gh pr create --draft) and say why in the body: ' + pr.draft : ''}
Closing lines for the body: ${pr.closes || '(none)'}
Author's report, for the body's "How it is kept true" (state only what was actually run):
${JSON.stringify(impl, null, 1)}`
}

const results = []
for (const pr of args.prs) {
  let impl
  if (pr.resume) {
    // Resume a reviewed branch: apply the listed defects first, then the normal review loop.
    phase('Fix')
    const base = { status: 'done', branch: pr.branch, head_sha: pr.resume.head_sha, summary: 'resumed from an earlier run', commands_run: '' }
    const fixed = await agent(fixPrompt(pr, base, pr.resume.defects), { label: `fix:${pr.id}:resume`, phase: 'Fix', model: 'opus', schema: IMPL })
    impl = fixed && fixed.status === 'done' ? { ...base, ...fixed } : fixed
  } else {
    phase('Implement')
    impl = await agent(implPrompt(pr), { label: `impl:${pr.id}`, phase: 'Implement', model: 'opus', schema: IMPL })
  }
  if (!impl || impl.status !== 'done') {
    results.push({ id: pr.id, status: 'impl-blocked', impl })
    log(`${pr.id}: implementer blocked; stopping this lane`)
    break
  }
  let passed = false
  let lastVerdicts = []
  for (let round = 1; round <= MAX_ROUNDS; round++) {
    const lenses = lensesFor(pr)
    const verdicts = await parallel(lenses.map(lens => () =>
      agent(reviewPrompt(pr, impl, lens, round), { label: `review:${pr.id}:${lens}:r${round}`, phase: 'Review', model: 'opus', schema: VERDICT })
        .then(v => v && { ...v, lens })))
    lastVerdicts = verdicts.filter(Boolean)
    const fails = lastVerdicts.filter(v => v.verdict !== 'PASS')
    log(`${pr.id} round ${round}: ${lastVerdicts.map(v => `${v.lens}=${v.verdict}`).join(', ')}`)
    if (lastVerdicts.length === lenses.length && fails.length === 0) { passed = true; break }
    if (round === MAX_ROUNDS) break
    phase('Fix')
    const fixed = await agent(fixPrompt(pr, impl, fails.length ? fails : [{ lens: 'missing', defects: [{ where: '-', what: 'a reviewer returned nothing', fix: 'no code change needed; re-review' }] }]),
      { label: `fix:${pr.id}:r${round}`, phase: 'Fix', model: 'opus', schema: IMPL })
    if (!fixed || fixed.status !== 'done') { results.push({ id: pr.id, status: 'fix-blocked', impl: fixed, verdicts: lastVerdicts }); break }
    impl = { ...impl, ...fixed }
  }
  if (!passed) {
    results.push({ id: pr.id, status: 'review-not-passed', sha: impl.head_sha, branch: impl.branch, verdicts: lastVerdicts.map(v => ({ lens: v.lens, verdict: v.verdict, defects: v.defects })) })
    log(`${pr.id}: no PASS after ${MAX_ROUNDS} rounds; stopping this lane`)
    break
  }
  if (!pr.publish) {
    results.push({ id: pr.id, status: 'reviewed-local', sha: impl.head_sha, branch: impl.branch, summary: impl.summary })
    continue
  }
  phase('Publish')
  const pub = await agent(publishPrompt(pr, impl), { label: `publish:${pr.id}`, phase: 'Publish', model: 'opus', schema: PUB })
  results.push({ id: pr.id, status: pub ? pub.status : 'publish-failed', pr: pub && pub.pr_number, url: pub && pub.url, sha: impl.head_sha, branch: impl.branch, notes: pub && pub.notes })
}
return results
