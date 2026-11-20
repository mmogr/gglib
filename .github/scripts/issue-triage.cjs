// What issue-labels.yml does to an issue: `applyFromWords` labels a new issue
// from its own words (issue-words.cjs), and `verify` holds every issue to one
// label in each of `component:`, `priority:`, `size:` and `type:`, with
// `status: needs-triage` and one comment while any is missing. The comment
// shows the reply that adds a label, which label-command.cjs reads.
//
// Everything is read from the API when it is needed, never from the event's
// payload, which a re-run replays as it was.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

const { deriveLabels } = require('./issue-words.cjs');

const REQUIRED_PREFIXES = ['component:', 'priority:', 'size:', 'type:'];
const TRIAGE = 'status: needs-triage';

// The hidden first line of the one comment `verify` keeps on an issue.
const MARKER = '<!-- issue-label-triage -->';
// The author of a comment made with the workflow's own token. The comment
// `verify` rewrites and removes is this author's, so a person's comment that
// quotes the marker is never touched.
const COMMENT_AUTHOR = 'github-actions[bot]';

const code = (text) => `\`${text}\``;

// "a", "a or b", "a, b or c".
function either(items) {
  if (items.length < 2) return items.join('');
  return `${items.slice(0, -1).join(', ')} or ${items[items.length - 1]}`;
}

// The required categories anyone may add a label to with a reply, and a label
// of each for the reply the comment shows. label-command.cjs holds the rule.
const OPEN_EXAMPLES = {
  'component:': 'component: cli',
  'type:': 'type: bug',
};

// The label the comment's example reply asks for: one anyone may add, of the
// first missing category that is open to all, else of a platform the words
// name; a candidate when the words gave one. Null when a reporter has
// nothing to add.
function example(missing, open) {
  const prefix = missing.find(p => p in OPEN_EXAMPLES) ?? open.find(o => o.prefix === 'platform:')?.prefix;
  if (!prefix) return null;
  const candidates = open.find(o => o.prefix === prefix);
  return candidates ? candidates.labels[0] : OPEN_EXAMPLES[prefix];
}

// The comment's text. `missing` are the required prefixes with no label;
// `applied` the labels just set, as `{ label, from }`; `open` the candidates
// for a category with no label, as `{ prefix, labels, from }`.
function triageComment({ missing, applied, open }) {
  const candidates = (prefix) => {
    const found = open.find(o => o.prefix === prefix);
    return found ? `, which may be ${either(found.labels.map(code))}, from ${found.from}.` : '';
  };
  const lines = [MARKER, '### Label triage', ''];
  if (applied.length) {
    lines.push('Set from this issue\'s own words:', '');
    for (const { label, from } of applied) lines.push(`- ${code(label)}, from ${from}.`);
    lines.push('');
  }
  lines.push('Still missing:', '');
  for (const prefix of missing) lines.push(`- ${code(prefix)}${candidates(prefix)}`);
  lines.push('');
  if (missing.includes('priority:') || missing.includes('size:')) {
    lines.push('Nobody is asked for `priority:` or `size:` when filing an issue: each is an estimate'
      + ' made after reading the code, and set by someone with write access.', '');
  }
  const platform = open.find(o => o.prefix === 'platform:');
  if (platform) {
    lines.push(`\`platform:\` is not required. It may be ${either(platform.labels.map(code))}, from ${platform.from}.`, '');
  }
  // Inside a sentence, so that this comment is never itself a command.
  const label = example(missing, open);
  const reply = label
    ? `A reply with the line ${code(`/label ${label}`)} adds that label.`
      + ' Anyone can give an issue its first `component:`, `type:` or `platform:` label this way. '
    : '';
  lines.push(`${reply}${code(TRIAGE)} clears by itself once`
    + ` ${REQUIRED_PREFIXES.slice(0, -1).map(code).join(', ')} and ${code(REQUIRED_PREFIXES[REQUIRED_PREFIXES.length - 1])}`
    + ' are all present.');
  return lines.join('\n');
}

// The issue's title and description, its labels, and what its words say.
async function read({ github, owner, repo, issue_number }) {
  const { data: issue } = await github.rest.issues.get({ owner, repo, issue_number });
  const names = (await github.paginate(github.rest.issues.listLabelsOnIssue, {
    owner, repo, issue_number, per_page: 100,
  })).map(l => l.name);
  const repoLabels = (await github.paginate(github.rest.issues.listLabelsForRepo, {
    owner, repo, per_page: 100,
  })).map(l => l.name);
  const derived = deriveLabels({ title: issue.title, body: issue.body, repoLabels });
  const has = (prefix) => names.some(n => n.startsWith(prefix));
  return { names, derived, has };
}

// Labels a newly opened issue from its words, and returns the labels it set.
// A category that already has a label is left as it is, whoever set it, and
// nothing is ever removed.
async function applyFromWords({ github, core, owner, repo, issue_number }) {
  const { derived, has } = await read({ github, owner, repo, issue_number });
  const labels = Object.entries(derived)
    .filter(([prefix]) => !has(prefix))
    .flatMap(([, found]) => found.apply);
  if (labels.length) {
    await github.rest.issues.addLabels({ owner, repo, issue_number, labels });
    core.info(`Applied from the issue's words: ${labels.join(', ')}`);
  }
  return labels;
}

// Sets or clears `status: needs-triage` and writes, rewrites or removes the
// comment, from the issue as it is now. `applied` names the labels
// `applyFromWords` set in this run; each is said to be set only while the
// issue carries it and its words still give it.
async function verify({ github, core, owner, repo, issue_number, applied = [] }) {
  const { names, derived, has } = await read({ github, owner, repo, issue_number });

  const missing = REQUIRED_PREFIXES.filter(p => !has(p));
  const hasTriage = names.includes(TRIAGE);
  if (missing.length && !hasTriage) {
    await github.rest.issues.addLabels({ owner, repo, issue_number, labels: [TRIAGE] });
  } else if (!missing.length && hasTriage) {
    await github.rest.issues.removeLabel({ owner, repo, issue_number, name: TRIAGE })
      .catch(() => {}); // already gone is fine
  }

  const existing = (await github.paginate(github.rest.issues.listComments, {
    owner, repo, issue_number, per_page: 100,
  })).find(c => c.user?.login === COMMENT_AUTHOR && c.body?.includes(MARKER));

  if (!missing.length) {
    // The labels say it all, so the comment goes: its text lists categories
    // as missing that no longer are.
    if (existing) {
      try {
        await github.rest.issues.deleteComment({ owner, repo, comment_id: existing.id });
        core.info('Removed the triage comment: all required labels are present.');
      } catch (e) {
        core.warning(`Could not delete the triage comment: ${e.message}`);
      }
    }
    return;
  }

  const set = [];
  const open = [];
  for (const [prefix, found] of Object.entries(derived)) {
    if (has(prefix)) {
      for (const label of found.apply) {
        if (applied.includes(label) && names.includes(label)) set.push({ label, from: found.from });
      }
    } else {
      const labels = found.apply.length ? found.apply : found.suggest;
      if (labels.length) open.push({ prefix, labels, from: found.from });
    }
  }

  const body = triageComment({ missing, applied: set, open });
  if (!existing) {
    await github.rest.issues.createComment({ owner, repo, issue_number, body });
  } else if (existing.body !== body) {
    await github.rest.issues.updateComment({ owner, repo, comment_id: existing.id, body });
  }
}

module.exports = { REQUIRED_PREFIXES, TRIAGE, MARKER, triageComment, applyFromWords, verify };
