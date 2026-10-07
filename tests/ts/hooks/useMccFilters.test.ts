/**
 * What the library page asks the daemon for when a filter is set.
 *
 * The transport is the real one over a stubbed `fetch`, so what is read here
 * is the request line itself. The Context Length slider set `contextRange`
 * and nothing sent it: the hook built its own query string, beside the one
 * in the transport, and the two had drifted. There is one now, typed by the
 * binding of the struct the daemon parses the query into.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook, waitFor } from '@testing-library/react';

vi.mock('../../../src/services/platform', () => ({
  appLogger: { debug: vi.fn(), warn: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

import type { FilterState } from '../../../src/components/FilterPopover';
import { useMccFilters } from '../../../src/pages/modelControlCenter/useMccFilters';
import { listModels } from '../../../src/services/transport/api/models/local';

const fetchMock = vi.fn();

/** The path of every request sent so far, oldest first. */
const paths = (): string[] => fetchMock.mock.calls.map(([path]) => path as string);

/** The filters the page starts with: newest first, nothing narrowed. */
const UNFILTERED: FilterState = {
  sortBy: 'added_at',
  sortOrder: 'desc',
  paramRange: null,
  contextRange: null,
  speedRange: null,
  selectedQuantizations: [],
  selectedTags: [],
};

function mounted() {
  const refresh = vi.fn(async () => {});
  return renderHook(() =>
    useMccFilters({
      models: [],
      addModel: vi.fn(async () => {}),
      loadModels: refresh,
      refreshFilterOptions: refresh,
      loadTags: refresh,
    }),
  );
}

describe('the model list the library page asks for', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    fetchMock.mockImplementation(
      async () => new Response('[]', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('asks for the sort alone while no filter is set', async () => {
    mounted();

    await waitFor(() => expect(paths()).toEqual(['/api/models?sort=added_at&order=desc']));
  });

  it('sends the context-length range the slider sets', async () => {
    const { result } = mounted();
    await waitFor(() => expect(paths()).toHaveLength(1));

    act(() => result.current.onFiltersChange({ ...UNFILTERED, contextRange: [8192, 32768] }));

    await waitFor(() =>
      expect(paths()[1]).toBe('/api/models?sort=added_at&order=desc&min_context=8192&max_context=32768'),
    );
  });

  it('sends every filter that is set, by the name the daemon reads it under', async () => {
    const { result } = mounted();
    await waitFor(() => expect(paths()).toHaveLength(1));

    act(() =>
      result.current.onFiltersChange({
        sortBy: 'param_count',
        sortOrder: 'asc',
        paramRange: [1, 30],
        contextRange: [4096, 131072],
        speedRange: [10, 90],
        selectedQuantizations: ['Q4_K_M', 'Q8_0'],
        selectedTags: ['chat', 'code'],
      }),
    );

    await waitFor(() => expect(paths()).toHaveLength(2));
    const [path, search] = paths()[1].split('?');
    expect(path).toBe('/api/models');
    expect(Object.fromEntries(new URLSearchParams(search))).toStrictEqual({
      sort: 'param_count',
      order: 'asc',
      min_params: '1',
      max_params: '30',
      min_context: '4096',
      max_context: '131072',
      min_speed: '10',
      max_speed: '90',
      quantizations: 'Q4_K_M,Q8_0',
      tags: 'chat,code',
    });
  });

  it('asks again with the same filters once a model has been added', async () => {
    const { result } = mounted();
    act(() => result.current.onFiltersChange({ ...UNFILTERED, contextRange: [8192, 32768] }));
    await waitFor(() => expect(paths()).toHaveLength(1));

    await act(() => result.current.handleModelAdded());

    expect(paths()).toHaveLength(2);
    expect(paths()[1]).toBe('/api/models?sort=added_at&order=desc&min_context=8192&max_context=32768');
    expect(paths()[1]).toBe(paths()[0]);
  });
});

describe('the one model list request', () => {
  beforeEach(() => {
    fetchMock.mockReset();
    fetchMock.mockImplementation(
      async () => new Response('[]', { status: 200, headers: { 'content-type': 'application/json' } }),
    );
    vi.stubGlobal('fetch', fetchMock);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('is the bare list with no query, and leaves out a filter that is null', async () => {
    await listModels();
    await listModels({ sort: 'name', min_context: null, max_context: 0, tags: undefined });

    expect(paths()).toEqual(['/api/models', '/api/models?sort=name&max_context=0']);
  });
});
