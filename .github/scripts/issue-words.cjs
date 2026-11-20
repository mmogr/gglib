// What an issue's own words say of its labels: `type:` and `component:` from a
// conventional title such as `fix(cli): …`, `component:` from the repository
// paths its description names, and `platform:` from Windows, macOS or Linux.
// issue-triage.cjs applies the result when an issue is opened and words its
// comment with it. `priority:` and `size:` are not here: each is an estimate
// someone makes after reading the code.
//
// A label is in `apply` only when the words point at exactly one answer and
// the rule that found it agreed with the labels past issues ended with.
// Anything less sure is in `suggest`, which is listed for a person and never
// applied. scripts/backtest_issue_labels.mjs measures every rule here against
// the repository's issues; run it again before changing one.
//
// CommonJS, hence `.cjs`, as fix-issue-rule.cjs is.

'use strict';

const { componentOfPath } = require('./component-paths.cjs');

// `type(scope): ` or `type: ` at the start of a title, lower case as the
// repository writes it, with the `!` of a breaking change allowed.
const TITLE_RE = /^([a-z]+)(?:\(([^)]*)\))?!?:\s/;

// The type words whose label is applied.
const TYPE_WORDS = {
  feat: 'type: feature',
  fix: 'type: bug',
  bug: 'type: bug',
  refactor: 'type: refactor',
  chore: 'type: chore',
  perf: 'type: perf',
  docs: 'type: docs',
};

// The type words whose label is only suggested: issues titled `test:` or
// `ci:` ended with another type too often for the word to decide.
const UNSURE_TYPE_WORDS = {
  test: 'type: test',
  ci: 'type: ci',
};

// The scopes whose label is only suggested. A title's `gui` names the window,
// which is `component: frontend`, as readily as the backend `component: gui`
// is for; and no issue titled `ci` ended with `component: ci`.
const UNSURE_SCOPES = new Set(['ci', 'gui']);

// Each platform's name as it is written, the word that names it, its label,
// and whether the label is applied when a title names that platform alone.
// `Windows` is matched with its capital only, since "windows" is also an
// ordinary word. `Linux` is only suggested: a title that names it is as often
// about where CI runs as about where gglib fails.
const PLATFORMS = [
  { name: 'Windows', word: /\bWindows\b/, label: 'platform: windows', applied: true },
  { name: 'macOS',   word: /\bmacOS\b/i,  label: 'platform: macos',   applied: true },
  { name: 'Linux',   word: /\bLinux\b/i,  label: 'platform: linux',   applied: false },
];

// A run of the characters a path is written in. A URL's path starts with
// `//` here, so it is under no folder of the table.
const PATH_TOKEN_RE = /[A-Za-z0-9_.\/-]+/g;

const NOTHING = Object.freeze({ apply: [], suggest: [], from: '' });

const unique = (items) => [...new Set(items)];

function typeOf(title, has) {
  const m = TITLE_RE.exec(title);
  if (!m) return NOTHING;
  const from = `\`${m[1]}\` in the title`;
  const sure = TYPE_WORDS[m[1]];
  if (sure && has(sure)) return { apply: [sure], suggest: [], from };
  const unsure = UNSURE_TYPE_WORDS[m[1]];
  if (unsure && has(unsure)) return { apply: [], suggest: [unsure], from };
  return NOTHING;
}

// The title's scopes, as written, and those of them that are the name of a
// `component:` label.
function scopesOf(title, has) {
  const m = TITLE_RE.exec(title);
  const written = m && m[2] ? m[2].split(/[,\/]/).map(s => s.trim()).filter(Boolean) : [];
  const labels = unique(written.map(s => `component: ${s}`).filter(has));
  return { written, labels };
}

// The components of the paths `body` names, in the order first named, and how
// many different paths were named in all.
function pathsOf(body, has) {
  const paths = new Map();
  for (const [token] of body.matchAll(PATH_TOKEN_RE)) {
    const path = token.replace(/^\.\//, '').replace(/\.+$/, '');
    const label = componentOfPath(path);
    // A folder named alone, such as `src/`, names nothing in it.
    if (label && has(label) && !path.endsWith('/')) paths.set(path, label);
  }
  return { labels: unique(paths.values()), count: paths.size };
}

function componentOf(title, body, has) {
  const scopes = scopesOf(title, has);
  const one = scopes.written.length === 1 && scopes.labels.length === 1;
  if (one && !UNSURE_SCOPES.has(scopes.written[0])) {
    return { apply: scopes.labels, suggest: [], from: `the title's scope \`${scopes.written[0]}\`` };
  }
  const paths = pathsOf(body, has);
  if (!scopes.labels.length) {
    if (!paths.labels.length) return NOTHING;
    if (paths.labels.length === 1 && paths.count >= 2) {
      return { apply: paths.labels, suggest: [], from: 'the paths the description names' };
    }
    const from = paths.count === 1 ? 'the one path the description names' : 'the paths the description names';
    return { apply: [], suggest: paths.labels, from };
  }
  const named = one ? `the title's scope \`${scopes.written[0]}\`` : 'the title\'s scopes';
  return {
    apply: [],
    suggest: unique([...scopes.labels, ...paths.labels]),
    from: paths.labels.length ? `${named} and the paths the description names` : named,
  };
}

function platformOf(title, body, has) {
  const named = (text) => PLATFORMS.filter(p => p.word.test(text) && has(p.label));
  const inTitle = named(title);
  if (inTitle.length === 1 && inTitle[0].applied) {
    return { apply: [inTitle[0].label], suggest: [], from: `\`${inTitle[0].name}\` in the title` };
  }
  const anywhere = named(`${title}\n${body}`);
  if (!anywhere.length) return NOTHING;
  return { apply: [], suggest: anywhere.map(p => p.label), from: 'the platforms this issue names' };
}

// For each category the words can speak to, `{ apply, suggest, from }`: the
// labels to apply, the candidates to list instead, and where they were read,
// as words that follow "from". Every label is one of `repoLabels`, the names
// of the repository's labels, so none is ever invented. `from` holds only
// words of this file and a scope that is a label's name, never other text of
// the issue.
function deriveLabels({ title, body, repoLabels }) {
  const names = new Set(repoLabels);
  const has = (label) => names.has(label);
  const text = body || '';
  return {
    'type:': typeOf(title, has),
    'component:': componentOf(title, text, has),
    'platform:': platformOf(title, text, has),
  };
}

module.exports = { deriveLabels };
