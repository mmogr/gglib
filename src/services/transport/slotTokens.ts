/**
 * How many tokens a llama-server slot is actually holding.
 *
 * Lives in its own module because it is the one piece of *behaviour* that grew
 * up among the proxy dashboard's type declarations. Those declarations are
 * being replaced by generated ones; this cannot be, so separating them first
 * keeps a mechanical swap from having to step around live logic.
 *
 * # The contract this reads, and why it is a mirror rather than a shared type
 *
 * The frontend connects directly to an already-running proxy's own HTTP port
 * (`http://{host}:{port}/v1/proxy/status/stream`), the same way the CLI's
 * `gglib proxy dashboard` command does — a real HTTP client of the JSON
 * contract, not a shared in-process type. Unknown or extra fields are ignored
 * by TypeScript's structural typing, so a reader tolerates additive
 * server-side changes the same way the CLI's `serde(default)` does.
 *
 * @module services/transport/slotTokens
 */

import type { SlotSnapshot } from './types/dashboard';

/**
 * Same additive logic as `SlotSnapshot::tokens_in_use()` (Rust) and
 * `proxy_dashboard.rs`'s local reimplementation (CLI) — kept in sync by hand,
 * since it's a tiny amount of logic mirrored across three consumers.
 *
 * Current-schema builds report prompt usage and generation progress as two
 * separate counters — `n_prompt_tokens(_processed)` and `next_token.n_decoded`
 * — which must be added together to get the true total (a 20k-token prompt
 * with 89 tokens generated so far is ~20k tokens in use, not 89).
 * `n_prompt_tokens_processed` is preferred over `n_prompt_tokens` when both
 * are present (it tracks real progress mid-prefill) and, when present, is
 * combined with `n_prompt_tokens_cache` (tokens reused from KV cache this
 * round, not re-processed) — otherwise a cache-hit follow-up prompt would
 * falsely collapse context usage down to just the tiny newly-processed
 * delta. The grand-total `n_prompt_tokens` fallback (used only when
 * `_processed` is absent) already includes any cached prefix, so cache is
 * NOT added on top of it. Only when neither prompt-side field is present
 * does this fall back to the legacy, non-additive chain: `n_past`, then
 * `cache_tokens`, then `n_decoded` alone.
 *
 * `next_token` may be a single object or an array (MTP builds); element 0 is
 * the accepted/main decode stream when it's an array.
 */
export function tokensInUse(slot: SlotSnapshot): number | null {
  const nextToken = Array.isArray(slot.next_token) ? slot.next_token[0] : slot.next_token;
  const nDecoded = nextToken?.n_decoded ?? undefined;

  const promptComponent =
    slot.n_prompt_tokens_processed != null
      ? slot.n_prompt_tokens_processed + (slot.n_prompt_tokens_cache ?? 0)
      : slot.n_prompt_tokens;

  if (promptComponent != null) {
    return promptComponent + (nDecoded ?? 0);
  }

  return slot.n_past ?? slot.cache_tokens ?? nDecoded ?? null;
}
