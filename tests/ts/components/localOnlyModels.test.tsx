/**
 * The paired machine's rows are `ModelInfo`, never this library's
 * `GgufModel`, so the surfaces that act on this machine's models — the
 * benchmark, the image model choice in Settings — cannot be handed one. That is
 * the compiler's to enforce: the lines marked `@ts-expect-error` below fail
 * `npm run typecheck` the day a far row becomes assignable to a local one.
 */

import { describe, it, expect } from 'vitest';
import type { ComponentProps } from 'react';

import type BenchmarkPage from '../../../src/places/diagnostics/benchmarks/BenchmarkPage';
import type { imageChoices } from '../../../src/places/settings/ModelsSection';
import type { GgufModel } from '../../../src/types';
import { farEntry } from '../fixtures/fakeFarDaemon';
import { guiModel } from '../fixtures/model';

type BenchmarkModels = ComponentProps<typeof BenchmarkPage>['models'];
type DefaultModels = Parameters<typeof imageChoices>[0];

describe('local-only model surfaces', () => {
  it('take this library’s models and refuse a far row', () => {
    const far = farEntry('qwen3', 3);
    const here: GgufModel = guiModel({ id: 3, name: 'qwen3' });

    const benchmark: BenchmarkModels = [here];
    const defaults: DefaultModels = [here];
    // @ts-expect-error a far row is not a model of this library
    const farBenchmark: BenchmarkModels = [far];
    // @ts-expect-error a far row is not a model of this library
    const farDefault: DefaultModels = [far];

    expect([benchmark, defaults, farBenchmark, farDefault].map((list) => list.length)).toEqual([1, 1, 1, 1]);
  });
});
