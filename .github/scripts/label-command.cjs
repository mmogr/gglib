// The reply that adds a label to an issue: a comment with a line that is
// `/label <label name>`. Without write access a reply gives an issue its
// first `component:`, `type:` or `platform:` label: one label for each of
// those categories that has none, and nothing for a category that has one,
// which is the rule `applyFromWords` follows. With write access a reply adds
// any label of those three and of `priority:` and `size:`, beside what is
// there. Every other label, and every other line, is ignored without a word.
// issue-labels.yml's `command` job runs `runLabelCommands` on each new comment
// that holds the word.
//
// The comment is public input and is only ever compared: a line's text is
// matched against the names of the repository's labels, and what is added is
// the repository's name for the label, never the text. A reply adds labels
// and removes none.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

// The categories a reply may add to, each with the space the repository's
// labels have after the colon: `component:build` is in none of them.
const OPEN_PREFIXES = ['component: ', 'type: ', 'platform: '];
const WRITE_PREFIXES = ['priority: ', 'size: '];

// What the event says of a comment's author when the author can write to the
// repository: its owner, a member of its organization, a collaborator. Any
// other value is no write access, and so is a payload that carries none.
const WRITE_ASSOCIATIONS = new Set(['OWNER', 'MEMBER', 'COLLABORATOR']);

// The whole of a line: the command, then the label's name.
const COMMAND_RE = /^\/label[ \t]+(\S.*)$/i;
// The line that opens or closes a fenced code block, and its run of marks.
const FENCE_RE = /^ {0,3}(`{3,}|~{3,})/;

// The label names a comment's commands ask for, as written. A command is a
// line of its own, indented by three spaces at most: one inside a sentence, a
// quotation, a list or a code block is text. The name may be wrapped in
// backticks or quotes, as it is when copied from the triage comment.
function parseLabelCommands(body) {
  const asked = [];
  let fence = null;
  for (const line of (body || '').split(/\r?\n/)) {
    const mark = FENCE_RE.exec(line);
    if (fence) {
      // A fence closes on a line of the same mark, at least as long, alone.
      if (mark && mark[1][0] === fence[0] && mark[1].length >= fence.length
          && line.trim() === mark[1]) fence = null;
      continue;
    }
    if (mark) { fence = mark[1]; continue; }
    if (/^( {4}|\t)/.test(line)) continue;
    const m = COMMAND_RE.exec(line.trim());
    if (!m) continue;
    const name = m[1].trim().replace(/^(["'`])(.*)\1$/, '$2').trim();
    if (name) asked.push(name);
  }
  return asked;
}

// A name as it is compared: lower case, with no space after its colon and
// single spaces elsewhere.
const comparable = (name) => name.toLowerCase().replace(/\s+/g, ' ').replace(/: /g, ':').trim();

// The repository's labels that `asked` names, among the categories a reply
// may add to, as `{ label, needsWrite }` in the order asked and once each.
// `label` is the repository's own spelling.
function matchLabels(asked, repoLabels) {
  const settable = new Map();
  for (const label of repoLabels) {
    const open = OPEN_PREFIXES.some(p => label.startsWith(p));
    if (open || WRITE_PREFIXES.some(p => label.startsWith(p))) {
      settable.set(comparable(label), { label, needsWrite: !open });
    }
  }
  const found = new Map();
  for (const name of asked) {
    const match = settable.get(comparable(name));
    if (match) found.set(match.label, match);
  }
  return [...found.values()];
}

// The category of a label, as issue-triage.cjs counts one: its name up to and
// with the colon.
const categoryOf = (label) => label.slice(0, label.indexOf(':') + 1);

// The labels of `matched` their author may have on an issue that carries
// `carried`. With write access, all of them. Without, none that needs it, and
// only the first asked for each category the issue has no label of.
function allowedLabels(matched, carried, canWrite) {
  if (canWrite) return matched.map(m => m.label);
  const filled = new Set(carried.map(categoryOf));
  const allowed = [];
  for (const { label, needsWrite } of matched) {
    const category = categoryOf(label);
    if (needsWrite || filled.has(category)) continue;
    filled.add(category);
    allowed.push(label);
  }
  return allowed;
}

// Adds the labels the event's comment asks for and its author may set, puts
// a thumbs-up on the comment, and returns whether there was any such label.
// A comment on a pull request, a bot's comment, and a comment with no command
// are left alone. The comment's text and its author's association are read
// from the event's payload.
async function runLabelCommands({ github, core, context }) {
  const { issue, comment } = context.payload;
  if (issue.pull_request || comment.user?.type === 'Bot') return false;
  const asked = parseLabelCommands(comment.body);
  if (!asked.length) return false;

  const { owner, repo } = context.repo;
  const issue_number = issue.number;
  const repoLabels = (await github.paginate(github.rest.issues.listLabelsForRepo, {
    owner, repo, per_page: 100,
  })).map(l => l.name);
  const matched = matchLabels(asked, repoLabels);
  if (!matched.length) return false;

  const carried = (await github.paginate(github.rest.issues.listLabelsOnIssue, {
    owner, repo, issue_number, per_page: 100,
  })).map(l => l.name);
  const allowed = allowedLabels(matched, carried, WRITE_ASSOCIATIONS.has(comment.author_association));
  if (!allowed.length) return false;

  const labels = allowed.filter(l => !carried.includes(l));
  if (labels.length) {
    await github.rest.issues.addLabels({ owner, repo, issue_number, labels });
    core.info(`Added from a reply: ${labels.join(', ')}`);
  }
  // The labels are on by now, so a reaction that fails is only said.
  try {
    await github.rest.reactions.createForIssueComment({ owner, repo, comment_id: comment.id, content: '+1' });
  } catch (e) {
    core.warning(`Could not react to the comment: ${e.message}`);
  }
  return true;
}

module.exports = { parseLabelCommands, matchLabels, allowedLabels, runLabelCommands };
