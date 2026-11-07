/**
 * How a model runs, read from its explanation: pure, so every sentence is
 * pinned without a screen. The rows are the plan's arguments grouped by
 * `LaunchArg.decision` (the wire's own grouping key), labelled by the one
 * table here; a row says the plan's narration where it has a decision of
 * that label (the words How it ran uses), else its arguments; the fit sentence and the Context row's note are composed from
 * `ContextFit`'s figures; the Sampling and Reasoning rows count the
 * explanation's sources. Nothing is worked out that the daemon did not say.
 *
 * @module inspector/local/rows
 */

import { CHOICE_WORDS } from '../../components/status/words';
import type { ProvenanceParamKey, SamplingExplanation, SamplingParamKey } from '../../types';
import type { ChoiceSource } from '../../types/generated/ChoiceSource';
import type { ContextFit } from '../../types/generated/ContextFit';
import type { LaunchArg } from '../../types/generated/LaunchArg';
import type { LaunchNarration } from '../../types/generated/LaunchNarration';
import type { LaunchPlan } from '../../types/generated/LaunchPlan';
import { formatSize } from '../../utils/format';
import { PARAM_LABELS, describeSource, formatProvenanceValue, publishedByParam } from '../../utils/samplingProvenance';

/**
 * The label for each decision key, in the order the rows are drawn: a
 * launch's arguments (`LaunchArg.decision`) and a launch's narration
 * (`LaunchDecision.label`) share the keys. `null` is a decision the launch
 * line shows and no row does (the plumbing: weights, host, port). A key not
 * here is drawn under its own name.
 */
export const DECISION_LABELS: Readonly<Record<string, string | null>> = {
  ctx: 'Context',
  images: 'Images',
  templates: 'Templates',
  flags: 'Templates',
  sampling: 'Sampling',
  reasoning: 'Reasoning',
  mtp: 'Speculative',
  kv: 'KV cache',
  cache: 'Prompt cache',
  dialect: 'Tool calls',
  backend: 'Backend',
  embeddings: 'Embeddings',
  mlock: 'Memory',
  parts: 'Parts',
  runtime: 'Runtime',
  slot: 'Slot',
  recipe: null,
  model: null,
  host: null,
  port: null,
  metrics: null,
  parallel: null,
};

/** A decision's label: the table's, else its key. */
export const labelOf = (key: string): string => DECISION_LABELS[key] ?? key;

/** One row of How it runs: a decision and the arguments that make it. */
export interface RunRow {
  key: string;
  label: string;
  args: LaunchArg[];
}

const ORDER = Object.keys(DECISION_LABELS);
const rank = (key: string) => (ORDER.includes(key) ? ORDER.indexOf(key) : ORDER.length);

/**
 * The rows for `plan`: one per decision with a label, in the table's
 * order. A chat model always has Images (None when no projector is linked),
 * Templates (Auto when gglib sends no flag either way), Sampling and
 * Reasoning; an image model has its Parts and nothing of a chat's. `also`
 * adds rows the plan has no argument for (Speculative turned off by the
 * person, so it can be seen and reset).
 */
export function runRows(plan: Pick<LaunchPlan, 'args' | 'fit'>, draws: boolean, also: readonly string[] = []): RunRow[] {
  const byKey = new Map<string, LaunchArg[]>();
  for (const arg of plan.args) {
    if (DECISION_LABELS[arg.decision] === null) continue;
    byKey.set(arg.decision, [...(byKey.get(arg.decision) ?? []), arg]);
  }
  const wanted = draws ? ['parts', ...also] : ['images', 'templates', 'sampling', 'reasoning', ...also];
  for (const key of wanted) if (!byKey.has(key)) byKey.set(key, []);
  return [...byKey]
    .map(([key, args]) => ({ key, label: labelOf(key), args }))
    .sort((a, b) => rank(a.key) - rank(b.key));
}

/** The last part of a path, for a value that names a file. */
export const fileName = (value: string): string => (value.includes('/') ? (value.split('/').pop() ?? value) : value);

/** A whole number grouped (`131,072`); any other value as sent. */
export const grouped = (value: string): string => (/^\d+$/.test(value) ? Number(value).toLocaleString() : value);

/** An argument's value for a person: a whole number grouped, a path as its file's name. */
const readable = (value: string): string => fileName(grouped(value));

/** A row's arguments as its value: one argument's value (or its flag), several as `flag value` pairs. */
export function argsValue(args: LaunchArg[]): string {
  if (args.length === 1) return args[0].value == null ? args[0].flag : readable(args[0].value);
  return args.map((a) => (a.value == null ? a.flag : `${a.flag} ${readable(a.value)}`)).join(' · ');
}

/** The narration's label for a row: the same key, but templates are narrated as `flags`. */
const narratedAs = (key: string) => (key === 'templates' ? 'flags' : key);

/** What the narration says of a row's decision, in Rust's words; `null` when it says nothing. */
export function narrated(narration: LaunchNarration | null | undefined, key: string): string | null {
  const said = narration?.decisions.find((d) => d.label === narratedAs(key));
  return said ? grouped(said.value) : null;
}

/** What a hover says of a row: whose each choice was, and why. */
export function argsWhy(args: LaunchArg[]): string {
  return [...new Set(args.map((a) => `${CHOICE_WORDS[a.source]}: ${a.reason}`))].join('\n');
}

