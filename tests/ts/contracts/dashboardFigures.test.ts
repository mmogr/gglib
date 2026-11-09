/**
 * The dashboard's figures are the proxy's, said as `gglib proxy dashboard`
 * says them.
 *
 * `contracts/proxy/figures.json` is written by `gglib-proxy`'s dashboard
 * figures tests from known traffic: recent requests with the rate the proxy
 * computed for each, beside the words `rate_words` says it in, the cache's
 * reuse with its percents and its sentence, and the loop-guard trips total.
 * The page must say the same rate words, show the sentence verbatim, and
 * read the total, dividing and summing nothing itself.
 */

import { describe, it, expect } from 'vitest';
import { activitySentences, recentRows } from '../../../src/places/endpoint/lines';
import { latestRate, rateWords } from '../../../src/readings/rate';
import type { CacheUsage } from '../../../src/types/generated/CacheUsage';
import type { ContextSnapshot } from '../../../src/types/generated/ContextSnapshot';
import { dashboardSnapshot } from '../fixtures/dashboard';
import { rust } from './rustSource';

const FIGURES = JSON.parse(rust('contracts/proxy/figures.json')) as {
  recent_requests: ContextSnapshot[];
  rate_words: (string | null)[];
  cache_usage: CacheUsage;
  tool_repairs_succeeded: number;
  loop_guard_trips_total: number;
};

const FRAME = dashboardSnapshot({
  recent_requests: FIGURES.recent_requests,
  loop_guard_trips_total: FIGURES.loop_guard_trips_total,
  cache: {
    disk_enabled: false,
    disk_suppressed_for_model: false,
    ram_budget_mb: null,
    ram_state: 'llama_default',
    needs_attention: false,
    warnings: [],
    usage: FIGURES.cache_usage,
  },
});

describe('dashboard figures contract', () => {
  it('says each recorded rate in the words the proxy recorded beside it', () => {
    const said = FIGURES.recent_requests.map((r) => (r.tokens_per_second != null ? rateWords(r.tokens_per_second) : null));
    expect(said).toEqual(FIGURES.rate_words);
    expect(said).toEqual(['34 t/s', '8.5 t/s', null, null, '10 t/s', '8.3 t/s']);
  });

  it("shows Recent's figures from the recorded requests, newest first, a rate only where the proxy sent one", () => {
    expect(recentRows(FRAME, 6).map((r) => `${r.model} ${r.figures}`.trim())).toEqual([
      'phi-4 33 tokens · 8.3 t/s',
      'phi-4 100 tokens · 10 t/s',
      'Qwen3-8B',
      'Qwen3-8B 1 token',
      'gemma-3-12b 1,200 tokens · 8.5 t/s',
      'Qwen3-8B 812 tokens · 34 t/s',
    ]);
  });

  it("names a model's newest recorded rate, passing over requests with none", () => {
    expect(latestRate(FIGURES.recent_requests, 'Qwen3-8B')).toBe(FIGURES.recent_requests[0].tokens_per_second);
    expect(latestRate(FIGURES.recent_requests, 'gemma-3-12b')).toBe(FIGURES.recent_requests[1].tokens_per_second);
    expect(latestRate(FIGURES.recent_requests, 'phi-4')).toBe(FIGURES.recent_requests[5].tokens_per_second);
    expect(latestRate(FIGURES.recent_requests, 'Mistral-7B')).toBeNull();
  });

  it("says the cache's reuse in the proxy's sentence and the trips in its total", () => {
    expect(activitySentences(FRAME)).toEqual(['Prompt cache 66% reused', '3 loop-guard trips']);
    expect(FIGURES.cache_usage.summary).toBe('Prompt cache 66% reused');
  });

  it('says no trips sentence when the proxy counted none', () => {
    expect(activitySentences({ ...FRAME, loop_guard_trips_total: 0 })).toEqual(['Prompt cache 66% reused']);
  });
});
