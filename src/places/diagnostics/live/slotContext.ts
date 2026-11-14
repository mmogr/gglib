/**
 * The context one llama.cpp slot holds, as `gglib proxy dashboard` prints
 * it slot by slot: the tokens in it (the proxy's `tokens_in_use`) of the
 * slot's own `n_ctx`, and the percent. Never summed across slots: no Rust
 * adds them.
 *
 * @module places/diagnostics/live/slotContext
 */

import type { SlotSnapshot } from '../../../services/transport/types/dashboard';
import { usagePercent } from '../../../utils/contextUsage';
import { formatCount } from '../../../utils/format';

export interface SlotContext {
  used: number;
  size: number;
  /** `used` of `size`, whole; never over 100. */
  percent: number;
}

/** The slot's context, or `null` while its tokens or its size are not known. */
export function slotContext(slot: SlotSnapshot): SlotContext | null {
  const used = slot.tokens_in_use;
  const size = slot.n_ctx;
  if (used === null || size === null || size <= 0) return null;
  return { used, size, percent: usagePercent(used, size) };
}

/** "1,024 of 4,096 tokens · 25%". */
export function slotContextWords(c: SlotContext): string {
  return `${formatCount(c.used)} of ${formatCount(c.size)} tokens · ${c.percent}%`;
}
