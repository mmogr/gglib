/**
 * Tokens written per second of writing: a request's on the Endpoint page's
 * Recent and in a loaded model's inspector, a reply's on its figure line.
 *
 * Nothing in Rust computes this rate for a request: the proxy records the
 * tokens a turn generated and the milliseconds between its first and last
 * token (`ContextSnapshot.completion_tokens`, `generation_ms`), and the
 * division is made here, once, for every surface. Without both figures, or
 * with no time between them (a turn of one token), there is no rate.
 *
 * @module readings/throughput
 */

import type { ContextSnapshot } from '../types/generated/ContextSnapshot';
import { formatPerSecond } from '../utils/formatPerSecond';

/** `tokens` over `ms` of writing, per second; null without both, or with no time. */
export function tokensPerSecond(tokens: number | null | undefined, ms: number | null | undefined): number | null {
  if (tokens == null || ms == null || ms <= 0) return null;
  return tokens / (ms / 1000);
}

/** A request's rate, from what the proxy recorded of it. */
export function requestRate(request: Pick<ContextSnapshot, 'completion_tokens' | 'generation_ms'>): number | null {
  return tokensPerSecond(request.completion_tokens, request.generation_ms);
}

/** A rate as the page says it: "34 t/s", "8.5 t/s". */
export function rateWords(rate: number): string {
  return `${formatPerSecond(rate)} t/s`;
}

/** The rate of the newest recorded request on `modelName` that has one; `null` when none does. */
export function latestRate(recent: ContextSnapshot[], modelName: string): number | null {
  for (let i = recent.length - 1; i >= 0; i -= 1) {
    if (recent[i].model_name !== modelName) continue;
    const rate = requestRate(recent[i]);
    if (rate !== null) return rate;
  }
  return null;
}