const SOURCE_ORDER: ChoiceSource[] = ['model_author', 'gglib', 'you'];

/** The caption over the rows: whose choices they are, from the arguments' sources. */
export function chosenBy(args: LaunchArg[]): string {
  const sources = SOURCE_ORDER.filter((s) => args.some((a) => a.source === s));
  return sources.length === 0 ? '' : `chosen by ${sources.map((s) => CHOICE_WORDS[s]).join(', ')}`;
}

/** A context size: `32K` when it is whole thousands of 1,024, else the number. */
export const tokens = (n: number): string => (n % 1024 === 0 ? `${n / 1024}K` : n.toLocaleString());

/** What a refused fit was missing, from the figures that are `null`. */
function missing(fit: ContextFit): string {
  const gaps = [
    fit.budgetBytes === null && "this Mac's memory could not be read",
    fit.weightsBytes === null && "the weights' size is not known",
    fit.kvBytesPerToken === null && 'the model does not say what its context costs',
  ].filter(Boolean);
  return gaps.length > 0 ? `: ${gaps.join(', ')}` : '';
}

/**
 * The fit sentence: the context the model would be served at, of the window
 * it was trained for, and whether it fits this machine. `null` for an image
 * model, which has no context.
 */
export function fitSentence(fit: ContextFit | null): string | null {
  if (!fit) return null;
  const trained = fit.trainedCtx ? `, of ${tokens(fit.trainedCtx)} trained` : '';
  const at = tokens(fit.servesAt);
  if (fit.fitted === null) return `Serves at ${at} context${trained}; gglib could not fit one to this Mac${missing(fit)}.`;
  if (fit.servesAt <= fit.fitted) return `Fits this Mac with ${at} context${trained}.`;
  return `Serves at ${at} context${trained}; ${tokens(fit.fitted)} fits this Mac.`;
}

/**
 * The Context row's note: what the served context needs (the weights and a
 * KV cache that size) against the budget, when the fit knew all three.
 */
export function contextNote(fit: ContextFit | null): string | null {
  if (!fit || fit.budgetBytes === null || fit.weightsBytes === null || fit.kvBytesPerToken === null) return null;
  const need = fit.weightsBytes + fit.kvBytesPerToken * fit.servesAt;
  const word = fit.fitted !== null && fit.servesAt <= fit.fitted ? 'fits' : 'does not fit';
  return `${word} (${formatSize(need)} of ${formatSize(fit.budgetBytes)})`;
}

/** The fifteen sampler fields the Sampling row counts. */
export const SAMPLING_KEYS: readonly SamplingParamKey[] = [
  'temperature', 'topP', 'topK', 'maxTokens', 'repeatPenalty', 'presencePenalty', 'minP', 'frequencyPenalty',
  'dynatempRange', 'dynatempExponent', 'topNSigma', 'dryMultiplier', 'dryBase', 'dryAllowedLength', 'dryPenaltyLastN',
];

/** The two reasoning fields the Reasoning row says. */
export const REASONING_KEYS: readonly ProvenanceParamKey[] = ['reasoningEffort', 'reasoningBudgetTokens'];

/**
 * The values `keys` resolve to, grouped by where each came from, and how
 * many resolve to nothing gglib sends (llama.cpp's own default applies). A
 * value the model's GGUF publishes and gglib defers to is the model
 * author's.
 */
export function settingsLine(sampling: SamplingExplanation, keys: readonly ProvenanceParamKey[]): { set: string; others: number } {
  const published = publishedByParam(sampling.published);
  const ctx = { profile: sampling.profile, defaultsOrigin: sampling.defaultsOrigin };
  const groups = new Map<string, string[]>();
  const add = (source: string, value: string) => groups.set(source, [...(groups.get(source) ?? []), value]);
  let others = 0;
  for (const entry of sampling.sources) {
    const param = entry.param as ProvenanceParamKey;
    if (!keys.includes(param)) continue;
    const label = PARAM_LABELS[param] ?? param;
    const author = published.get(param);
    if (entry.kind === 'layer') add(describeSource(entry, ctx), `${label} ${formatProvenanceValue(param, sampling.resolved)}`);
    else if (author?.state === 'deferred') add(CHOICE_WORDS.model_author, `${label} ${Number(author.published.toPrecision(6))}`);
    else others += 1;
  }
  const set = [...groups].map(([source, values]) => `${values.join(' · ')} from ${source}`).join('; ');
  return { set, others };
}

/** The Sampling row's value: the set values by source, then a count of the rest. */
export function samplingValue(sampling: SamplingExplanation): string {
  const { set, others } = settingsLine(sampling, SAMPLING_KEYS);
  const rest = others === 0 ? '' : `${others} ${set ? (others === 1 ? 'other uses' : 'others use') : 'use'} llama.cpp's defaults`;
  return [set, rest].filter(Boolean).join('; ');
}

/** Whether the person set one of `keys` on this model (its per-model layer). */
export function setByYou(sampling: SamplingExplanation | null, keys: readonly ProvenanceParamKey[]): boolean {
  return (sampling?.sources ?? []).some(
    (e) => keys.includes(e.param as ProvenanceParamKey) && e.kind === 'layer' && e.layer === 'modelUserSet',
  );
}
