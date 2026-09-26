export const meta = {
  name: 'publish-only',
  description: 'Publish one already-reviewed branch as a PR, following the publisher brief',
  phases: [{ title: 'Publish' }],
}
const SP = '/private/tmp/claude-501/-Users-mattogrady--local-src-gglib/d2f123ee-56ab-4c4c-a881-dbf2eef763a6/scratchpad'
const pr = args
phase('Publish')
const out = await agent(`Read ${SP}/briefs/PUBLISHER.md first and follow it exactly.

Publish PR ${pr.id}: repo mmogr/${pr.repo}, worktree ${pr.wt}, branch '${pr.branch}', head ${pr.head_sha}.
${pr.review_note}
Title: ${pr.subject}
Labels: ${(pr.labels || []).join(', ') || '(none for this repo)'}
Closing lines for the body: ${pr.closes || '(none)'}
The author's reports and the reviewers' last verdicts, for the body's "How it is kept true" (state only what was actually run): ${pr.reports}`,
  { label: `publish:${pr.id}`, phase: 'Publish', model: 'opus', schema: { type: 'object', properties: { status: { type: 'string' }, pr_number: { type: ['integer', 'null'] }, url: { type: 'string' }, notes: { type: 'string' } }, required: ['status', 'notes'] } })
return out
