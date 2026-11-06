/**
 * The library's groups, search and sort, as pure functions: a model is in
 * one group; a search is words that must all match; a sort orders each
 * group, an unmeasured model last by speed either way.
 */

import { describe, it, expect } from 'vitest';
import {
  DEFAULT_SORT,
  libraryGroups,
  matchesDownload,
  matchesFar,
  matchesModel,
  sortModels,
} from '../../../src/places/library/groups';
import { guiModel } from '../fixtures/model';
import { runningRow } from '../fixtures/downloads';
import type { ModelInfo } from '../../../src/types/generated/ModelInfo';

const qwen = guiModel({ id: 1, name: 'Qwen3-8B', paramCountB: 8.19, quantization: 'Q4_K_M', architecture: 'qwen3', tags: ['chat', 'tools'], sizeBytes: 5, addedAt: '2026-01-02 00:00:00' });
const gemma = guiModel({ id: 2, name: 'gemma-3-12b', paramCountB: 12.2, quantization: 'Q5_K_M', imageInput: true, sizeBytes: 9, addedAt: '2026-03-01 00:00:00', benchmarkSummary: { latest_tg_tps: 40, best_tg_tps: 41 } as GgufModelBench });
const flux = guiModel({ id: 3, name: 'flux1-schnell', paramCountB: 12, imageFamily: 'flux1', sizeBytes: 7, addedAt: '2026-02-01 00:00:00', benchmarkSummary: { latest_tg_tps: null, best_tg_tps: 90 } as GgufModelBench });
const tiny = guiModel({ id: 4, name: 'Qwen3-0.6B', paramCountB: 0.6, sizeBytes: 1, addedAt: '2025-12-01 00:00:00' });

type GgufModelBench = NonNullable<ReturnType<typeof guiModel>['benchmarkSummary']>;

const names = (models: { name: string }[]) => models.map((m) => m.name);

describe('libraryGroups', () => {
  it('puts a model with a server in Loaded and nowhere else', () => {
    const groups = libraryGroups([qwen, gemma, flux], new Set([2]), '', DEFAULT_SORT);

    expect(names(groups.loaded)).toEqual(['gemma-3-12b']);
    expect(names(groups.here)).toEqual(['flux1-schnell', 'Qwen3-8B']);
  });

  it('filters both groups by the search', () => {
    const groups = libraryGroups([qwen, gemma, flux], new Set([2]), 'q4', DEFAULT_SORT);

    expect(groups.loaded).toEqual([]);
    expect(names(groups.here)).toEqual(['Qwen3-8B']);
  });
});

describe('matchesModel', () => {
  it('matches the name, quant, a tag and the architecture, ignoring case', () => {
    for (const q of ['qwen3-8b', 'Q4_K', 'TOOLS', 'qwen3']) expect(matchesModel(qwen, q), q).toBe(true);
    expect(matchesModel(qwen, 'llama')).toBe(false);
  });

  it('needs every word to match', () => {
    expect(matchesModel(qwen, 'chat tools')).toBe(true);
    expect(matchesModel(qwen, 'chat vision')).toBe(false);
  });

  it('matches what a model can do', () => {
    expect(matchesModel(gemma, 'vision')).toBe(true);
    expect(matchesModel(gemma, 'sees images')).toBe(true);
    expect(matchesModel(qwen, 'vision')).toBe(false);
    expect(matchesModel(flux, 'draws')).toBe(true);
    expect(matchesModel(flux, 'flux.1')).toBe(true);
    expect(matchesModel(gemma, 'draws')).toBe(false);
  });

  it('matches a size word by the parameter count, not the name', () => {
    expect(matchesModel(guiModel({ name: 'Llama-3.1-Instruct', paramCountB: 8.03 }), '8b')).toBe(true);
    expect(matchesModel(guiModel({ name: 'Llama-3.1-Instruct', paramCountB: 8.03 }), '7b')).toBe(false);
    expect(matchesModel(tiny, '0.6b')).toBe(true);
    expect(matchesModel(gemma, '12b')).toBe(true);
    expect(matchesModel(flux, '8b')).toBe(false);
  });
});

describe('matchesFar', () => {
  const far: ModelInfo = { id: 'Llama-3.3-70B', gglib_id: 7, quantization: 'Q4_K_M', object: 'model', created: 0, owned_by: 'gglib', description: 'big one', capabilities: ['vision'] };

  it("matches the far row's id, quant, description and vision", () => {
    for (const q of ['llama', 'q4_k_m', 'big', 'vision']) expect(matchesFar(far, q), q).toBe(true);
    expect(matchesFar(far, 'qwen')).toBe(false);
    expect(matchesFar({ ...far, capabilities: [] }, 'vision')).toBe(false);
  });
});

describe('matchesDownload', () => {
  it("matches a download by its row's name", () => {
    expect(matchesDownload(runningRow(), 'zeta q8')).toBe(true);
    expect(matchesDownload(runningRow(), 'qwen')).toBe(false);
  });
});

describe('sortModels', () => {
  it('sorts by name, recently added and size, either way', () => {
    expect(names(sortModels([qwen, gemma, flux], { key: 'name', ascending: true }))).toEqual(['flux1-schnell', 'gemma-3-12b', 'Qwen3-8B']);
    expect(names(sortModels([qwen, gemma, flux], { key: 'added', ascending: false }))).toEqual(['gemma-3-12b', 'flux1-schnell', 'Qwen3-8B']);
    expect(names(sortModels([qwen, gemma, flux], { key: 'size', ascending: true }))).toEqual(['Qwen3-8B', 'flux1-schnell', 'gemma-3-12b']);
  });

  it('sorts by measured speed with an unmeasured model last either way', () => {
    expect(names(sortModels([qwen, gemma, flux], { key: 'speed', ascending: false }))).toEqual(['flux1-schnell', 'gemma-3-12b', 'Qwen3-8B']);
    expect(names(sortModels([qwen, gemma, flux], { key: 'speed', ascending: true }))).toEqual(['gemma-3-12b', 'flux1-schnell', 'Qwen3-8B']);
  });
});
