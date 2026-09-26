export const meta = {
  name: 'fix-review-publish',
  description: 'Apply a narrow, specified repair, then a focused adversarial review (up to two rounds), then publish on PASS',
  phases: [{ title: 'Fix' }, { title: 'Review' }, { title: 'Publish' }],
}
const SP = '/private/tmp/claude-501/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/scratchpad'
const pr = args
const VERDICT = { type: 'object', properties: { verdict: { type: 'string', enum: ['PASS', 'FAIL'] }, defects: { type: 'array', items: { type: 'object', properties: { where: { type: 'string' }, what: { type: 'string' }, fix: { type: 'string' } }, required: ['where', 'what', 'fix'] } }, commands: { type: 'string' } }, required: ['verdict', 'defects'] }
const IMPL = { type: 'object', properties: { status: { type: 'string', enum: ['done', 'blocked'] }, head_sha: { type: 'string' }, summary: { type: 'string' } }, required: ['status', 'head_sha', 'summary'] }
let head = pr.head_sha
let defects = pr.defects
let v = null
for (let round = 1; round <= 2; round++) {
  phase('Fix')
  const f = await agent(`Read ${SP}/briefs/IMPLEMENTER.md and follow it.

Narrow repair to PR ${pr.id} in worktree ${pr.wt}, branch '${pr.branch}' (head ${head}, one commit on origin/main). Change ONLY what the defects below ask, in the places they name, using the wording they give (it was verified against the code by a reviewer). Amend the single commit: keep the subject; if the message must change, write it to a file and commit with -F <file> --cleanup=whitespace, then read it back. Run: ${pr.checks}. Do not push. Report the new head_sha and exactly which files and lines changed.

DEFECTS:
${JSON.stringify(defects, null, 1)}`, { label: `fix:${pr.id}:r${round}`, phase: 'Fix', model: 'opus', schema: IMPL })
  if (!f || f.status !== 'done') return { id: pr.id, status: 'fix-blocked', fix: f }
  const prev = head
  head = f.head_sha
  phase('Review')
  v = await agent(`Read ${SP}/briefs/REVIEWER.md first and follow it.

Focused review of a narrow repair to PR ${pr.id} (repo ${pr.repo}, main repo /Users/mattogrady/.local/src/${pr.repo}). Commit under review: ${head} on branch '${pr.branch}' (the previous head was ${prev}). Make your own worktree at ${SP}/review/${pr.id}-focus-${round}/${pr.repo}; remove it at the end.
${pr.focus}
The repair was asked to apply exactly these defects' fixes:
${JSON.stringify(defects, null, 1)}
Check: (1) git diff ${prev} ${head} changes only what the defects name; (2) every sentence the repair wrote is true at ${head} — probe it against the code, including the edge cases named in the defects; (3) the rest of the PR's claims of the same kind (grep for them) agree; (4) the checks: ${pr.checks}; (5) attribution and identity. PASS only if all hold.`, { label: `review:${pr.id}:focus-${round}`, phase: 'Review', model: 'opus', schema: VERDICT })
  if (v && v.verdict === 'PASS') break
  defects = v ? v.defects : [{ where: '-', what: 'reviewer returned nothing', fix: 're-review' }]
}
if (!v || v.verdict !== 'PASS') return { id: pr.id, status: 'review-not-passed', head, verdict: v }
phase('Publish')
const out = await agent(`Read ${SP}/briefs/PUBLISHER.md first and follow it exactly.

Publish PR ${pr.id}: repo mmogr/${pr.repo}, worktree ${pr.wt}, branch '${pr.branch}', head ${head} (a focused reviewer PASS was just given for exactly this sha).
Title: ${pr.subject}
Labels: ${(pr.labels || []).join(', ') || '(none for this repo)'}
Closing lines for the body: ${pr.closes || '(none)'}
${pr.publish_note}
The focused reviewer's verdict: ${JSON.stringify(v)}`, { label: `publish:${pr.id}`, phase: 'Publish', model: 'opus', schema: { type: 'object', properties: { status: { type: 'string' }, pr_number: { type: ['integer', 'null'] }, url: { type: 'string' }, notes: { type: 'string' } }, required: ['status', 'notes'] } })
return { id: pr.id, status: out && out.status, pr: out && out.pr_number, url: out && out.url, head, notes: out && out.notes }
