/**
 * A sampling explanation, as `GET /api/models/:id/explain` sends one.
 *
 * Three fields the hand-written mirror made optional are unconditional:
 * `published` is an array — empty on almost every model, never absent —
 * `defaultsOrigin` is a required nullable, and so is `profile`. The mirror's
 * `?:` described a backend that predates those fields and no longer exists,
 * and the fixtures written against it were each describing a slightly
 * different impossible response.
 */
import type { LaunchExplanation, SamplingExplanation } from '../../../src/types';
import type { LaunchPlan } from '../../../src/types/generated/LaunchPlan';
import { resolvedConfig } from './inference';

const BASE: SamplingExplanation = {
  resolved: resolvedConfig(),
  sources: [],
  profile: null,
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

/**
 * A whole explanation around `sampling`: by default the launch half is the
 * smallest plan the daemon can send, a program with no arguments and no
 * fit, for a test that reads only the sampling.
 */
export function launchExplanation(
  sampling: SamplingExplanation,
  plan: LaunchPlan = {
    program: 'llama-server',
    args: [],
    narration: { model_name: 'model', quantization: null, weights_bytes: 0, decisions: [] },
    fit: null,
  },
): LaunchExplanation {
  return { plan, sampling };
}

/**
 * A chat model's plan as the runtime makes one (`explain_args.rs`'s
 * decisions and reasons): its weights, the port, a context fitted to the
 * machine, templates for a model tagged agent, a linked projector and a
 * quantized KV cache, fitted at 32K of a 128K window.
 */
export function chatPlan(overrides: Partial<LaunchPlan> = {}): LaunchPlan {
  return {
    program: 'llama-server',
    args: [
      { flag: '-m', value: '/models/gemma.gguf', decision: 'model', source: 'model_author', reason: 'the weights, as published' },
      { flag: '--port', value: 'auto', decision: 'port', source: 'gglib', reason: 'the next free port above the base when it loads' },
      { flag: '-c', value: '32768', decision: 'ctx', source: 'gglib', reason: 'the largest rung that fits beside the weights on this machine' },
      { flag: '--jinja', decision: 'templates', source: 'gglib', reason: "tagged agent: tool calls need the model's own template" },
      { flag: '--mmproj', value: '/models/mmproj-F16.gguf', decision: 'images', source: 'you', reason: 'the projector linked to this model' },
      { flag: '--cache-type-k', value: 'q8_0', decision: 'kv', source: 'gglib', reason: 'q8_0 halves the KV cache at no measured cost' },
      { flag: '--cache-type-v', value: 'q8_0', decision: 'kv', source: 'gglib', reason: 'q8_0 halves the KV cache at no measured cost' },
    ],
    narration: { model_name: 'gemma', quantization: 'Q4_K_M', weights_bytes: 2_500_000_000, decisions: [] },
    // The first fit `contracts/models/fit_sentence.json` records, words and
    // all (`tests/ts/contracts/fitSentence.test.ts` holds it there).
    fit: {
      servesAt: 32768,
      source: 'fitted_to_hardware',
      fitted: 32768,
      budgetBytes: 48 * 1024 ** 3,
      weightsBytes: 2_500_000_000,
      kvBytesPerToken: 1_097_000,
      kvBytes: 35_946_496_000,
      trainedCtx: 131072,
      unsnapped: 40005,
      sentence: 'Fits this machine with 32K context, of 128K trained.',
      note: 'fits (35.81 GiB of 43.20 GiB usable)',
      figures: [
        { label: 'memory', value: 'fits (35.81 GiB of 43.20 GiB usable)', source: null },
        { label: 'device budget', value: '48.00 GiB', source: null },
        { label: 'weights', value: '2.33 GiB', source: null },
        { label: 'kv bytes/token', value: '1097000', source: null },
        { label: 'kv cache', value: '33.48 GiB', source: null },
        { label: 'trained window', value: '131072', source: null },
        { label: 'before snapping', value: '40005', source: null },
      ],
    },
    ...overrides,
  };
}
