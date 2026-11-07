/**
 * What the Load with options sheet sends, from what the person typed in it:
 * pure, so every rule is pinned without a screen.
 *
 * A launch carries only typed values (IN-7): an empty field sends no key,
 * Auto sends no `jinja`, an untouched Speculative sends no draft counts, and
 * the daemon resolves the rest as `gglib model explain` says it would. The
 * sampling half is spread whole, so every `InferenceConfig` key the form
 * offers reaches the request (IN-6). With "This load only" unchecked the
 * typed values that have a per-model home (context, Jinja, MTP on or off,
 * sampling) are also saved, each over the stored rest; the draft counts,
 * the port and the pin have none and ride the launch only.
 *
 * @module inspector/loadOptions
 */

import type { ModelUpdates } from '../services/modelRegistry';
import type { GgufModel, ServeConfig, SparseInferenceConfig } from '../types';
import type { LaunchArg } from '../types/generated/LaunchArg';
import { stored } from './local/Change';

/** What the person typed in the sheet; an empty string or `null` is untouched. */
export interface Typed {
  context: string;
  /** `null` is Auto. */
  jinja: boolean | null;
  /** `null` is the plan's own; `false` turns drafting off for this launch. */
  mtp: boolean | null;
  nMax: string;
  pMin: string;
  sampling: SparseInferenceConfig;
  port: string;
  pin: boolean;
  thisLoadOnly: boolean;
}

export const UNTOUCHED: Typed = {
  context: '',
  jinja: null,
  mtp: null,
  nMax: '',
  pMin: '',
  sampling: {},
  port: '',
  pin: false,
  thisLoadOnly: false,
};

/** The sheet's fields, by the plan decision each holds. */
export type SheetField = 'ctx' | 'templates' | 'mtp' | 'sampling' | 'reasoning' | 'port';

/** The decisions a launch-line token can open the sheet on. */
export const SHEET_FIELDS: ReadonlySet<string> = new Set<SheetField>(['ctx', 'templates', 'mtp', 'sampling', 'reasoning', 'port']);

const whole = (text: string): number | null => {
  const n = Number(text.trim());
  return text.trim() !== '' && Number.isInteger(n) ? n : null;
};

/** The sampling values typed, empties dropped. */
function samplingTyped(sampling: SparseInferenceConfig): SparseInferenceConfig {
  return Object.fromEntries(Object.entries(sampling).filter(([, v]) => v != null)) as SparseInferenceConfig;
}

/** The plan's value for `flag` among `args`, the placeholder a typed field replaces. */
export const planValue = (args: readonly LaunchArg[], flag: string): string | undefined => args.find((a) => a.flag === flag)?.value;

/**
 * What stops a load, in a sentence, or `null`. `mtpOn` is whether the
 * launch would draft without the sheet (the plan's mtp arguments).
 */
export function problem(typed: Typed, mtpOn: boolean): string | null {
  const ctx = whole(typed.context);
  if (typed.context.trim() !== '' && (ctx === null || ctx < 1)) return 'Context is a whole number above 0.';
  const port = whole(typed.port);
  if (typed.port.trim() !== '' && (port === null || port < 1024 || port > 65535)) return 'Port is a whole number from 1024 to 65535.';
  const n = whole(typed.nMax);
  if (typed.nMax.trim() !== '' && (n === null || n < 1)) return 'Draft tokens is a whole number above 0.';
  const p = Number(typed.pMin);
  if (typed.pMin.trim() !== '' && !(p >= 0 && p <= 1)) return 'Min probability is from 0 to 1.';
  // On for this launch alone, where nothing else turns it on, needs a count: the stored default cannot.
  if (typed.mtp === true && !mtpOn && typed.thisLoadOnly && n === null) return 'Say how many draft tokens for this load.';
  return null;
}

/** The launch request: the model's id and only what was typed. Call after `problem` says nothing. */
export function launchConfig(model: Pick<GgufModel, 'id'>, typed: Typed): ServeConfig {
  const config: ServeConfig = { ...samplingTyped(typed.sampling), id: model.id, mlock: false };
  const ctx = whole(typed.context);
  if (ctx !== null) config.contextLength = ctx;
  const port = whole(typed.port);
  if (port !== null) config.port = port;
  if (typed.jinja !== null) config.jinja = typed.jinja;
  if (typed.mtp === false) {
    config.mtpDraftNMax = 0;
  } else {
    const n = whole(typed.nMax);
    if (n !== null) config.mtpDraftNMax = n;
    if (typed.pMin.trim() !== '') config.mtpDraftPMin = Number(typed.pMin);
  }
  return config;
}

/**
 * The model's defaults to save before the load, with "This load only"
 * unchecked: only the fields typed, each over the stored rest. `null` when
 * the box is checked or nothing typed has a per-model home.
 */
export function defaultsPatch(model: GgufModel, typed: Typed): ModelUpdates | null {
  if (typed.thisLoadOnly) return null;
  const patch: ModelUpdates = {};
  const ctx = whole(typed.context);
  const server = {
    ...(ctx !== null && { contextLength: ctx }),
    ...(typed.jinja !== null && { jinja: typed.jinja }),
    ...(typed.mtp !== null && { mtp: typed.mtp }),
  };
  if (Object.keys(server).length > 0) patch.serverDefaults = { ...stored(model), ...server };
  const sampling = samplingTyped(typed.sampling);
  if (Object.keys(sampling).length > 0) patch.inferenceDefaults = { ...(model.inferenceDefaults ?? {}), ...sampling };
  return Object.keys(patch).length > 0 ? patch : null;
}
