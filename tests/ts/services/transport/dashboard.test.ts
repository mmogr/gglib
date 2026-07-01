/**
 * Tests for `tokensInUse()`, the proxy dashboard's slot-usage fallback chain.
 *
 * Context: llama-server builds with Multi-Token Prediction ("draft-mtp")
 * enabled send `next_token` as an array of objects instead of a single
 * object. `tokensInUse()` must handle both shapes without throwing, mirroring
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
});
