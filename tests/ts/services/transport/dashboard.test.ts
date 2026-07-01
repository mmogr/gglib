/**
 * Tests for `tokensInUse()`, the proxy dashboard's slot-usage fallback chain.
 *
 * Context: llama-server builds with Multi-Token Prediction ("draft-mtp")
 * enabled send `next_token` as an array of objects instead of a single
 * object, and report prompt usage separately from generation progress via
 * `n_prompt_tokens(_processed)`. `tokensInUse()` must handle both `next_token`
 * shapes without throwing, and must add prompt tokens to `n_decoded` rather
 * than reading `n_decoded` alone — mirroring
 * `gglib_proxy::slots::SlotSnapshot::tokens_in_use()` (Rust) and
 * `proxy_dashboard.rs`'s local reimplementation (CLI).
 */

import { describe, it, expect } from 'vitest';
import { tokensInUse, type SlotSnapshot } from '../../../../src/services/transport/types/dashboard';

function slot(overrides: Partial<SlotSnapshot>): SlotSnapshot {
  return { id: 0, is_processing: false, ...overrides };
}

describe('tokensInUse', () => {
  it('prefers n_past when present', () => {
    expect(tokensInUse(slot({ n_past: 512, cache_tokens: 1, next_token: { n_decoded: 2 } }))).toBe(
      512,
    );
  });

  it('falls back to cache_tokens when n_past is absent', () => {
    expect(tokensInUse(slot({ cache_tokens: 256, next_token: { n_decoded: 2 } }))).toBe(256);
  });

  it('falls back to next_token.n_decoded when it is a single object', () => {
    expect(tokensInUse(slot({ next_token: { n_decoded: 89 } }))).toBe(89);
  });

  it('falls back to next_token[0].n_decoded when next_token is an MTP array', () => {
    expect(tokensInUse(slot({ next_token: [{ n_decoded: 89 }, { n_decoded: 999 }] }))).toBe(89);
  });

  it('returns null when no candidate field is present', () => {
    expect(tokensInUse(slot({}))).toBeNull();
  });

  it('returns null when next_token is an empty MTP array', () => {
    expect(tokensInUse(slot({ next_token: [] }))).toBeNull();
  });

  it('adds n_prompt_tokens_processed to n_decoded rather than using n_decoded alone', () => {
    // Real payload shape that previously showed ~0% used for a 20k+-token
    // prompt: n_decoded (89) alone is not the total context in use.
    expect(
      tokensInUse(
        slot({
          n_prompt_tokens: 20994,
          n_prompt_tokens_processed: 20906,
          next_token: [{ n_decoded: 89 }],
        }),
      ),
    ).toBe(20906 + 89);
  });

  it('prefers n_prompt_tokens_processed over n_prompt_tokens when both present', () => {
    expect(
      tokensInUse(
        slot({ n_prompt_tokens: 500, n_prompt_tokens_processed: 300, next_token: { n_decoded: 10 } }),
      ),
    ).toBe(310);
  });

  it('falls back to n_prompt_tokens when n_prompt_tokens_processed is absent', () => {
    expect(tokensInUse(slot({ n_prompt_tokens: 500, next_token: { n_decoded: 10 } }))).toBe(510);
  });

  it('defaults the n_decoded contribution to 0 when generation has not started yet', () => {
    expect(tokensInUse(slot({ n_prompt_tokens: 500, n_prompt_tokens_processed: 250 }))).toBe(250);
  });

  it('does not add n_decoded onto the legacy n_past/cache_tokens branch', () => {
    expect(tokensInUse(slot({ n_past: 512, next_token: { n_decoded: 89 } }))).toBe(512);
  });

  it('adds n_prompt_tokens_cache to the processed delta on a KV-cache-reuse hit', () => {
    // Real-world scenario: a follow-up prompt where llama-server found a
    // large cached prefix match and only newly processed a small delta.
    // n_prompt_tokens_cache must be added to n_prompt_tokens_processed, or
    // context usage falsely collapses to just the tiny newly-processed
    // delta (the exact regression this test guards against).
    expect(
      tokensInUse(
        slot({
          n_prompt_tokens: 7981,
          n_prompt_tokens_processed: 1245,
          n_prompt_tokens_cache: 6736,
          next_token: [{ n_decoded: 12 }],
        }),
      ),
    ).toBe(1245 + 6736 + 12);
  });

  it('does not double-count n_prompt_tokens_cache against the n_prompt_tokens grand total', () => {
    // n_prompt_tokens (the fallback used when _processed is absent) already
    // includes any cached prefix, so cache must not be added on top of it.
    expect(
      tokensInUse(
        slot({
          n_prompt_tokens: 500,
          n_prompt_tokens_cache: 400,
          next_token: { n_decoded: 10 },
        }),
      ),
    ).toBe(510);
  });
});


