/**
 * How a reply's turn was made: one shape for the `turn_usage` event a run
 * streams and for the metadata the daemon saves from that same event, so a
 * reply just drawn and the same reply loaded say the same.
 *
 * The keys are the saved row's (`MADE_KEYS` in `gglib-core`'s replay). Each
 * is present only when the turn had it: absent is not zero.
 *
 * @module turnMade
 */

export interface TurnMade {
  modelName?: string;
  modelQuantization?: string;
  /** Tokens read, cached ones included. */
  promptTokens?: number;
  /** Of `promptTokens`, those from the KV cache. */
  cachedTokens?: number;
  /** Tokens written. */
  completionTokens?: number;
  turnDurationMs?: number;
  /** From the first thing written to the end. */
  writingDurationMs?: number;
}

const TEXT_KEYS = ['modelName', 'modelQuantization'] as const;
const COUNT_KEYS = [
  'promptTokens',
  'cachedTokens',
  'completionTokens',
  'turnDurationMs',
  'writingDurationMs',
] as const;

/** The figures a saved row's metadata holds; `undefined` when it holds none. */
export function turnMadeFromMetadata(metadata: Record<string, unknown> | null | undefined): TurnMade | undefined {
  if (!metadata) return undefined;
  const made: TurnMade = {};
  for (const key of TEXT_KEYS) {
    const value = metadata[key];
    if (typeof value === 'string' && value) made[key] = value;
  }
  for (const key of COUNT_KEYS) {
    const value = metadata[key];
    if (typeof value === 'number' && Number.isFinite(value)) made[key] = value;
  }
  return Object.keys(made).length > 0 ? made : undefined;
}

/** A `turn_usage` event, as the saved row will say it. */
export interface TurnUsageWire {
  model?: string;
  quantization?: string;
  prompt_tokens?: number;
  cached_tokens?: number;
  completion_tokens?: number;
  duration_ms?: number;
  writing_ms?: number;
}

/** The figures a `turn_usage` event carries, under the saved row's keys. */
export function turnMadeFromUsage(event: TurnUsageWire): TurnMade | undefined {
  return turnMadeFromMetadata({
    modelName: event.model,
    modelQuantization: event.quantization,
    promptTokens: event.prompt_tokens,
    cachedTokens: event.cached_tokens,
    completionTokens: event.completion_tokens,
    turnDurationMs: event.duration_ms,
    writingDurationMs: event.writing_ms,
  });
}
