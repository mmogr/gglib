/**
 * How fast a model wrote, as every surface says it: a recent request's
 * rate on the Endpoint page's Recent, in a loaded model's inspector and
 * beside its log, and a reply's on its figure line.
 *
 * The rate is the daemon's, served: `tokens_per_second` on a recorded
 * request (`ContextSnapshot`) and on a turn's `turn_usage` frame and saved
 * row (`TurnMade.tokensPerSecond`), computed once in Rust
 * (`gglib_core::domain::figures::tokens_per_second`). Nothing here divides.
 * The words are `rate_words`', which `gglib proxy dashboard` and `gglib
 * chat` print, held to `contracts/proxy/figures.json` and
 * `contracts/runs/turn_made.json`.
 *
 * @module readings/rate
 */

import type { ContextSnapshot } from '../types/generated/ContextSnapshot';

/** A rate as every surface says it: "34 t/s", "8.5 t/s"; under ten keeps one decimal, else whole. */
export function rateWords(rate: number): string {
  return `${rate < 10 ? rate.toFixed(1) : String(Math.round(rate))} t/s`;
}

/** The rate of the newest recorded request on `modelName` that has one; `null` when none does. */
export function latestRate(recent: ContextSnapshot[], modelName: string): number | null {
  for (let i = recent.length - 1; i >= 0; i -= 1) {
    const rate = recent[i].tokens_per_second;
    if (recent[i].model_name === modelName && rate != null) return rate;
  }
  return null;
}
