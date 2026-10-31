// The rule Label Check holds a fix to: a pull request titled `fix(…):` closes
// an issue that carries all four of component:, priority:, size: and type:,
// unless it carries `no-issue`. label-check.yml's two jobs both require this
// file: `enrich` puts the problem in its comment, `label-check` fails on it.
//
// CommonJS, hence `.cjs`: package.json says `"type": "module"`, and
// actions/github-script's `require` loads CommonJS.

'use strict';

const FIX_RE = /^fix(\([^)]*\))?!?:/;
const TRIAGE_PREFIXES = ['component:', 'priority:', 'size:', 'type:'];

// A closing keyword before `#N`, `owner/repo#N` or the issue's URL. A bare
// URL in the body closes nothing. A new expression on each call, since a
// global one carries its position between uses.
function linkRe(owner, repo) {
  return new RegExp(String.raw`\b(?:close[sd]?|fix(?:e[sd])?|resolve[sd]?)\s*:?\s*(?:https://github\.com/${owner}/${repo}/issues/|${owner}/${repo}#|#)(\d+)\b`, 'gi');
}

// What is wrong with pull request `number` under the rule, as one Markdown
// sentence, or null when nothing is. `labelNames` are the pull request's
// labels as the caller read them from the API.
async function fixIssueProblem({ github, owner, repo, number, labelNames }) {
  // Title and body come from the API for the reason the labels do: a re-run
  // replays its event's payload, which can predate the latest edit.
  const { data: current } = await github.rest.pulls.get({ owner, repo, pull_number: number });
  if (!FIX_RE.test(current.title) || labelNames.includes('no-issue')) return null;

  const linked = new Set();
  for (const m of (current.body || '').matchAll(linkRe(owner, repo))) linked.add(Number(m[1]));
  if (!linked.size) {
    return '**A fix closes its issue.** Add `Closes #N`, or the `no-issue` label'
      + ' if there is none on purpose.';
  }

  const problems = [];
  for (const num of linked) {
    try {
      const { data: issue } = await github.rest.issues.get({ owner, repo, issue_number: num });
      if (issue.pull_request) {
        problems.push(`#${num} is a pull request, not an issue`);
        continue;
      }
      const labels = issue.labels.map(l => typeof l === 'string' ? l : l.name);
      const missing = TRIAGE_PREFIXES.filter(p => !labels.some(l => l.startsWith(p)));
      if (missing.length) {
        problems.push(`#${num} is missing ${missing.map(p => `\`${p}\``).join(', ')}`);
      }
    } catch (e) {
      problems.push(`#${num} could not be read (${e.message})`);
    }
  }
  if (!problems.length) return null;
  return `**A fix's issue is triaged first.** ${problems.join('; ')}.`
    + ` Link the right issue and give it all four labels, then edit this PR or re-run Label Check.`;
}

module.exports = { linkRe, fixIssueProblem };
