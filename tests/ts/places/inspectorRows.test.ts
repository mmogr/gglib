/**
 * `inspector/local/rows`: which decisions become rows and in what order,
 * how a row's arguments read, and how the Sampling row counts what the
 * daemon resolved.
 */

import { describe, it, expect } from 'vitest';

import {
  argsValue,
  chosenBy,
  narrated,
  runRows,
  samplingValue,
  setByYou,
  SAMPLING_KEYS,
} from '../../../src/inspector/local/rows';
import type { LaunchArg } from '../../../src/types/generated/LaunchArg';
import { chatPlan, samplingExplanation } from '../fixtures/explain';
import { resolvedConfig } from '../fixtures/inference';

const arg = (flag: string, decision: string, value?: string, source: LaunchArg['source'] = 'gglib'): LaunchArg => ({
  flag,
  value,
  decision,
  source,
  reason: 'why',
});

describe('the rows', () => {
  it('are the labelled decisions in the table order, the plumbing left to the launch line', () => {
    const rows = runRows(chatPlan(), false);
    expect(rows.map((r) => r.label)).toEqual(['Context', 'Images', 'Templates', 'Sampling', 'Reasoning', 'KV cache']);
    expect(rows.find((r) => r.key === 'kv')?.args).toHaveLength(2);
  });

  it('always have Templates for a chat model, and the rows asked for that have no argument', () => {
    const plan = { args: chatPlan().args.filter((a) => a.decision !== 'templates'), fit: null };
    const rows = runRows(plan, false, ['mtp']);
    expect(rows.find((r) => r.key === 'templates')?.args).toEqual([]);
    expect(rows.map((r) => r.label)).toContain('Speculative');
  });

  it("say the narration's value for their decision, a whole number grouped", () => {
    const narration = {
      model_name: 'm', quantization: null, weights_bytes: 0,
      decisions: [
        { label: 'ctx', value: '131072', source: null },
        { label: 'flags', value: '--jinja (agent tag)', source: null },
      ],
    };
    expect(narrated(narration, 'ctx')).toBe('131,072');
    // The narration's `flags` line holds more than Templates' flag, so Templates never reads it.
    expect(narrated(narration, 'templates')).toBeNull();
    expect(narrated(narration, 'kv')).toBeNull();
    expect(argsValue([arg('-c', 'ctx', '131072')])).toBe('131,072');
  });

  it('are Parts alone for an image model, and keep a decision the table does not know under its own name', () => {
    const plan = { args: [arg('--vae', 'parts', '/m/ae.safetensors'), arg('--steps', 'recipe', '4'), arg('--new', 'novel')], fit: null };
    expect(runRows(plan, true).map((r) => r.label)).toEqual(['Parts', 'novel']);
    expect(runRows(plan, true, ['mtp']).map((r) => r.label)).toEqual(['Parts', 'novel']);
  });

  it("read one argument as its value's file name, several as flag and value", () => {
    expect(argsValue([arg('--mmproj', 'images', '/models/mmproj-F16.gguf')])).toBe('mmproj-F16.gguf');
    expect(argsValue([arg('--jinja', 'templates')])).toBe('--jinja');
    expect(argsValue([arg('-k', 'kv', 'q8_0'), arg('-v', 'kv', 'q8_0')])).toBe('-k q8_0 · -v q8_0');
  });

  it('are captioned by whose choices they are, in one order', () => {
    expect(chosenBy([arg('a', 'x', undefined, 'you'), arg('b', 'x', undefined, 'model_author')])).toBe('chosen by model author, you');
  });
});

describe('the Sampling row', () => {
  const unset = SAMPLING_KEYS.map((param) => ({ param, kind: 'unset' as const, label: 'unset by design' }));

  it('says the set values by where they came from, then counts the rest', () => {
    const sampling = samplingExplanation({
      resolved: resolvedConfig({ topP: 0.95, topK: 64 }),
      sources: [
        ...unset.filter((e) => e.param !== 'topP' && e.param !== 'topK'),
        { param: 'topP', kind: 'layer', layer: 'global', label: 'global settings' },
        { param: 'topK', kind: 'layer', layer: 'global', label: 'global settings' },
      ],
    });
    expect(samplingValue(sampling)).toBe("Top P 0.95 · Top K 64 from global settings; 13 others use llama.cpp's defaults");
  });

  it("gives a value the model's GGUF publishes, and gglib defers to, to the model author", () => {
    const sampling = samplingExplanation({
      sources: unset,
      published: [{ param: 'temperature', key: 'general.sampling.temp', state: 'deferred', published: 0.6 }],
    });
    expect(samplingValue(sampling)).toBe("Temperature 0.6 from model author; 14 others use llama.cpp's defaults");
  });

  it('is set by you only when a field sits on your per-model layer', () => {
    const own = samplingExplanation({ sources: [{ param: 'topP', kind: 'layer', layer: 'modelUserSet', label: 'per-model defaults (user-set)' }] });
    const global = samplingExplanation({ sources: [{ param: 'topP', kind: 'layer', layer: 'global', label: 'global settings' }] });
    expect(setByYou(own, SAMPLING_KEYS)).toBe(true);
    expect(setByYou(global, SAMPLING_KEYS)).toBe(false);
  });
});
