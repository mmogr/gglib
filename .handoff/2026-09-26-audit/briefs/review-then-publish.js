export const meta = {
  name: 'review-then-publish',
  description: 'One focused adversarial review of a narrow repair, then publish on PASS',
  phases: [{ title: 'Review' }, { title: 'Publish' }],
}
const SP = '/private/tmp/claude-501/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/scratchpad'
const pr = args
phase('Review')
const v = await agent(`Read ${SP}/briefs/REVIEWER.md first and follow it.

Focused review of a narrow repair to PR ${pr.id} (repo ${pr.repo}, main repo /Users/mattogrady/.local/src/${pr.repo}). Commit under review: ${pr.head_sha} on branch '${pr.branch}'. Make your own worktree at ${SP}/review/${pr.id}-focused/${pr.repo} (git worktree add --detach <path> ${pr.head_sha}); remove it at the end.
${pr.focus}
Verdict PASS only if every check holds.`, { label: `review:${pr.id}:focused`, phase: 'Review', model: 'opus', schema: { type: 'object', properties: { verdict: { type: 'string', enum: ['PASS', 'FAIL'] }, defects: { type: 'array', items: { type: 'object', properties: { where: { type: 'string' }, what: { type: 'string' }, fix: { type: 'string' } }, required: ['where', 'what', 'fix'] } }, commands: { type: 'string' } }, required: ['verdict', 'defects'] } })
if (!v || v.verdict !== 'PASS') return { id: pr.id, status: 'review-not-passed', verdict: v }
phase('Publish')
const out = await agent(`Read ${SP}/briefs/PUBLISHER.md first and follow it exactly.

Publish PR ${pr.id}: repo mmogr/${pr.repo}, worktree ${pr.wt}, branch '${pr.branch}', head ${pr.head_sha} (a focused reviewer PASS was just given for exactly this sha; the earlier full review history is described below).
Title: ${pr.subject}
Labels: ${(pr.labels || []).join(', ') || '(none for this repo)'}
Closing lines for the body: ${pr.closes || '(none)'}
${pr.publish_note}
The focused reviewer's verdict: ${JSON.stringify(v)}`, { label: `publish:${pr.id}`, phase: 'Publish', model: 'opus', schema: { type: 'object', properties: { status: { type: 'string' }, pr_number: { type: ['integer', 'null'] }, url: { type: 'string' }, notes: { type: 'string' } }, required: ['status', 'notes'] } })
return { id: pr.id, status: out && out.status, pr: out && out.pr_number, url: out && out.url, notes: out && out.notes }
