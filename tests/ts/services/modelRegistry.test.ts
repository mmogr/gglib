/**
 * The model library, once: filled by `modelEvents` on init and after each
 * burst of `model_*` events, read by every surface through `modelRegistry`,
 * and re-read as soon as an edit made here has been answered.
 *
 * The order of subscribe, read and apply is `bridgeEvents`' and is held in
 * its own test; what is held here is what this registry adds: the batching
 * of a burst into one read, the failure kept beside the last list, the first
 * read ending `loading` either way, and the three actions reading again.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, renderHook } from '@testing-library/react';

import type { ModelEvent } from '../../../src/services/transport/types/events';
import { guiModel } from '../fixtures/model';

const transport = vi.hoisted(() => ({
  subscribe: vi.fn(),
  listModels: vi.fn(),
  addModel: vi.fn(),
  removeModel: vi.fn(),
  updateModel: vi.fn(),
}));

vi.mock('../../../src/services/transport', () => ({
  getTransport: () => transport,
}));

import {
  addModel,
  cleanupModelEvents,
  initModelEvents,
  onModelLibraryChange,
  refreshModels,
  removeModel,
  updateModel,
} from '../../../src/services/modelEvents';
import { getModelLibrary, useModel, useModelLibrary } from '../../../src/services/modelRegistry';

const REMOVED: ModelEvent = { type: 'model_removed', modelId: 9 };

/** Let every answered read run to its end. */
const settle = () => act(() => new Promise((resolve) => setTimeout(resolve, 0)));

describe('the model registry', () => {
  /** Send one event the way the stream would, to the latest subscription. */
  let deliver: (evt: ModelEvent) => void = () => {};
  const unsubscribe = vi.fn();

  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    unsubscribe.mockReset();
    transport.listModels.mockReset().mockResolvedValue([guiModel({ id: 1, name: 'llama-7b' })]);
    transport.addModel.mockReset().mockResolvedValue({});
    transport.removeModel.mockReset().mockResolvedValue(undefined);
    transport.updateModel.mockReset().mockResolvedValue({});
    transport.subscribe.mockReset().mockImplementation((_category: string, listener: (evt: ModelEvent) => void) => {
      deliver = listener;
      return unsubscribe;
    });
  });

  afterEach(() => {
    cleanupModelEvents();
    vi.useRealTimers();
  });

  it('is empty and loading until init reads the library, then holds what was read', async () => {
    const { result } = renderHook(() => useModelLibrary());
    expect(result.current).toEqual({ models: [], loading: true, error: null });

    act(() => initModelEvents());
    await settle();

    expect(transport.subscribe).toHaveBeenCalledWith('model', expect.any(Function));
    expect(result.current.loading).toBe(false);
    expect(result.current.models.map((m) => m.name)).toEqual(['llama-7b']);
  });

  it('keeps the last list and says why when a read fails, and is no longer loading', async () => {
    transport.listModels.mockRejectedValueOnce(new Error('daemon away'));
    const { result } = renderHook(() => useModelLibrary());

    act(() => initModelEvents());
    await settle();
    expect(result.current).toEqual({ models: [], loading: false, error: 'Failed to load models: daemon away' });

    await act(() => refreshModels());
    expect(result.current.error).toBeNull();
    expect(result.current.models).toHaveLength(1);
  });

  it('reads once for a burst of events, after the window, and tells its listeners once', async () => {
    const changed = vi.fn();
    act(() => initModelEvents());
    await settle();
    expect(transport.listModels).toHaveBeenCalledTimes(1);
    const off = onModelLibraryChange(changed);

    act(() => {
      deliver(REMOVED);
      deliver({ ...REMOVED, modelId: 10 });
      deliver({ ...REMOVED, modelId: 11 });
    });
    expect(transport.listModels).toHaveBeenCalledTimes(1);

    await act(() => vi.advanceTimersByTimeAsync(400));
    expect(transport.listModels).toHaveBeenCalledTimes(2);
    expect(changed).toHaveBeenCalledTimes(1);
    off();
  });

  it('a read an event overtook is dropped, and the burst brings the newer one', async () => {
    let answerFirst: (models: unknown[]) => void = () => {};
    transport.listModels.mockReturnValueOnce(new Promise((resolve) => { answerFirst = resolve; }));
    const { result } = renderHook(() => useModelLibrary());
    act(() => initModelEvents());

    act(() => deliver(REMOVED));
    transport.listModels.mockResolvedValueOnce([guiModel({ id: 9, name: 'gemma-3-12b' })]);
    await act(() => vi.advanceTimersByTimeAsync(400));
    act(() => answerFirst([guiModel({ id: 1, name: 'stale' })]));
    await settle();

    expect(result.current.models.map((m) => m.name)).toEqual(['gemma-3-12b']);
  });

  it('cleanup unsubscribes, drops a pending burst, and empties the library', async () => {
    const { result } = renderHook(() => useModelLibrary());
    act(() => initModelEvents());
    await settle();
    act(() => deliver(REMOVED));

    act(() => cleanupModelEvents());
    await act(() => vi.advanceTimersByTimeAsync(400));

    expect(unsubscribe).toHaveBeenCalledTimes(1);
    expect(transport.listModels).toHaveBeenCalledTimes(1);
    expect(result.current).toEqual({ models: [], loading: true, error: null });
  });

  it('adds, removes and updates through the transport, and reads again after each', async () => {
    act(() => initModelEvents());
    await settle();

    await act(() => addModel('/models/new.gguf'));
    expect(transport.addModel).toHaveBeenCalledWith({ filePath: '/models/new.gguf' });
    expect(transport.listModels).toHaveBeenCalledTimes(2);

    await act(() => removeModel(1));
    expect(transport.removeModel).toHaveBeenCalledWith(1);
    expect(transport.listModels).toHaveBeenCalledTimes(3);

    await act(() => updateModel(1, { name: 'renamed', projectorPath: null, components: { vae: '/v.safetensors' } }));
    expect(transport.updateModel).toHaveBeenCalledWith({
      id: 1,
      name: 'renamed',
      quantization: undefined,
      filePath: undefined,
      inferenceDefaults: undefined,
      serverDefaults: undefined,
      projectorPath: null,
      components: { vae: '/v.safetensors' },
    });
    expect(transport.listModels).toHaveBeenCalledTimes(4);
  });

  it('a failed edit rejects and reads nothing', async () => {
    act(() => initModelEvents());
    await settle();
    transport.removeModel.mockRejectedValueOnce(new Error('in use'));

    await expect(removeModel(1)).rejects.toThrow('in use');

    expect(transport.listModels).toHaveBeenCalledTimes(1);
  });

  it('useModel finds a row by id, and null for none or for no id', async () => {
    act(() => initModelEvents());
    await settle();
    expect(getModelLibrary().models).toHaveLength(1);

    const found = renderHook(() => useModel(1));
    const missing = renderHook(() => useModel(2));
    const none = renderHook(() => useModel(null));

    expect(found.result.current?.name).toBe('llama-7b');
    expect(missing.result.current).toBeNull();
    expect(none.result.current).toBeNull();
  });
});
