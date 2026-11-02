// The reminder Label Check gives a fix while a maintenance line exists: say
// whether the fix goes to the line. A pull request titled `fix(…):`, or
// labelled `type: bug`, carries either a `backport: X.Y.x` label (backport.yml
// cherry-picks it onto release/X.Y.x once merged) or `no-backport` (it stays
// on main). With neither, and a release/* branch in the repository, both of
// label-check.yml's jobs ask: `enrich` in its comment, `label-check` as a
// warning. A reminder, not a failure: the required check is unchanged, since
// a fix that belongs on main alone is the common case.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

const FIX_RE = /^fix(\([^)]*\))?!?:/;

// The lines that have a branch, as `X.Y.x`, newest first.
async function lines({ github, owner, repo }) {
  const { data: refs } = await github.rest.git.listMatchingRefs({ owner, repo, ref: 'heads/release/' });
  return refs
    .map(r => r.ref.replace(/^refs\/heads\/release\//, ''))
    .filter(name => /^\d+\.\d+\.x$/.test(name))
    .sort((a, b) => b.localeCompare(a, undefined, { numeric: true }));
}

// The reminder for pull request `title` with `labelNames`, as one Markdown
// sentence, or null when there is nothing to ask: not a fix, decided already,
// or no line to carry it.
async function backportAsk({ github, owner, repo, title, labelNames }) {
  const isFix = FIX_RE.test(title) || labelNames.includes('type: bug');
  if (!isFix) return null;
  if (labelNames.some(l => l.startsWith('backport: ')) || labelNames.includes('no-backport')) return null;
  const open = await lines({ github, owner, repo });
  if (!open.length) return null;
  const labels = open.map(l => `\`backport: ${l}\``).join(' or ');
  return `**Does this fix go to the maintenance line?** Label it ${labels} to have it`
    + ` cherry-picked onto the line when it merges, or \`no-backport\` to say it stays on main.`
    + ` A reminder only; CONTRIBUTING's "Maintenance lines" says which fixes go.`;
}

module.exports = { backportAsk };
