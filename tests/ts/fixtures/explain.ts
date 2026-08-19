/**
 * A sampling explanation, as `GET /api/models/:id/explain` sends one.
 *
 * Three fields the hand-written mirror made optional are unconditional:
 * `published` is an array — empty on almost every model, never absent —
 * `defaultsOrigin` is a required nullable, and so is `profile`. The mirror's
 * `?:` described a backend that predates those fields and no longer exists,
 * and the fixtures written against it were each describing a slightly
 * different impossible response.
 *
 * `effortSuppressed` stays genuinely optional: it carries
 * `skip_serializing_if`, so a model whose template reads `reasoning_effort`
 * really does omit the key.
 */
import type { SamplingExplanation } from '../../../src/types';
import { resolvedConfig } from './inference';

const BASE: SamplingExplanation = {
  resolved: resolvedConfig(),
  sources: [],
  profile: null,
  isReasoning: false,
  trustClientSampling: false,
  published: [],
  defaultsOrigin: null,
};

/** An explanation with `overrides` applied over "nothing resolved anywhere". */
export function samplingExplanation(
  overrides: Partial<SamplingExplanation> = {},
): SamplingExplanation {
  return { ...BASE, ...overrides };
}
